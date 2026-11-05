# Soak test

`soak.py` keeps NoAr on air for a long time (90 minutes by default) with
synthetic media and the faults that have taken channels off air before, and
fails when the transport or decoded moving picture stops. GitHub Actions runs
separate SRT and HLS jobs every night on `main`
(`.github/workflows/soak.yml`); run it by hand from the Actions tab
(**Soak test → Run workflow**, any branch and length). A pull request that
changes the soak test runs a 15-minute one.

## What it does

Everything runs in disposable containers on the loopback interface, with an
empty `/data`:

- MediaMTX (`mediamtx.yml`) receives NoAr's SRT output and relays the
  synthetic live. In the HLS job NoAr writes its own HLS output; MediaMTX
  still relays the scheduled live sources.
- NoAr is the image under test, set up through its API with a random
  password.
- GStreamer publishers, started from the same image, play the live encoder.
- An independent FFmpeg receiver on the test host decodes the actual SRT
  or HLS output and compares its pictures.

The playlist repeats three clips of different sizes and frame rates (one
without sound). Clips and lives use scrolling SMPTE patterns so movement is
expected throughout normal playback. Every 12 minutes a cycle schedules three lives and acts on
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
  switch error.

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
correctness beyond motion are also outside this check.

References: [GStreamer videotestsrc](https://gstreamer.freedesktop.org/documentation/videotestsrc/)
(scrolling pattern), [FFmpeg rawvideo](https://ffmpeg.org/ffmpeg-formats.html#rawvideo-1)
and [fps filter](https://ffmpeg.org/ffmpeg-filters.html#fps-1). Installed tool
properties and actual output evidence take precedence over newer online examples.
