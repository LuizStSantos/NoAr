---
paths:
  - "backend/engine/**/*.rs"
  - "backend/app/src/player/**/*.rs"
  - "qa/**/*.py"
---

# FFmpeg source-reference rule (NoAr)

GStreamer is NoAr's only media runtime and NoAr does not link FFmpeg. Start
engine research with `/noar-gstreamer`; this rule applies only when a codec
GStreamer takes from `gstreamer1.0-libav`, or an `ffmpeg`/`ffprobe` test
fixture, specifically needs FFmpeg documentation.

When a task involving these files depends on FFmpeg details, consult `/noar-ffmpeg` or request the `noar-ffmpeg-docs-specialist` to examine the **relevant** official documentation. Do not spawn the specialist for trivial Rust edits or rename-only tasks. This is a documentation adviser, not a new audit council member.

- Official index: `https://www.ffmpeg.org/documentation.html`
- Scoped lookup: `python3 .claude/scripts/ffmpeg_docs.py topics '<topic>'`
- Excerpt: `python3 .claude/scripts/ffmpeg_docs.py search '<term>' --manual <manual-id>`
- Local compatibility: `python3 .claude/scripts/ffmpeg_docs.py capabilities`
- Never treat FFmpeg trunk documentation as proof of local build support.
- Treat on-air continuity as safety-critical for production; use staging and synthetic media for tests.
