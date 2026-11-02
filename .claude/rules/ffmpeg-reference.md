---
paths:
  - "backend/engine/**/*.rs"
  - "backend/app/src/player/**/*.rs"
  - "backend/app/src/utils/ffmpeg*.rs"
---

# FFmpeg source-reference rule (NoAr)

GStreamer is the sole playout runtime on the integration branch. Start engine
research with `/noar-gstreamer`; this rule applies only when a remaining FFmpeg
frame-memory interface, codec dependency or test fixture specifically needs it.

When a task involving these files depends on FFmpeg details, consult `/noar-ffmpeg` or request the `noar-ffmpeg-docs-specialist` to examine the **relevant** official documentation. Do not spawn the specialist for trivial Rust edits or rename-only tasks. This is a documentation adviser, not a new audit council member.

- Official index: `https://www.ffmpeg.org/documentation.html`
- Scoped lookup: `python3 .claude/scripts/ffmpeg_docs.py topics '<topic>'`
- Excerpt: `python3 .claude/scripts/ffmpeg_docs.py search '<term>' --manual <manual-id>`
- Local compatibility: `python3 .claude/scripts/ffmpeg_docs.py capabilities`
- Never treat FFmpeg trunk documentation as proof of local build support.
- Treat on-air continuity as safety-critical for production; use staging and synthetic media for tests.
