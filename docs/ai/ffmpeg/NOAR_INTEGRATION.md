# FFmpeg ↔ NoAr integration notes

GStreamer is the sole playout engine on the integration branch. Start media
research with `docs/ai/tech-references/gstreamer.md` and `/noar-gstreamer`.
NoAr does not link the FFmpeg libraries. FFmpeg remains only as GStreamer codecs (`gstreamer1.0-libav`) and as `ffmpeg`/`ffprobe` test tools.

## Where the FFmpeg code lives

- Root Cargo workspace has `backend/app` (`noar`) and `backend/engine` (`noar-engine`).
- No crate binds FFmpeg (`ffmpeg-next` and the `ffmpeg-filter`/`ffmpeg-device` features were removed).
- Native playback/timestamps: `backend/engine/src/playout.rs` and `input/gstreamer.rs` use GStreamer.
- Native encoding/muxing: `backend/engine/src/output/gstreamer_output.rs`, `output/pipeline.rs` and `output/link.rs`.
- Frames: `backend/engine/src/frame.rs` (`VideoFrame` in YUV420P, YUVA420P and BGRx; `AudioFrame` in planar f32), copy-on-write planes. Time bases: `backend/engine/src/time_base.rs` (`Rational`, `Rescale` with FFmpeg's nearest, halves-away-from-zero rounding).
- The move off FFmpeg frames was checked against the FFmpeg path with lossless x264 on the same clips: identical decoded video and audio frames.
- Broadcast cue insertion: `backend/engine/src/output/scte35.rs`.
- Control/scheduling: `backend/app/src/player/`.

## Crucial distinction

The official *ffmpeg CLI* docs cover the `ffmpeg`/`ffprobe` test tools. NoAr has no FFmpeg C-library binding any more; codec behaviour inside GStreamer's libav plugin is described by `gst-inspect-1.0 avdec_*`/`avenc_*` and the GStreamer docs.

## Request-specific handoff

The Orchestrator should supply a narrowly-scoped question, affected file paths, build/target details, whether a runtime test is available, and an output budget. The specialist should reply with a minimal evidence record:

```text
Question: <one sentence>
FFmpeg build: <verified version / unknown>
Official manuals: <2-4 URLs and relevant options/APIs>
NoAr location: <file:line>
Finding: <source-verified/documented/inferred>
Suggested change: <bounded proposal, no automatic edit>
Staging verification: <specific, safe, synthetic-media test>
```

## Common pitfalls

- The online manuals are generated from current FFmpeg, possibly ahead of the installed library.
- Available encoders, protocols and hardware accelerators depend on the build.
- Packet/frame lifetime and ownership differ across Rust bindings and C functions.
- Staging must verify HLS playlist/segment health, A/V clock continuity, reconnect and SCTE-35 with reproducible synthetic fixtures, not real broadcasts.
