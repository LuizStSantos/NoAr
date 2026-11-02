# External live sources

NoAr uses GStreamer as its playback engine. It reads live sources from external
servers such as MediaMTX; it does not accept encoder publications or start RTMP
or SRT input listeners. There is no Live ingest settings page.

Publish the encoder to MediaMTX first, then create a `.live` source in NoAr
containing the URL that reads that publication. Use that source in a live event
on the Schedule page. The existing event settings control its start, duration,
source checks, retries and return to the programme in Clock or Continue mode.

For example, with a MediaMTX path named `studio`:

```text
srt://mediamtx:8890?streamid=read:studio
```

The hostname must resolve from the NoAr container. `localhost` inside that
container refers to NoAr itself. When MediaMTX is on another machine, use its
reachable hostname or IP address. SRT requires UDP connectivity to MediaMTX.
RTMP and RTSP client URLs are also supported when their native plugins exist.
The Scheduler also accepts HLS HTTP(S) and UDP MPEG-TS URLs. Playback uses
GStreamer's `uridecodebin3` and adaptive `hlsdemux2`; `audiorate` normalizes
incoming audio timestamps before the common playout timeline. It preserves
continuous samples through small FLV/PCR timestamp differences while normal
source-loss and output continuity handling remain active.
The Scheduler explicitly marks live events during preloading, including HTTP
URLs. This keeps HLS at the live edge without treating ordinary HTTP video
files as live or losing the event's live context when an input is reopened.

NoAr's SRT client defaults to caller mode. Do not put `mode=listener`, `listen=1`
or RTMP listener options in a live source. For saved SRT URL options, legacy
latency values remain in microseconds and are converted to GStreamer's units.
Stream IDs and passphrases are URL-encoded when passed to the native plugin.

The output encoder, muxer and transport stay open while input pipelines change.
If a live source fails, the scheduler regains control to run its waiting and
retry policy. An unavailable output reconnects independently of source changes.

Existing databases retain historical input settings and aired-report rows.
Those settings are not returned by the configuration API, changed by ordinary
settings saves, or activated at startup. Reports can still show historical
entries identified as ingest. No destructive database migration is required.

The independent Python experiment in `qa/gstreamer` is useful continuity
reference evidence. It is not a substitute for testing the integrated NoAr
scheduler, API, database and frontend with disposable data.
