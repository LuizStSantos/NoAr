# Soak test

`soak.py` keeps NoAr on air for a long time (90 minutes by default) with
synthetic media and the faults that have taken channels off air before, and
fails when the transport or decoded moving picture stops. GitHub Actions runs
separate SRT and HLS jobs every night on `main`
(`.github/workflows/soak.yml`); run it by hand from the Actions tab
(**Soak test → Run workflow**, any branch and length). A pull request that
changes the soak test runs a 15-minute one.

## What it does

By default everything runs in disposable containers on the loopback interface,
with an empty `/data`. An optional native adapter uses the same workload and
deadlines inside an existing development container, without creating more
Docker writable layers (see below):

- MediaMTX (`mediamtx.yml`) receives NoAr's SRT output and relays the
  synthetic live. In the HLS job NoAr writes its own HLS output; MediaMTX
  still relays the scheduled live sources.
- NoAr is the image under test, set up through its API with a random
  password.
- GStreamer publishers, started from the same image, play the live encoder.
- An independent FFmpeg receiver on the test host decodes the actual SRT
  or HLS output and compares its pictures; a second one measures A/V sync.

The playlist repeats three clips of different sizes and frame rates (one
without sound), made with FFmpeg on the test host. Clips and lives have moving
pictures so movement is expected throughout normal playback. Each clip also
carries an A/V sync marker once a second: a box in the top-left corner, black
except for one white frame, and a 1 kHz beep that starts on that frame, 0.4 s
into each second, a time every usual frame rate has a frame at, so NoAr's
frame-rate conversion cannot move the flash (`sync.py`). Every 12 minutes a cycle schedules three lives and acts on
them:

| Time | What happens |
| --- | --- |
| 0:30–3:30 | SRT live; its encoder drops out at 1:30 for 20 s and comes back |
| 4:30–6:30 | The same live read over RTMP |
| 7:00–9:30 | UDP live; its encoder drops its video at 8:00 for 30 s and takes it back |
| 10:30 | Playout restart in the middle of a clip |

Every 2 s it reads `/health`, the bytes MediaMTX receives from NoAr (SRT) or
the advancing media playlist (HLS), and the clip on air. The HLS check observes
playlist contents rather than directory size, which may stay constant as old
segments rotate out. The independent decoder samples two 160×90 grayscale
frames per second. It fails when:

- NoAr exits;
- the output stops growing for more than 12 s;
- no decoded video frames arrive for more than 12 s;
- the decoded picture stays unchanged for more than 12 s, even while new
  frames, network bytes and a healthy `/health` response keep arriving;
- the channel log has an error (a live that did not air, for example);
- `/health` is not ok for more than 20 s;
- NoAr's output has `panicked`, `CRITICAL`, `assertion` or a decoder stream
  switch error;
- the sound of three markers in a row airs outside EBU R37 from its picture
  (more than 40 ms ahead or 60 ms behind), or fewer than 20 markers were
  measured.

### A/V sync

The sync receiver runs `blackdetect` on the top-left corner of the output
(the flash ends a black interval) and `silencedetect` on its sound (the beep
ends a silence), both on the stream's own timestamps. A flash must last one
frame and a beep be short, so the start of a live or the end of the waiting
card is not taken for a marker; lives carry none. Each beep is paired with
the nearest flash, and the offset (sound minus picture) of every pair is
written to `av-sync.jsonl` with the time of the run. FFmpeg prints those
times with six significant digits, so from 1000 s on only to 10 ms: the
receiver starts over every 10 minutes to keep its clock below that and the
offsets to the millisecond. The lines the offsets come from are kept in
`av-sync.receiver.log`. The report's `av_sync`
gives the pairs, the lowest, highest, mean and last offset, how many fell
outside EBU R37, and the drift: the mean of the last 20 offsets minus that of
the first 20. The box is 1/8 of the clip and the receiver reads 1/16 of the
output, so the L-shape template, which shrinks the clip to 75 %, keeps it in
view; no soak graphic covers that corner.

