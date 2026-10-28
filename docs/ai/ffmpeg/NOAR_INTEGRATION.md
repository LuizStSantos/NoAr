# FFmpeg ↔ NoAr integration notes

## Where the FFmpeg code lives

- Root Cargo workspace has `backend/app` (`noar`) and `backend/engine` (`noar-engine`).
- `backend/engine/Cargo.toml` declares `ffmpeg-next` with selected native features and optional `ffmpeg-filter` / `ffmpeg-device` feature flags.
- Core playback/timestamps: `backend/engine/src/playout.rs` and related `input/` modules.
- Encoding and muxing: `backend/engine/src/output/encoded.rs`, plus protocol-specific output modules.
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
