# FFmpeg ↔ NoAr integration notes

GStreamer is the sole playout engine on the integration branch. Start media
research with `docs/ai/tech-references/gstreamer.md` and `/noar-gstreamer`.
FFmpeg remains a memory-frame and codec/test dependency.

## Where the FFmpeg code lives

- Root Cargo workspace has `backend/app` (`noar`) and `backend/engine` (`noar-engine`).
- `backend/engine/Cargo.toml` declares `ffmpeg-next` with selected native features and optional `ffmpeg-filter` / `ffmpeg-device` feature flags.
- Native playback/timestamps: `backend/engine/src/playout.rs` and `input/gstreamer.rs` use GStreamer.
- Native encoding/muxing: `backend/engine/src/output/gstreamer_output.rs`, `output/pipeline.rs` and `output/link.rs`.
- Remaining FFmpeg memory adapter: `backend/engine/src/utils/ffmpeg.rs`; frame containers are also used by the Rust compositor/audio code. Test modules may use FFmpeg to generate independent fixtures.
- Broadcast cue insertion: `backend/engine/src/output/scte35.rs`.
- Control/scheduling: `backend/app/src/player/`.

## Crucial distinction

The official *ffmpeg CLI* docs cover executable flags. NoAr also uses the FFmpeg *C libraries* through `ffmpeg-next`, so look at the official FFmpeg Doxygen interface and the actual binding, including library versions, ownership, error paths and time-base conversions. Never transcribe a CLI flag into a Rust API call without checking the binding.

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

- `ffmpeg-next` crate version is not proof of native runtime library versions.
- The online manuals are generated from current FFmpeg, possibly ahead of the installed library.
- Available encoders, protocols and hardware accelerators depend on the build.
- Packet/frame lifetime and ownership differ across Rust bindings and C functions.
- Staging must verify HLS playlist/segment health, A/V clock continuity, reconnect and SCTE-35 with reproducible synthetic fixtures, not real broadcasts.
