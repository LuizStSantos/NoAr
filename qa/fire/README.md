# Local integrated fire test

This is a real, disposable 15-minute NoAr/GStreamer exercise, not a mocked API
test. It combines Scheduler browser operations, a synthetic playlist, four live
input protocols, deliberate signal failures, graphics and a playout restart.
Use it only on an isolated development machine with no real channels running.

Prerequisites: the built NoAr binary, native GStreamer 1.26.11 core/base/libav
(matching the Dockerfile), FFmpeg and FFprobe as independent test tools, Python 3.11+, MediaMTX, and the Chromium /
Playwright dependencies in `qa/e2e`. Keep the native processes in one network
namespace. The runner refuses occupied loopback ports 8787, 1935, 8888, 9997 TCP
and 8890 / 5004 UDP. MediaMTX uses the supplied loopback-only configuration.

Create a fresh directory with the ownership marker; existing databases are
refused. Point the frontend preview proxy at this backend on port 8787 and serve
the already-built production frontend on `127.0.0.1:5757`. Run the two commands
concurrently, using the same directory:

```bash
mkdir -p /tmp/noar-fire-example
touch /tmp/noar-fire-example/.qa-fire-owned
NOAR_E2E_STAGING=1 NOAR_E2E_DISPOSABLE=1 python3 qa/fire/run.py \
  --binary /absolute/path/to/noar \
  --mediamtx /absolute/path/to/mediamtx \
  --work-root /tmp/noar-fire-example
```

```bash
node qa/fire/browser.cjs /tmp/noar-fire-example
```

The browser logs into the actual application with the disposable account written
to `auth.json` (mode 0600). It verifies the channel's storage before any writes,
creates the SRT live and temporary graphics event in Scheduler, toggles the
channel logo, and takes a lower third while the video is in L-shape. Do not copy
`auth.json` into reports or commit the fixture database. The runner permits one
minute for browser provisioning and starts playout at the configured day start.
Define the output size before authoring the test models: compatibility settings
DTOs represent logo geometry in pixels, while Composer models use percentages.
The native runner also starts `observe.py`, recording temporary/permanent model
state, data-source rows, and backend RSS, threads, open descriptors and CPU samples.
CPU percentages count one fully occupied logical core as 100%; these observations
are not a capacity benchmark or a long-term memory leak qualification.

The programme uses mixed dimensions/frame rates and a deliberately silent clip.
Its live source has a distinctive green marker and an 880 Hz tone. MediaMTX
receives the SRT publisher and exposes the same source to SRT, RTMP and HLS
clients. A separate publisher sends UDP. The commercial has its own colour and
550 Hz tone. These identifiers let the checks distinguish the requested source
from a healthy-looking output which is actually showing the wrong programme.

The timeline includes:

- SRT live at 30–210 seconds, with a publisher interruption at 90–110 seconds;
- logo replacement and movement, L-shape with a browser-operated lower third,
  a scheduled notice at 150–190 seconds, and an update to CSV-backed crawl data;
- return to playlist, a clip-specific logo / hide interval / L-shape artwork;
- RTMP live at 270–390 seconds, with a scene and return to full screen;
- a scheduled commercial at 395 seconds and UDP live at 420–570 seconds,
  including an audio-only source at 480–510 seconds;
- a real playout restart at 630 seconds, HLS live at 750–810 seconds, and five
  short lower thirds near the end.

`PictureMonitor` independently decodes a fixed video-only region. It excludes
the clock, scrolling text, logo and lower third, so graphics animation cannot
conceal a frozen video. Ordinary missing/still pictures fail after 12 seconds.
Explicit, bounded grace windows cover cold start (15 seconds), SRT source loss
(40 seconds), the audio-only fault (50 seconds) and restart (45 seconds). HLS
must keep advancing outside startup/restart. These windows are test policy,
not measured recovery times or a promise of seamless fault handling.

The runner archives original completed TS segments, SHA-256 hashes, health and
programme samples, source/backend/decoder logs, UI receipts and actual HLS
frames. Pixel checks inspect source markers, logo geometry and the GC inside
the video window; colour tolerance is 25/255 per RGB channel for compression.
Short clip marks are checked over consecutive decoded frames with their expected
artwork and video regions, since a wall-clock snapshot can land in a transition.
An older snapshot assertion is retained as `PASS_WITH_CAPTURE_RECHECK` only when
that decoded sequence and every other final check pass; its original incident
and raw runner status remain in the report. This is not a frame-parity test of
transition timing. The restart creates a separate output generation; timestamp continuity across
that intentional restart is not asserted. The browser saves dark desktop and
pt-BR desktop/phone screenshots. The phone screenshot is a full-page capture.

After the complete run, execute the independent final checks:

```bash
python3 qa/fire/analyse.py /tmp/noar-fire-example
```

The analyser requires the full cycle and browser receipts, admission of every
programme event, successful decode, strictly increasing DTS, at most 120 ms
between packets of each elementary stream, A/V tails within 250 ms per output
generation, and original TS continuity inside each segment without transport
errors. Native splitmuxsink starts a new muxer per file: counter restarts between
files are retained as metrics rather than treated as lost packets. It checks the
880 Hz live tone / 550 Hz commercial tone and audible RMS in selected archived
segments. Review every logged error and the intentionally induced fault windows
before signing off; a process exit code alone is insufficient.

For a shorter actual-server regression of changing UDP tracks, use a separate
fresh owned directory and add `--scenario udp-tracks` to `run.py`. This 90-second
scenario needs no browser: UDP A/V enters at 5 seconds, becomes audio-only at 25,
and regains video at 55. It checks HLS picture progress and the returned green
source marker. It is a focused check, not completion of the full fire test.
`udp_tracks.py uridecodebin3 <owned-directory>` separately diagnoses the native
track switch. Version 1.26.2 aborts; the 1.26.11 runtime from the Docker image
must survive. Record the native version alongside the result.

This local exercise does **not** qualify physical SDI/NDI devices, desktop audio,
WAN packet loss, output RTMP/SRT/UDP, multi-channel capacity or a 24-hour service.
For longer continuity coverage, use the separate [`../soak`](../soak/) harness.

For an additional post-run monitor proof, keep the owned backend available on
8787 and supply the synthetic UDP source on 5004, then run
`node qa/fire/monitor.cjs <owned-directory>`. It verifies storage ownership,
creates a temporary live, takes a uniquely labelled GC, waits for two presented
video frames on desktop and phone, and saves dark pt-BR screenshots. It stops
playout and removes its temporary event afterward. This separate proof does not
replace or rewrite the original 900-second report.
