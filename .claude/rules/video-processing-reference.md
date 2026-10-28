---
paths:
  - "backend/engine/src/**/*.rs"
  - "backend/app/src/player/**/*.rs"
---

# Media processing reference routing

For timing, A/V sync, encoding, SCTE-35, filters and output resilience, use `/noar-video-processing` for pipeline reasoning; use `/noar-ffmpeg` for specific FFmpeg API references. Do not claim runtime correctness without synthetic-media verification.
