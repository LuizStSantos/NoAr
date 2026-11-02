---
name: noar-video-processing
description: GStreamer-first media processing guidance for NoAr decoding, encoding, A/V timing, SCTE-35 and output continuity.
---

# NoAr Video Processing Reference

User request: $ARGUMENTS

This skill traces the whole playout pipeline. GStreamer is the sole media engine on this integration branch. Use `/noar-gstreamer` for native APIs, plugin properties, states, seeking, clocks and caps. `/noar-ffmpeg` remains scoped to memory-frame compatibility and test/codec tooling. These references add no agents or Backend Council voters.

## Official sources

- GStreamer documentation https://gstreamer.freedesktop.org/documentation/
- GStreamer application development https://gstreamer.freedesktop.org/documentation/application-development/
- GStreamer Rust bindings https://gstreamer.freedesktop.org/bindings/rust.html
- Versioned Rust API https://docs.rs/gstreamer/0.24.5/gstreamer/
- Project references `docs/ai/tech-references/gstreamer.md`
- HLS RFC https://www.rfc-editor.org/rfc/rfc8216

### Remaining frame-memory and fixture interfaces

- FFmpeg documentation https://www.ffmpeg.org/documentation.html
- FFmpeg Doxygen https://www.ffmpeg.org/doxygen/
- Rust bindings https://docs.rs/ffmpeg-next/

## Research procedure

Inspect only affected `backend/engine/`, `backend/app/src/player/` and output code. Identify native input → demux → decode → Rust processing → encode → per-destination parse/mux → transport. Review segments/running time, PTS/DTS, frame rates, GOP/keyframes, A/V sync, loudness, frame layouts, queue bounds, seek/flush, EOS, ownership, cancellation and reconnects. Keep output pipelines persistent across source changes. NoAr uses external live clients, not input listeners. Confirm Cargo.lock, GStreamer versions and installed factories before proposing properties; preserve legacy configuration units where adapters translate them. Use FFmpeg research only when its remaining memory or test interfaces matter. Verify SCTE-35 packets and decoded transitions in isolated synthetic-media staging, including wall-clock receiver progress and MediaMTX session counts.

## Research and safety rules

1. Search the persistent NoAr repository map and inspect only relevant current code.
2. Read exact lockfile/resolved dependency versions, native GStreamer/plugin metadata and active features before applying advice. Online "latest" documentation may not match NoAr.
3. Consult 1–3 relevant official reference sections, cite exact URLs and NoAr file paths, and keep the reply concise.
4. Distinguish DOCUMENTED, SOURCE_VERIFIED, RUNTIME_VERIFIED and NOT_RUN. Documentation and code review do not prove on-air playback or clickable UI.
5. Read-only research never changes application code, production configuration, live services or data.
6. This skill is a reference playbook, not a new agent or auditor. Implementation belongs to the main workflow, and runtime validation belongs to authorized isolated QA.
