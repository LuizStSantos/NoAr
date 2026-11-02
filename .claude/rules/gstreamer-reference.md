---
paths:
  - "backend/engine/**"
  - "backend/app/src/player/**"
  - "Dockerfile"
  - ".github/workflows/validate.yml"
  - "qa/gstreamer/**"
---

# GStreamer source-reference rule (NoAr)

GStreamer is the sole playout engine on this branch. For version-sensitive
media work, read `/noar-gstreamer` and `docs/ai/tech-references/gstreamer.md`.
Use the resolved gstreamer-rs version in Cargo.lock and installed native plugin
properties/caps/units; do not apply FFmpeg options directly to GStreamer.

Live URLs are clients of external services such as MediaMTX. Do not restore
embedded RTMP/SRT input listeners or add an optional FFmpeg playback backend.
FFmpeg research is limited to remaining frame-memory and fixture interfaces.

Preserve output sessions and timelines across source changes. Verify actual
NoAr output with synthetic media, decoded transitions and wall-clock receiver
progress; prototype success or source review alone is not runtime evidence.