Missing output and an unhealthy state are allowed for 45 s after the
deliberate restart.

Picture checks have a 60 s startup grace and the same 45 s restart grace.
Only deliberate encoder outages additionally allow static waiting cards:
40 s from the SRT dropout and 50 s from the UDP audio-only fault, including
their 20/30 s fault durations and a bounded recovery allowance. A repeated
decoder reconnect never resets the missing-frame or frozen-picture deadline.
A run without enough decoded movement or any checks outside grace fails.

An image changes when its mean absolute grayscale difference from the last
changed reference reaches 0.5 on the 0–255 scale. Comparing against that
reference lets small movements accumulate. This is a check of known-moving
synthetic media, not an alarm suitable for arbitrary programmes with legitimate
still pictures. It does not establish A/V synchronization or detect every
partially frozen region.

The run's `summary.md` (also on the workflow page), `report.json`,
`soak.log` (the test's own steps), `noar.log` (NoAr's output) and
`channel.log` (the channel log), `picture-output.decoder.log` and
`picture-output.jsonl` (decoded frame counts, ages and pixel differences) are
kept as separate SRT/HLS workflow artifacts for 14 days. `report.json` includes
the selected output, picture evidence and explicit output coverage statuses.

## Run it locally

Use a disposable machine with Docker and host `ffmpeg` supporting SRT and HTTP,
and free loopback ports 18787, 1935,
8890 and 9997:

```sh
docker build -t noar-soak:ci .
NOAR_SOAK=1 NOAR_SOAK_MINUTES=30 python3 qa/soak/soak.py
NOAR_SOAK=1 NOAR_SOAK_OUTPUT=hls NOAR_SOAK_MINUTES=30 python3 qa/soak/soak.py
```

`NOAR_SOAK_BINARY=/path/to/noar` mounts another NoAr binary over the
image's, with an image that has GStreamer (a development build).
`NOAR_SOAK_DIR` sets where the data and logs go (default
`qa/soak/artifacts/run`, ignored by git).

### Native execution inside an existing development container

The native adapter requires explicit NoAr and MediaMTX executables, the matching
GStreamer libraries/plugins, `gst-launch-1.0`, FFmpeg, and the same free loopback
ports as Docker mode, plus UDP 5004. Run the following **inside** the already
prepared container (for example `noar-dev`), using paths visible there:

```sh
NOAR_SOAK=1 NOAR_SOAK_MODE=native \
NOAR_SOAK_BINARY=/path/to/current/noar \
NOAR_SOAK_MEDIAMTX_BINARY=/path/to/mediamtx \
NOAR_SOAK_DIR=/tmp/noar-soak-hls-unique-run \
NOAR_SOAK_OUTPUT=hls NOAR_SOAK_GRAPHICS=1 NOAR_SOAK_MINUTES=90 \
python3 qa/soak/soak.py
```

`NOAR_SOAK_DIR` must name a **nonexistent** directory outside the repository.
The harness refuses to reuse a run or configure an initialized installation.
It overrides inherited `DB` and setup paths with its own `data/` subdirectories,
checks loopback ports before starting, and never targets an existing server.
The child processes inherit the container's library/plugin environment; this
does not install or upgrade GStreamer. `NOAR_SOAK_GST=/path/to/gst-launch-1.0`
can select an explicit publisher/fixture executable. MediaMTX uses the checked-in
loopback-only configuration and should match the Docker fixture version 1.15.3.

GStreamer creates clips and publishes directly; NoAr and MediaMTX are direct
children with their own process groups. Cleanup on success, errors, Ctrl-C or
SIGTERM stops only groups created by this run. `processes.jsonl` records owned
PIDs/commands, and `owned-run.json` identifies the run. Data and evidence remain
in the owned directory for review; the harness does not remove them. Native mode
makes no Docker calls. Docker remains the default, including the existing meaning
of `NOAR_SOAK_BINARY` as a bind-mounted replacement binary.

