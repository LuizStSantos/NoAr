**SCTE-35 study**
================

*Status: step 2, SCTE-35 on the SRT/UDP output, is done, with breaks marked
inside videos since NoAr 1.2; how to use it is in
[Ad breaks (SCTE-35)](ad_breaks.md). Written for NoAr 1.0.0 (October 2026) to
decide whether and in what order NoAr should signal and read ad breaks, and
kept up to date with the decisions taken since: one fixed signal from NoAr,
and per-platform profiles in a [fork of MediaMTX](#mediamtx).*

## In short

- **SCTE-35** is the standard message that says "a break starts here, for this
  long" (and "the programme is back") inside a video stream. FAST platforms,
  server-side ad insertion (SSAI) services, cable headends and affiliates use
  it to replace or insert ads at the right frame.
- NoAr already knows where its breaks are: commercials in the playlist and in
  commercials folders, scheduled breaks, stretches without logo inside a
  video, the logo API. What it lacks is a way to **write** that knowledge into
  its outputs, and to **read** it from the lives it receives.
- FFmpeg can **read** SCTE-35 from an incoming MPEG-TS. Mainline FFmpeg cannot
  **write** it: its MPEG-TS muxer has no SCTE-35 support, and its HLS muxer
  writes no ad markers. NoAr would have to produce both itself; this is
  feasible without patching FFmpeg.
- Recommended order, cheapest first:
  1. read SCTE-35 from lives, to take the logo off and log the break;
  2. write it into the SRT/UDP MPEG-TS output;
  3. write ad markers into the HLS output;
  4. pass breaks through from a live, and fill them with NoAr's own
     commercials.

  If the goal is a FAST channel or SSAI, steps 2–3 come first.

**Decided since** (October 2026), for a channel going to Pluto TV and Samsung
TV Plus through MediaMTX:

- NoAr always sends the **same signal**, the most widely accepted one
  (below). It does not know which platform the stream goes to.
- Whatever one platform needs on top, its HLS tags, segment length or
  command type, is done by a **fork of MediaMTX**, one profile per platform.
- Step 2 came first and is done: [Ad breaks (SCTE-35)](ad_breaks.md).

## What SCTE-35 is

ANSI/SCTE 35 ("Digital Program Insertion Cueing Message") defines one binary
message, the **splice_info_section** (`table_id` 0xFC), with a CRC-32 at the
end. Each message carries one command:

| Command | Use |
| --- | --- |
| `splice_null` | A heartbeat with no meaning, so a receiver knows the signal is alive. |
| `splice_insert` | The classic cue: `out_of_network_indicator` = 1 for "leave the programme" (break starts) and 0 for "return"; a `splice_time` (or *immediate*); optionally a `break_duration` with `auto_return`; an event ID; and `avail_num`/`avails_expected`. |
| `time_signal` | Only a time; what happens is said by **segmentation descriptors** attached to it. This is the direction the industry is moving to, because it can describe more than ad breaks. |

A **segmentation descriptor** carries a `segmentation_type_id`. Even values
start a segment and odd values end it. The ones that matter for breaks:

| Type | Meaning |
| --- | --- |
| 0x30 / 0x31 | Provider advertisement start / end: the content owner's ads. |
| 0x32 / 0x33 | Distributor advertisement start / end: the distributor's ads. |
| 0x34 / 0x35 | Provider placement opportunity start / end: a break someone else may fill. |
| 0x36 / 0x37 | Distributor placement opportunity start / end. |
| 0x10 / 0x11 | Program start / end (and other program and chapter types). |

Times are in the **90 kHz PTS clock of the stream itself**, plus a
`pts_adjustment`. A cue is normally sent some seconds **before** its
`splice_time` (pre-roll), so downstream systems can prepare the ads. The
encoder should also put a key frame (IDR) at the splice point, so the switch is
clean.

## How it travels

| Transport | How | Notes |
| --- | --- | --- |
| **MPEG-TS** (SRT, UDP, RIST, files) | Sections on their own PID. The PMT lists that PID with `stream_type` 0x86, and has a `CUEI` registration descriptor at program level. | The native form. SRT and UDP carry it as plain TS packets. |
| **HLS** | Tags in the media playlist. `#EXT-X-DATERANGE` with `SCTE35-OUT` / `SCTE35-IN` (the message in hex) and `PLANNED-DURATION` is the standard form. The older `#EXT-X-CUE-OUT:<duration>`, `#EXT-X-CUE-OUT-CONT:<elapsed>/<duration>` and `#EXT-X-CUE-IN` are still widely expected. | SSAI services such as AWS MediaTailor accept both forms; each has its own rules. Breaks should start and end on segment boundaries. |
| **RTMP / FLV** | No standard; some platforms use AMF `onCuePoint` data messages. | Vendor-specific; not worth doing until a destination asks for it. |

## What NoAr has today

**Outputs.** The engine encodes with FFmpeg's libraries and muxes with:

- the `mpegts` muxer for SRT and UDP (and the Player's preview);
- the `hls` muxer for HLS. NoAr manages its segments (`delete_segments`,
  `temp_file`, `append_list`), and `program_date_time` can be turned on;
- `flv` for RTMP.

**What FFmpeg does with SCTE-35** (checked against FFmpeg's `master` in
October 2026):

- *Demuxing*: an MPEG-TS stream with `stream_type` 0x86 and a `CUEI`
  descriptor becomes a data stream with codec `AV_CODEC_ID_SCTE_35`. Each
  splice_info_section arrives as one packet, timestamped with the last PCR. So
  NoAr can read cues from a live with the libraries it already links.
- *Muxing*: `mpegtsenc.c` has no SCTE-35 handling. A data stream would be muxed
  as private PES, which breaks SCTE-35. Patches to fix this were posted in
  2021, 2023 and 2025 and are not merged. NoAr builds against Debian's
  FFmpeg, so a patched FFmpeg is not an option.
- *HLS*: `hlsenc.c` writes no CUE or DATERANGE tags.

**Where NoAr knows breaks are**, with their exact output time:

- clips marked as commercials, and clips from [commercials folders](commercials.md),
  in playlists and in scheduled events;
- scheduled events made only of commercials;
- stretches without logo inside a video ([logo](logo.md));
- calls to the logo API, when an operator or an encoder hides the logo for a
  break inside a live;
- a scheduled live starting and ending (programme start and end).

## How it could fit into NoAr

### One break model

The engine gets a small queue of **cues**: start in output video PTS, a
duration if known, an ID, and the kind (provider or distributor, ad or
placement opportunity). Every output writer reads from it.

The app fills the queue:

- a run of consecutive commercials is one break, with its total duration. It
  is known before it starts, because the playlist and the scheduler already
  know what comes next; the preloader already looks ten seconds ahead;
- a scheduled commercial event is one break;
- stretches without logo can optionally count as breaks;
- a new API call (`POST /api/control/{id}/cue` with a duration, and the
  keyed route for encoders) inserts a break by hand;
- cues read from a live (below) can be passed through.

At each cue's start and end, the encoder forces a key frame.

### MPEG-TS output (SRT, UDP)

Since FFmpeg cannot write the sections, NoAr writes them **between the muxer
and the network**. The muxer's output already goes through an AVIO context
that NoAr opens; a thin layer there:

1. watches the 188-byte TS packets that pass;
2. rewrites the **PMT**: it adds an elementary stream with `stream_type` 0x86
   on a free PID, and the `CUEI` registration descriptor, then recomputes
   the CRC;
3. inserts the **splice_info_section** packets for each cue on that PID, with
   their own continuity counter, a few seconds before the splice time when
   the break is known in advance. Optionally it also inserts `splice_null`
   heartbeats.

The message itself would come from a crate:
[`scte35-splice`](https://docs.rs/scte35-splice) parses and builds
splice_info_section (SCTE 35 2023r1) and recomputes the CRC.

The delicate part is the clock. The splice time must be in the PTS of the
TS that leaves NoAr, and the muxer may offset timestamps (its `max_delay`).
The layer should read the video PES PTS it sees and place cues against those,
rather than trust the encoder's timeline.

### HLS output

Two steps:

1. **Tags**: after the `hls` muxer writes a media playlist, NoAr adds
   `#EXT-X-DATERANGE` (with `SCTE35-OUT`/`SCTE35-IN` and `PLANNED-DURATION`),
   and optionally the CUE-OUT/CUE-IN form, before the segments where breaks
   start and end. The playlist is written through `temp_file` and renamed, so
   NoAr can rewrite it the same way. `program_date_time` must be on for
   DATERANGE.
2. **Segment boundaries**: SSAI services expect a break to start on a new
   segment. The `hls` muxer only splits at a key frame once `hls_time` has
   passed, so a forced key frame alone does not start a segment. This may need
   NoAr to cut segments itself (for example with the `segment` muxer and
   playlists written by NoAr). It is the largest piece of work in this study.

A first version can do step 1 only, with breaks rounded to the nearest
segment, and still work with players and services that accept that.

### Reading SCTE-35 from lives

For scheduled lives and live ingest over SRT/UDP:

1. While playing a live, the engine also reads its `AV_CODEC_ID_SCTE_35`
   stream (today it keeps only the best video and audio stream), parses each
   message, and turns its splice time into output time. The live input
   already keeps a mapping from source to output timestamps.
2. What NoAr does with a break is set per live:
   - **take the logo off** for the break (the logo control already exists);
   - **record it in the report** (as-run): break inside the live, from, to;
   - **pass it through** to NoAr's own outputs (needs the output side);
   - **fill it with NoAr's commercials** (local ad insertion): play a
     commercials folder for the break's duration and return to the live. The
     preloader can keep the live connected meanwhile, as it already does
     before a live starts.

Software encoders often send no SCTE-35 at all; check what yours does. The
[logo API](logo.md) covers them, and could also emit a cue.

### Settings and interface

- **Settings › Output**: SCTE-35 on or off; `splice_insert` or `time_signal`
  with a segmentation type; pre-roll in seconds; the PID; and which breaks are
  signalled (commercials, scheduled breaks, stretches without logo, API).
- **Lives** (Storage and live ingest listeners): what to do with an incoming
  break.
- **Report**: each cue with its ID, so the as-run matches what downstream saw.

## The signal NoAr sends

Settled with the first implementation; [Ad breaks](ad_breaks.md) has the
details users need.

- `splice_insert`, program splice, never immediate, `pts_adjustment` 0, tier
  0xFFF.
- Out of network at the first frame of the break, with `break_duration` and
  `auto_return`; back into the network at the first frame after it, with the
  same `splice_event_id`. A break called off after its announcement gets a
  `splice_event_cancel_indicator`.
- Each message goes the pre-roll ahead (6 s by default, SCTE 67 asks 4 to
  8 s), and twice more a second apart.
- Both splice frames are IDR. The splice times are the PTS the muxer wrote
  for those frames: NoAr measures the muxer's offset (its delay and the shift
  for AAC priming) by matching each video packet it hands over with the PES
  that comes out.
- Breaks are runs of commercials, 30 to 300 s by default, plus breaks marked
  by hand.

Making the splice exact needed one change in the engine: a clip's last audio
frame used to run past the clip's end, and the video padded a frame to cover
it, so every clip lasted a frame longer than planned and a break announced
ahead started a frame early. The last audio frame is now cut at the clip's
end, and clips last exactly their duration.

## Platforms

What the two platforms in view ask for, as far as public documents tell. Ask
each one for its current written specification before relying on it.

**Pluto TV** ([live stream specification v2.2](https://www.paramount.com/pluto-tv-live-stream-specifications)):

- Pluto pulls HLS from a URL, with 5 s segments of steady length and a list of
  10. Its guidelines also name RTMP, which FFmpeg cannot carry SCTE-35 in.
- Video H.264, 1080p at 5–10 Mbps (or 720p), 30 fps, 16:9. Audio AAC-LC
  stereo, 48 kHz, 96 kb/s, loudness to EBU R128.
- SCTE-35 both in the playlist (`#EXT-X-CUE-OUT`/`#EXT-X-CUE-IN` and
  `#EXT-OATCLS-SCTE35`) and in the transport stream of the segments.
- Breaks of 30 to 300 s, at least four segments between CUE-OUT and CUE-IN,
  and a duration that does not change once CUE-OUT is out.
- Streams are tested before they go on the platform; a third party delivering
  the stream needs Pluto's written approval.

**Samsung TV Plus** has no public specification. Channels usually come in
through Samsung's content team or a partner (Amagi, Wurl, Frequency), and
frame-accurate SCTE-35 is expected. Its profile waits for the written
specification.

## MediaMTX

A stock MediaMTX cannot carry these breaks on. Checked against MediaMTX and
`bluenviron/mediacommon` in October 2026:

- MediaMTX does not pass an incoming MPEG-TS through. It splits it into
  tracks and muxes them again for each reader.
- Its MPEG-TS reader knows video, audio, DVB subtitles and KLV. A
  `stream_type` 0x86 track is "unsupported" and dropped, with a "skipping
  track" line in the log.
- Its HLS muxer (`gohlslib`) writes no ad markers at all.

So everything that comes out of MediaMTX, HLS, SRT, RTMP or WebRTC, has no
breaks. The plan is a fork of the three libraries (MIT licensed), offered
upstream so it need not be kept apart for long:

1. **mediacommon**: read and write an SCTE-35 track: sections, not PES, on
   their PID, with the `CUEI` registration.
2. **mediamtx**: a track type for it, like KLV, passed on to SRT, UDP and
   MPEG-TS readers, with the splice times moved along when MediaMTX shifts
   timestamps.
3. **gohlslib**: a segment starts at each splice point; the playlist gets the
   tags of the path's profile; the SCTE-35 track stays in the `.ts` segments
   when the profile asks for it.

Each MediaMTX path then has a profile: Pluto's (5 s segments,
`CUE-OUT`/`CUE-IN` with `OATCLS`), one with `EXT-X-DATERANGE`, one that turns
`splice_insert` into `time_signal` with segmentation descriptors, and
Samsung's once known.

```
NoAr ──SRT (MPEG-TS + SCTE-35)──▶ MediaMTX (fork)
                                     ├─ HLS, Pluto profile ─▶ nginx/CDN ─▶ Pluto TV
                                     ├─ HLS, Samsung profile ─▶ nginx/CDN ─▶ Samsung TV Plus
                                     └─ SRT with SCTE-35 (a platform or partner that takes SRT)
```

## Testing

- [TSDuck](https://tsduck.io): `tsp -I srt … -P splicemonitor` shows every
  splice command in a TS. `-P spliceinject` injects cues into a test source
  for the reading side.
- [threefive](https://pypi.org/project/threefive/) (Python) decodes SCTE-35
  from TS, HLS playlists, hex and base64.
- [x9k3](https://pypi.org/project/x9k3/) is an SCTE-35-aware HLS segmenter, a
  reference for the HLS tags.
- `ffprobe -show_streams` lists a `scte_35` data stream when the PMT is right.

## Rough effort

| Step | Scope | Size |
| --- | --- | --- |
| 1. Read from lives | Demux, parse, map times, hide the logo, as-run | Small–medium |
| 2. TS output (**done**) | Cue queue, key frames, PMT rewrite, section packets, settings, cue API | Medium |
| 3. HLS output | Playlist tags; segment cutting controlled by NoAr | Medium–large |
| 4. Pass-through and local ad insertion | Joining 1 with 2–3; playing commercials over a live | Medium |

## Open questions

- **Where does the channel go?** Answered: Pluto TV and Samsung TV Plus,
  through MediaMTX. Their written specifications decide the MediaMTX
  profiles.
- **Which breaks are ads?** Answered for now: every run of commercials of 30
  to 300 s, and breaks marked by hand.
- **Do the lives NoAr receives carry SCTE-35?** If they come from vMix or OBS,
  most likely not, and step 1 waits until a source sends cues.

## Sources

- [FFmpeg `libavformat/mpegts.c`](https://github.com/FFmpeg/FFmpeg/blob/master/libavformat/mpegts.c),
  [`mpegtsenc.c`](https://github.com/FFmpeg/FFmpeg/blob/master/libavformat/mpegtsenc.c),
  [`hlsenc.c`](https://github.com/FFmpeg/FFmpeg/blob/master/libavformat/hlsenc.c)
- FFmpeg patches for writing SCTE-35 in TS:
  [2021](https://patchwork.ffmpeg.org/project/ffmpeg/patch/2a3fb2cb-1f37-a3f5-950a-8e6c943f3fd4@m1stereo.tv/),
  [2023](https://ffmpeg.org/pipermail/ffmpeg-devel/2023-July/312420.html),
  [2025](https://ffmpeg.org/pipermail/ffmpeg-devel/2025-June/344978.html)
- [AWS MediaTailor: HLS supported ad markers](https://docs.aws.amazon.com/mediatailor/latest/ug/hls-ad-markers.html)
- [Broadpeak: SCTE-35 markers](https://developers.broadpeak.io/docs/dai-scte35)
- [THEOplayer: scheduling breaks](https://theoplayer.com/docs/theoads/how-to-guides/scheduling-breaks)
- [Bitmovin: SCTE-35](https://bitmovin.com/?p=275713)
- [AWS MediaLive: ad avail mode](https://docs.aws.amazon.com/medialive/latest/ug/getting-ready-set-the-ad-avail-mode.html)
- [`scte35-splice` crate](https://docs.rs/scte35-splice), [`scte35-reader` crate](https://docs.rs/scte35-reader)
- [Pluto TV live stream specification](https://www.paramount.com/pluto-tv-live-stream-specifications),
  [additional content delivery guidelines](https://www.paramount.com/pluto-tv-additional-content-delivery-guidelines)
- [Frequency: dynamic ad insertion](https://docs.frequency.com/en/distribution-specifications/dynamic-ad-insertion.html)
- [MediaMTX](https://github.com/bluenviron/mediamtx) (`internal/protocols/mpegts/to_stream.go`),
  [mediacommon](https://github.com/bluenviron/mediacommon) (`pkg/formats/mpegts/track.go`)
- [threefive](https://pypi.org/project/threefive/), used to check NoAr's output
