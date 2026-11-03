---
paths:
  - "backend/app/**/*.rs"
  - "backend/engine/**/*.rs"
  - "migrations/**/*.sql"
  - "Cargo.toml"
  - "backend/*/Cargo.toml"
---

# Rust reference routing

For nontrivial Rust/async/SQLite documentation questions, use the relevant parts of `/noar-rust`, checking `Cargo.lock` and `backend/AGENTS.md`. For the media engine use `/noar-gstreamer` and the exact gstreamer-rs version; NoAr does not link FFmpeg, so `/noar-ffmpeg` only covers libav codecs inside GStreamer and test fixtures. Do not consult language docs on trivial formatting work or add new council votes.
