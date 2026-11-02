---
name: noar-ffmpeg
description: Independently research official FFmpeg CLI, protocol, filter and C library documentation for the NoAr Rust playout engine using scoped, version-aware lookups.
---

# NoAr FFmpeg Specialist (independent project skill)

User command: `$ARGUMENTS`

This skill is **separate** from `/noar-intelligent-development`. It may be invoked directly or be consulted by that workflow. It never votes in the existing five-agent audit councils and does not replace `noar-media-engine-reviewer`.

GStreamer is NoAr's sole playout engine on this integration branch. Use
`/noar-gstreamer` first for pipeline control, native encoders/transports, clocks,
states, seeking and caps. This skill covers remaining FFmpeg frame-memory
compatibility, codec dependencies and fixture/inspection tools; do not treat its
CLI options as GStreamer properties or revive removed input listeners.

## Supported requests

- `reference <topic>`: route to relevant official FFmpeg manuals, extract short passages, and connect findings to NoAr code.
- `analyze <feature|file|symbol>`: inspect a small code range and compare its FFmpeg API use with the appropriate documentation and locally installed library version.
- `plan <feature>`: propose bounded, testable implementation steps with source references, risks and rollback.
- `review <topic>`: read-only FFmpeg-specific review; never count as a sixth Backend Council vote.
- `verify`: examine local version/build/protocol/filter/encoder availability; report NOT_AVAILABLE where the environment lacks FFmpeg.
- `help`: give the commands and usage, not a full repository scan.

## Reference retrieval workflow

1. Consult `docs/ai/ffmpeg/README.md`, `docs/ai/ffmpeg/NOAR_INTEGRATION.md`, and `docs/ai/ffmpeg/SOURCES.json`. Navigate code with `docs/ai/repository-map.md` and targeted `--find` lookups if the Token Saver is installed.
2. Discover relevant manuals WITHOUT reading them all:
   `python3 .claude/scripts/ffmpeg_docs.py topics '<topic>'`
3. Read only short on-demand excerpts from official manuals, for example:
   `python3 .claude/scripts/ffmpeg_docs.py search 'srt' --manual protocols --limit 4 --context 3`
4. For Rust binding work use the official **version-specific Doxygen C API**, the installed FFmpeg headers and the `ffmpeg-next` crate source/definitions; do not mistake CLI options for C API members or for Rust binding methods. `ffmpeg-next = 9` is a crate dependency, not proof of the native FFmpeg library versions at runtime.
4a. For deeper C API references, navigate official Doxygen to a specific function or group page, then use `python3 .claude/scripts/ffmpeg_docs.py search '<function>' --url 'https://www.ffmpeg.org/doxygen/<version>/...html'`. Only direct official FFmpeg documentation URLs are accepted.
5. Get installed versions with `python3 .claude/scripts/ffmpeg_docs.py capabilities` if possible. If the host lacks binaries, check the build image, Cargo.lock, build flags and CI environment, explicitly labeling uncertain version/capability details.
6. State exact source URL, retrieved/manual version if known, relevant option/function and the NoAr file/line. Mark claims `DOCUMENTED`, `SOURCE_VERIFIED`, `RUNTIME_VERIFIED`, `INFERENCE`, or `NOT_VERIFIED`.
7. Do not download or paste entire FFmpeg documents into the chat or context. Use source names and bounded excerpts; cache automatically in `.cache/noar-ffmpeg-docs/` (ignored by Git if installed instructions are followed). Remote calls require normal network access, so report `NOT_AVAILABLE` honestly if blocked.

## Media-engine invariants

- Never assume new FFmpeg documentation describes the exact installed binary; the official online manuals are updated frequently.
- Respect per-stream time bases, PTS/DTS and rescaling, A/V sync, clock and buffering, muxer timestamp constraints, sample/pixel formats, ownership/lifetimes, graceful cancellation, stream recovery and muxer finalization.
- Treat live SRT, HLS, RTMP, UDP, SCTE-35, playlists, overlays, subtitles, recording and hardware acceleration as separate surfaces with explicit failure modes.
- Test in a disposable environment with synthetic media and no production output. Never start, stop or reconfigure live playout without explicit authorization.
- Separate documentation research, code-level confirmation, and actual media-output verification. A cited option does not prove a codec or protocol exists in a particular build.

## Collaboration with existing NoAr system

For `/noar-intelligent-development implement ...` or `audit ...`, the coordinator can call `noar-ffmpeg-docs-specialist` as an **advisory non-voting** agent when the question requires FFmpeg expertise. The existing `noar-media-engine-reviewer` remains a Backend Council voter. Hand over brief evidence with sources, no broad duplicate repository scans. Have `noar-documentation-guardian` evaluate doc changes when appropriate. See `docs/ai/ffmpeg/AGENT_HANDOFF.md`.

## Output structure

- **Question and scope** (one sentence)
- **Version/build context** (confirmed vs unknown)
- **Official references** (2-5 relevant URLs, never dozens)
- **NoAr call sites** (precise file and small line range)
- **Conclusion and compatibility concerns**
- **Proposed tests / observed tests** (clearly separate)
- **Documentation handoff** (if needed)

Do not modify source for `reference`, `review`, or `verify`. For `plan`, provide a plan rather than making changes. For implementation use the main NoAr workflow with explicit approval and corresponding test gates.
