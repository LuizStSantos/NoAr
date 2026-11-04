# Soak test

`soak.py` keeps NoAr on air for a long time (90 minutes by default) with
synthetic media and the faults that have taken channels off air before, and
fails when the output stops. GitHub Actions runs it every night on `main`
(`.github/workflows/soak.yml`); run it by hand from the Actions tab
(**Soak test → Run workflow**, any branch and length).

## What it does

Everything runs in disposable containers on the loopback interface, with an
empty `/data`:

- MediaMTX (`mediamtx.yml`) receives NoAr's SRT output and relays the
  synthetic live.
- NoAr is the image under test, set up through its API with a random
  password.
- GStreamer publishers, started from the same image, play the live encoder.

The playlist repeats three clips of different sizes and frame rates (one
without sound). Every 12 minutes a cycle schedules three lives and acts on
them:

| Time | What happens |
| --- | --- |
| 0:30–3:30 | SRT live; its encoder drops out at 1:30 for 20 s and comes back |
| 4:30–6:30 | The same live read over RTMP |
| 7:00–9:30 | UDP live; its encoder drops its video at 8:00 for 30 s and takes it back |
| 10:30 | Playout restart in the middle of a clip |

Every 2 s it reads `/health`, the bytes MediaMTX receives from NoAr and the
clip on air. It fails when:

- NoAr exits;
- the output stops growing for more than 12 s;
- `/health` is not ok for more than 20 s;
- NoAr's output has `panicked`, `CRITICAL`, `assertion` or a decoder stream
  switch error.

Missing output and an unhealthy state are allowed for 45 s after the
deliberate restart.

The run's `summary.md` (also on the workflow page), `report.json`,
`soak.log` (the test's own steps), `noar.log` (NoAr's output) and
`channel.log` (the channel log) are kept as the workflow's artifact for 14
days.

## Run it locally

Use a disposable machine with Docker, and free loopback ports 18787, 1935,
8890 and 9997:

```sh
docker build -t noar-soak:ci .
NOAR_SOAK=1 NOAR_SOAK_MINUTES=30 python3 qa/soak/soak.py
```

`NOAR_SOAK_BINARY=/path/to/noar` mounts another NoAr binary over the
image's, with an image that has GStreamer (a development build).
`NOAR_SOAK_DIR` sets where the data and logs go (default
`qa/soak/artifacts/run`, ignored by git).

## What it does not check

It measures that the output keeps flowing and that NoAr reports itself on
air, not what the picture looks like: a frozen picture that still encodes
passes. It does not use NDI, SDI or HLS outputs.
