# Integrated fire-test evidence

Result: **`PASS_WITH_CAPTURE_RECHECK`**. Read [summary.json](summary.json),
[analysis.json](analysis.json) and the unchanged [raw-report.json](raw-report.json).
This is one actual 900-second local channel test, not a 24-hour qualification.

The raw runner reports FAIL for one wall-clock clip-mark snapshot taken during
the exit animation. The final analyser verifies 43 consecutive decoded frames
with the expected artwork, video and border; both captures are retained.
The snapshot assertion was subsequently deferred to decoded-interval checking.
The exact runner source loaded at the beginning was not hashed. Final helper
hashes identify the later tools, not a clean online rerun of that edit.

Observed limits matter: the audio-only UDP phase held a video picture for up to
21.513 seconds inside its declared grace window. Injected SRT loss produced a
1.0-second pacing reset and two signal warnings. There were seven backend
warnings in total, zero ERROR/fatal lines. No seamless recovery or transition
parity claim is made. Earlier waiting/filler handling for video loss needs
separate investigation.

The initial 1.26.2 development runtime aborted in decodebin3. The Docker image
already contains the correction in core/base/libav 1.26.11. The final test used
those libraries and the same original NoAr binary; no backend fix was committed.
See [native provenance](native-runtime-provenance.txt),
[native track-switch probe](native-udp-track-probe.json), and the original
[environment failure](before-runtime-alignment-report.json) /
[log](before-runtime-alignment-backend.log.gz).

## Browser screenshots

The additional [monitor proof](browser-monitor-proof.json) requires two presented
video frames, live UDP and a uniquely labelled GC. Desktop images are 1440×1000;
the phone screenshot is 390 px wide/full-page. All use dark mode.

| Screen | Screenshot |
| --- | --- |
| On air, pt-BR, advancing real video | [Desktop](fire-onair-verified-pt-BR.jpg) |
| On air, pt-BR, advancing real video | [Phone](fire-onair-verified-mobile-pt-BR.jpg) |
| Scheduler, pt-BR | [Screenshot](fire-scheduler-pt-BR.jpg) |
| Models, pt-BR (8 verified via API; scene continues below viewport) | [Screenshot](fire-models-pt-BR.jpg) |
| Live created in the original test | [Form](fire-live-form.jpg) |
| Original Scheduler capture | [Screenshot](fire-scheduler.jpg) |
| Original capture before video became ready | [Desktop](fire-onair-pt-BR.jpg) |

## Frames from original HLS segments

Each output image is 640×360. Pixel checks use the original decoded RGB;
the JPEG is the review copy. Colour tolerance is 25/255 per component.

| Phase | Frame |
| --- | --- |
| SRT and channel logo | [Frame](output-fire-srt-live.jpg) |
| Logo replaced | [Frame](output-fire-logo-swapped.jpg) |
| Logo moved | [Frame](output-fire-logo-moved.jpg) |
| SRT signal removed | [Frame](output-fire-srt-loss.jpg) |
| SRT recovered | [Frame](output-fire-srt-recovered.jpg) |
| L-shape and following lower third | [Frame](output-fire-lshape-and-gc.jpg) |
| Scheduled notice and CSV crawl | [Frame](output-fire-scheduled-notice-data.jpg) |
| Full screen with scheduled notice | [Frame](output-fire-full-screen-notice.jpg) |
| Notice ended | [Frame](output-fire-notice-ended.jpg) |
| Clip-specific logo | [Frame](output-fire-clip-logo.jpg) |
| Clip L-shape: aligned stable frame | [Frame](output-fire-clip-lshape-aligned.jpg) |
| Clip L-shape: original exit-animation capture | [Frame](output-fire-clip-lshape.jpg) |
| RTMP and scene | [Frame](output-fire-rtmp-scene.jpg) |
| RTMP full screen | [Frame](output-fire-rtmp-fullscreen.jpg) |
| Scheduled commercial, logo suppressed | [Frame](output-fire-commercial.jpg) |
| UDP live | [Frame](output-fire-udp-live.jpg) |
| UDP audio-only fault | [Frame](output-fire-udp-audio-only.jpg) |
| UDP recovered | [Frame](output-fire-udp-recovered.jpg) |
| Playout restarted | [Frame](output-fire-after-restart.jpg) |
| HLS input | [Frame](output-fire-hls-live.jpg) |
| Late temporary graphics | [Frame](output-fire-late-graphics.jpg) |

## Logs and measurements

- [Health/program samples](health-and-program.jsonl.gz),
  [graphics/resources](graphics-and-resources.jsonl.gz),
  [picture samples](decoded-picture.jsonl.gz),
  [segment hashes](hls-segments.json.gz).
- [Backend](backend.log.gz), [MediaMTX](mediamtx.log.gz),
  [independent decoder](picture-decoder.log.gz), [actions](run.log).
- [Browser actions](browser-actions.json),
  [marked-frame sequence](clip-mark-frames.json),
  [GIF/clock changes](permanent-animation-check.json).
- [Rust checks](rust-checks.log.gz), [observer tests](observer-tests.log.gz),
  [fixture/preflight tests](harness-tests.log.gz).

Both output generations fully decode. Video DTS steps are 40 ms; maximum audio
step is 21.344 ms; A/V tail differences are under 59 ms. TS continuity is checked
inside each segment: splitmuxsink counter restarts between files are separately
recorded, not represented as packet loss. All 451 original segment hashes were
rechecked. The full TS archive stays in the owned workspace path recorded in
the summary; this repository stores hashes, measurements, logs and review frames.

No credentials, account database or operator configuration is included.
NOT_RUN: physical devices, desktop audio, WAN loss, multiple channels, output
RTMP/SRT/UDP, a 24-hour soak and frame-accurate transition parity.
