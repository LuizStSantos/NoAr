---
name: noar-video-processing
description: Official-reference media processing guidance for NoAr decode, filtering, encoding, A/V timing, SCTE-35 and output continuity.
---

# NoAr Video Processing Reference

User request: $ARGUMENTS

This skill traces the whole playout pipeline. It complements `/noar-ffmpeg`, which focuses on specific FFmpeg CLI or C API questions. Neither adds an agent or a Backend Council voter.

## Official sources

- FFmpeg documentation https://www.ffmpeg.org/documentation.html
- FFmpeg codecs https://www.ffmpeg.org/ffmpeg-codecs.html
- FFmpeg filters https://www.ffmpeg.org/ffmpeg-filters.html
- FFmpeg formats https://www.ffmpeg.org/ffmpeg-formats.html
- FFmpeg protocols https://www.ffmpeg.org/ffmpeg-protocols.html
- FFmpeg Doxygen https://www.ffmpeg.org/doxygen/
- Rust bindings https://docs.rs/ffmpeg-next/
- HLS RFC https://www.rfc-editor.org/rfc/rfc8216

## Research procedure

Inspect only affected `backend/engine/`, `backend/app/src/player/` and output code. Identify input → demux → decode → process/filter → encode → mux → output. Review time bases and PTS/DTS, frame-rate conversion, GOP/keyframes, A/V sync, loudness, pixel formats, buffering, filter ownership, shutdown and reconnect behavior. Distinguish SCTE-35 cue generation, transport signaling and observed downstream behavior. Confirm `ffmpeg-next` binding and native FFmpeg ABI/build before suggesting APIs. Use the existing FFmpeg Specialist for focused documented C API questions without duplicating research. Proposed validation must be synthetic-media staging with FFprobe/packet evidence, not live broadcast.

## Research and safety rules

1. Search the persistent NoAr repository map and inspect only relevant current code.
2. Read exact lockfile/resolved dependency versions and active features before applying advice. Online "latest" documentation may not match NoAr.
3. Consult 1–3 relevant official reference sections, cite exact URLs and NoAr file paths, and keep the reply concise.
4. Distinguish DOCUMENTED, SOURCE_VERIFIED, RUNTIME_VERIFIED and NOT_RUN. Documentation and code review do not prove on-air playback or clickable UI.
5. Read-only research never changes application code, production configuration, live services or data.
6. This skill is a reference playbook, not a new agent or auditor. Implementation belongs to the main workflow, and runtime validation belongs to authorized isolated QA.