The observation deadline starts after setup and uses monotonic time. A 90-minute
request observes at least 5,400 real seconds; setup is additional time. Interrupting
or exiting early fails duration coverage. `report.json` records the UTC start/end
and actual `measurement_seconds`; changing the requested minutes creates a shorter
test and cannot establish a 90-minute result.

### Optional graphics workload and video-only motion checks

Set `NOAR_SOAK_GRAPHICS=1` in either mode to create synthetic logo artwork and four
new Composer models through the real API: assigned logo, assigned L-shape,
assigned scrolling ticker, and a temporary lower third (GC). Every 30 seconds
the harness advances this eight-step sequence throughout the observation:

1. Show logo and ticker.
2. Take the GC with a changing field value.
3. Enter the L-shape.
4. Take the GC out (a deliberate restart may already have cleared it).
5. Save alternate logo artwork through the model API.
6. Exit the L-shape.
7. Hide ticker and logo.
8. Save updated scrolling ticker text and show ticker and logo.

Each API request and response, including creation, assignments, controls and
on-air/channel snapshots, is appended to `graphics-receipts.jsonl` with elapsed
and UTC timestamps. Failed HTTP operations, mismatched acknowledged visibility,
missing GC takes, or edits that do not persist fail the run. The report includes
model IDs, completed steps and receipt count. Receipts contain only synthetic
model data, never the login token/password.

Graphics mode also starts a second independent decoder for a video-only ROI:
output x=10–45%, y=15–50%. The full-frame video and the authored top-left 75%
L-shape video both occupy that region. Logo, ticker and GC layers (including the
GC's follow-picture transform) stay outside it. The same missing/frozen-frame
deadlines and bounded fault/restart grace apply to this observer. Animated or
scrolling overlays therefore cannot supply the movement needed to pass a frozen
underlying picture. Its decoder log and samples are `picture-video-roi.decoder.log`
and `picture-video-roi.jsonl`; `report.json` includes `video_roi` and
`video_roi_checks`. Insufficient ROI frames, motion, or checks outside grace fail.

This workload checks API state, sustained transport and independent moving video.
It does not assert artwork pixels, text legibility, scroll speed, A/V sync,
frame-accurate transitions, physical outputs, or long-term resource bounds.
HLS output retains its normal rotating window; it is not a full segment archive.

## Verify the observer itself

```sh
python3 -m unittest discover -s qa/soak -p 'test_*.py'
NOAR_SOAK=1 python3 qa/soak/verify_picture.py --binary target/debug/noar
```

The second command requires a Linux NoAr binary, matching GStreamer plugins
and host FFmpeg. It starts fresh native instances on loopback ports 18787
(HTTP) and 18990 (SRT output listener), using random accounts, empty databases
and synthetic files. For SRT and HLS it verifies both moving video (must pass)
and intentionally static video (must be detected while decoded frames and
`/health` keep arriving). Files and databases are removed; logs and JSON evidence
remain under `work/picture-*`. This short test validates the observer and real
output paths; it is not a substitute for the full 90-minute fault cycle.

NDI and SDI remain **NOT_AVAILABLE** in this harness: no NDI runtime/receiver
or DeckLink output/capture fixture is configured. They need separate tests on
appropriate equipment. Adaptive HLS variants, audio continuity and picture
correctness beyond motion are also outside this check. A/V sync is measured
on the clips by `sync.py` (above) and, on a short run through the encoder,
by the engine test `av_sync_tests`.

References: [GStreamer videotestsrc](https://gstreamer.freedesktop.org/documentation/videotestsrc/)
(scrolling pattern), [FFmpeg rawvideo](https://ffmpeg.org/ffmpeg-formats.html#rawvideo-1)
and [fps filter](https://ffmpeg.org/ffmpeg-filters.html#fps-1). Installed tool
properties and actual output evidence take precedence over newer online examples.
