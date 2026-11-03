---
paths:
  - "backend/engine/src/**/*.rs"
  - "backend/app/src/player/**/*.rs"
---

# Media processing reference routing

For timing, A/V sync, encoding, SCTE-35, processing and output resilience, use `/noar-video-processing` for pipeline reasoning and `/noar-gstreamer` for native APIs and plugin properties. GStreamer is the only playout engine on this branch; live input URLs connect to external ingest servers. Use `/noar-ffmpeg` only for the codecs GStreamer takes from `gstreamer1.0-libav` and `ffmpeg`/`ffprobe` test tooling (NoAr does not link FFmpeg). Do not claim runtime correctness without integrated synthetic-media verification, wall-clock receiver progress and decoded transitions.
