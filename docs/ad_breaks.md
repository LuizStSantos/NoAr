**Ad breaks (SCTE-35)**
================

NoAr marks in its stream where each commercial break starts and how long it
lasts, with SCTE-35. Whoever receives the channel, for example a FAST platform
(Pluto TV, Samsung TV Plus), a server-side ad insertion service or an
affiliate, can then replace or insert commercials at the exact frame.

NoAr only marks the stream. What happens in the break is decided by the
receiver; if it inserts nothing, viewers see what NoAr airs, so a break always
has commercials of its own.

## What it needs

- An **SRT or UDP output** (Settings › Output). SCTE-35 travels inside the
  MPEG-TS next to the video. HLS and RTMP outputs carry no SCTE-35 yet; with
  them the page says so and nothing is marked.
- A receiver that keeps the SCTE-35 track. **A stock MediaMTX drops it** (it
  only keeps the video, audio, subtitle and KLV tracks it knows), so a
  MediaMTX in the path loses every break. See [the study](scte35.md#mediamtx)
  for the plan to fork it.

## Turning it on

**Settings › Ad breaks (SCTE-35)**: switch on **Mark the breaks** and save.
Saving restarts the playout.

| Setting | Default | |
| --- | --- | --- |
| Announce ahead | 6 s | How long before a break its first message goes out. SCTE 67 recommends 4 to 8 s. |
| Shortest break / Longest break | 30 s / 300 s | Runs of commercials outside this range are not marked, and the log says so. Pluto TV accepts 30 to 300 s. |
| PID | 500 | The MPEG-TS track the messages go on. It must not be one the muxer uses. |

## Which breaks are marked

- **Runs of commercials**: clips in a row that are commercials, either marked
  in the playlist or taken from a [commercials folder](commercials.md), in the
  daily playlists and in scheduled events. One run is one break, as long as
  all its clips together.
- The break is **announced during the clip before it**, the pre-roll ahead.
  If that clip is skipped or cut by a scheduled event, the break is called
  off; when the commercials do come, they open a break of their own.
- A run that opens the playlist or a scheduled event has no clip before it, so
  its break is announced as it starts, without pre-roll.
- **Manual breaks**, from the Player, Settings or the API, start the pre-roll
  from now. They only mark the stream: what airs meanwhile is still the
  programme, as in a live whose encoder goes to commercials.

- **Breaks marked inside a video**, where a break was edited into a
  programme file: see below.

Stretches without logo inside a video and the logo API are not breaks; a
stretch becomes a break only when it is marked as one.

### Breaks marked inside a video

When a programme file has its commercial break edited in, mark where the
programme leaves and where it comes back, the same way as the logo: in
**Storage**, open the video's **Trim, logo and breaks in this video** (or the
same button in the Player's clip editor), choose **SCTE-35 breaks**, and while
it plays press **Break starts here** and **Back from the break here**, or
type the times. Each break then has two handles on the bar, as the
[trim](trim.md) does: drag them, or select one and use the arrow keys (Shift
moves a second); a break cannot run into the next. When the logo is already
marked off there, **Use the stretches without logo** copies them.

- Up to 16 breaks per video, in seconds of the video itself. They are kept
  with the logo stretches in `<name>.logo.json`, as `"breaks"`, and count
  wherever the video plays: the playlist and the Scheduler, from the next
  time it starts.
- Each one is a break like the others: announced the pre-roll ahead (three
  copies), with a key frame at the splice points, and shown in the Player and
  in Settings › Ad breaks. A break that starts less than the pre-roll into
  the clip is announced as it starts. In the status and the
  [API](api.md) their `source` is `marked` (the others are `commercials` and
  `manual`).
- When the playlist plays only part of the video, only the breaks in that
  part count; one that runs past the end of the clip ends with it. When the
  clip is cut short (skipped, interrupted by a scheduled event), the breaks
  still to come are called off.
- A marked break that overlaps another break (a run of commercials still on
  air, a manual break) is not signalled; the log says so.
- Commercials themselves never carry marked breaks: they are already part of
  a break.

## What NoAr sends

Every break is marked the same way, the form platforms and ad servers accept
most widely (SCTE 35 with the SCTE 67 practice). Adapting it to one platform
is left to what receives the stream.

**In the transport stream**

- The SCTE-35 messages travel on their own PID (500 unless set otherwise)
  with `stream_type` 0x86.
- The PMT lists that PID, with the `CUEI` registration descriptor for the
  programme and a `cue_identifier_descriptor` for the stream.

**For each break**

| When | Message |
| --- | --- |
| Pre-roll before the break, then 1 s and 2 s later | `splice_insert`, out of network: the splice time, the break duration with auto return, and an event ID. |
| Pre-roll before the end, likewise three times | `splice_insert` back into the network, at the end, with the same event ID. |
| A break called off after it was announced | `splice_insert` with `splice_event_cancel_indicator`. |

The three copies of a message are identical; receivers keep the first. Event
IDs count up from the clock at each server start, so they do not repeat.

**On the picture**

The first frame of the break and the first frame after it are key frames
(IDR). The splice times point at those frames, so a receiver cuts exactly
there. Hardware encoders get `forced-idr` for it.

The other details: `splice_insert` for the whole programme, no
`splice_immediate`, `pts_adjustment` 0, tier 0xFFF, `unique_program_id` 1.

## Following it

- **Player** (Live view): under the clip on air, an SCTE-35 strip shows the next break
  and how far it is, whether its message is already out ("announced 2/3"), or
  the break on air and when the programme returns. It can call off the next
  break, or mark one by hand with a length from the list.
- **Settings › Ad breaks**: the state (marking on PID 500, waiting for the
  playout, or why nothing is marked), how NoAr marks a break, and a table of
  the breaks to come, on air and the last 20. For each break it shows the
  messages sent at the start and at the end, and whether the frames at both
  splice points went out as key frames. The page updates by itself.
- The log has a line for each break scheduled, on air, ended or called off.

## API

| Method | Endpoint | Access | |
| --- | --- | --- | --- |
| `GET` | `/api/control/{id}/scte35` | `GA, CA, U` | The state, the breaks to come and on air, and the last 20. |
| `POST` | `/api/control/{id}/scte35/break` | `GA, CA, U` | `{ "duration": 120 }`: mark a break of that many seconds, from now plus the pre-roll. |
| `DELETE` | `/api/control/{id}/scte35/break/{event_id}` | `GA, CA, U` | Call off a break that has not started. |
| `GET`, `POST` | `/scte35/{id}/{break\|cancel}` | SCTE-35 key | Query `key` (or header `X-Scte35-Key`); `duration` (seconds) for `break`; `event` for `cancel`, else the next break that has not started. Answers with the state, as `GET /api/control/{id}/scte35`. |

```bash
curl -X POST http://127.0.0.1:8787/api/control/1/scte35/break \
  -H 'Authorization: Bearer <ACCESS_TOKEN>' \
  -H 'Content-Type: application/json' \
  --data '{"duration": 120}'
```

### From an encoder or an automation, with the SCTE-35 key

The `/scte35/...` route is for switchers, encoders and automation that cannot
log in (vMix, OBS, Bitfocus Companion, a playout system), like the
[logo key](api.md#logo-on-air). Create the key in **Settings › Ad breaks ›
Breaks by API** and save; the tutorial there lists the addresses ready to
copy, with examples for each tool. Without a key, or with a wrong one, it
answers 403. Such a break is a manual one: it starts after the pre-roll and
must last between the shortest and the longest break set.

```bash
curl -X POST 'http://127.0.0.1:8787/scte35/1/break?key=<KEY>&duration=120'
curl -X POST 'http://127.0.0.1:8787/scte35/1/cancel?key=<KEY>'
```

Times in the answers are Unix milliseconds. A break is `scheduled`,
`announced` (its message is out), `on_air`, `ended` or `cancelled`.

## Checking the stream

Any of these, on a recording of the output or on the stream itself:

```bash
# The PMT lists an scte_35 data stream.
ffprobe -v error -show_streams -of compact=p=0 capture.ts | grep scte_35

# Every message, decoded (pip install threefive).
threefive capture.ts

# TSDuck: splice commands as they pass.
tsp -I srt --caller 127.0.0.1:9000 -P splicemonitor -O drop
```

To record a UDP output for a while: `ffmpeg -i udp://127.0.0.1:41234 -c copy
-t 120 capture.ts`.

## Not yet

- HLS and RTMP outputs (see the [study](scte35.md)).
- Reading SCTE-35 from the lives NoAr receives.
