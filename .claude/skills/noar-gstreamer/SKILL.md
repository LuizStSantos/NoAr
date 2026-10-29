---
name: noar-gstreamer
description: Official GStreamer and gstreamer-rs references for NoAr's single native media engine, source switching, clocks, buffers, encoding, muxing and transports.
---

# NoAr GStreamer Reference

Use this skill for NoAr media-runtime questions. GStreamer is the mandatory
playout engine on the integration branch. Do not suggest a backend selector,
FFmpeg playback fallback, or built-in RTMP/SRT input servers. External client
sources, including MediaMTX URLs, remain part of scheduled live playback.

## Check the implementation first

1. Locate affected symbols with `docs/ai/repository-map.md` and
   `python3 .claude/scripts/update_repository_map.py --find '<topic>'`.
2. Confirm versions in `backend/engine/Cargo.toml`, `Cargo.lock`, `Dockerfile`
   and `.github/workflows/validate.yml`. The initial integration uses native
   GStreamer 1.26 and gstreamer-rs 0.24; use the resolved versions as evidence.
3. Inspect only the relevant source: `gstreamer.rs`, `input/gstreamer.rs`,
   `input/preload.rs`, `playout.rs`, `output/gstreamer_output.rs`,
   `output/pipeline.rs`, `output/link.rs`, and the native HLS/recording helpers.
4. Read 1–3 relevant official documentation sections. Check installed native
   property/caps metadata and exact Rust bindings before applying examples.

## Official documentation

- Documentation index: https://gstreamer.freedesktop.org/documentation/
- Application development: https://gstreamer.freedesktop.org/documentation/application-development/
- Clock and synchronization design: https://gstreamer.freedesktop.org/documentation/additional/design/synchronisation.html
- AppSrc: https://gstreamer.freedesktop.org/documentation/app/appsrc.html
- AppSink: https://gstreamer.freedesktop.org/documentation/app/appsink.html
- SRT input/output: https://gstreamer.freedesktop.org/documentation/srt/srtsrc.html and https://gstreamer.freedesktop.org/documentation/srt/srtsink.html
- MPEG-TS muxer: https://gstreamer.freedesktop.org/documentation/mpegtsmux/mpegtsmux.html
- Segmented recording: https://gstreamer.freedesktop.org/documentation/multifile/splitmuxsink.html
- Rust bindings: https://gstreamer.freedesktop.org/bindings/rust.html
- Versioned Rust API: https://docs.rs/gstreamer/0.24.5/gstreamer/
- Project navigation: `docs/ai/tech-references/gstreamer.md`.

## Local evidence

Use `pkg-config --modversion` for `gstreamer-1.0`, `gstreamer-app-1.0`,
`gstreamer-audio-1.0`, `gstreamer-video-1.0`, `gstreamer-pbutils-1.0`, and
`gstreamer-mpegts-1.0`. Use `gst-inspect-1.0 <factory>` for installed pad caps,
properties, enum values and units. Latest web documentation is not proof that
a plugin, property, codec, GPU or transport is available locally.

Review states, dynamic pads, queues/backpressure, flush/seek, EOS, cancellation,
PTS/DTS and segment-to-running-time conversion. Keep the output encoder, muxer
and connection persistent across source changes. Check byte-stream versus AVC
framing independently for each destination, and preserve both A/V timelines.
Never equate a network connection count or valid timestamps with uninterrupted
wall-clock delivery: inspect receiver progress and decoded source transitions.

FFmpeg frame structures remain a memory adapter for Rust graphics/audio. The
FFmpeg CLI is also used for synthetic fixtures and independent inspection.
Use `/noar-ffmpeg` only for those scoped compatibility or fixture questions;
it is not the primary reference for playout control or GStreamer properties.

## Verification and boundaries

Run focused tests, formatting and applicable Clippy checks. Media changes need
actual isolated NoAr/MediaMTX/capture evidence; the Python prototype alone does
not validate the integrated scheduler. Distinguish `PASS`, `FAIL`, `NOT_RUN`
and `NOT_AVAILABLE`, including unavailable GPU/browser/native dependencies.
Documented behavior, source review and successful compilation do not establish
broadcast readiness. Do not alter production, publish releases or bypass a
network policy while researching documentation.

This is a scoped reference skill, not a new agent or audit council member.
