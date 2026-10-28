# NoAr FFmpeg Specialist — Reference Guide

A standalone Claude Code project skill (`/noar-ffmpeg`) plus non-voting advisory subagent, designed for the NoAr Rust playout engine. Its reference source is the **official FFmpeg documentation hub** at https://www.ffmpeg.org/documentation.html.

## Why it is separate

The existing `noar-media-engine-reviewer` is a **voting** backend auditor. The new `noar-ffmpeg-docs-specialist` researches FFmpeg details and reports evidence to the main session or media reviewer but **does not vote**. This keeps the three-of-five council rule intact.

## Documentation coverage

`SOURCES.json` lists the official command-line manuals (`ffmpeg`, `ffplay`, `ffprobe`), component references (codecs, formats, protocols, filters, devices, bitstream filters, utilities and converters), FFmpeg library references, general docs, developer/FATE resources, and Doxygen API entry points. The index may change upstream: the hub remains the source of truth for newly published pages.

**This does not bundle, clone, or embed all FFmpeg manuals.** Instead, it makes the official documentation available on demand and reads only small relevant excerpts, reducing token consumption and avoiding stale copies. Cached extracted documentation is local-only and may be stale when offline.

FFmpeg online reference is regenerated frequently and can reflect a newer development revision than a deployed binary. For Rust API questions, inspect both native FFmpeg version/build and the `ffmpeg-next` binding.

## Quick usage

From NoAr repository root:

```sh
python3 .claude/scripts/ffmpeg_docs.py list
python3 .claude/scripts/ffmpeg_docs.py topics 'srt latency'
python3 .claude/scripts/ffmpeg_docs.py search 'latency' --manual protocols --limit 4
python3 .claude/scripts/ffmpeg_docs.py topics 'pts dts timebase'
python3 .claude/scripts/ffmpeg_docs.py search 'time base' --manual ffmpeg --limit 3
python3 .claude/scripts/ffmpeg_docs.py version
python3 .claude/scripts/ffmpeg_docs.py capabilities
```

Claude Code command:

```text
/noar-ffmpeg reference srt latency and reconnection
/noar-ffmpeg analyze backend/engine/src/output/encoded.rs timestamp handling
/noar-ffmpeg plan HLS timestamp discontinuity recovery
/noar-ffmpeg review SCTE-35 insertion path
/noar-ffmpeg verify
```

For a specific FFmpeg API page linked from official Doxygen, use `--url` instead of `--manual`:

```sh
python3 .claude/scripts/ffmpeg_docs.py search 'av_packet_rescale_ts' --url 'https://www.ffmpeg.org/doxygen/9.0/group__lavc__packet.html'
```

Only direct HTTPS HTML pages on `www.ffmpeg.org` or `ffmpeg.org` are accepted. The URL and the selected search result should be checked against the relevant FFmpeg version. The example URL is illustrative; verify that the particular Doxygen page exists.

`--offline` reads cached manuals only. `--refresh` requests a new official copy. `--cache-dir <directory>` chooses another directory. No Python package installation is required.

## Evidence levels

- `DOCUMENTED`: confirmed in official documentation at a provided URL.
- `SOURCE_VERIFIED`: read the corresponding NoAr implementation, with path and line.
- `RUNTIME_VERIFIED`: executed a reproducible staging test and recorded output.
- `INFERENCE`: reasoned but not confirmed in source/runtime.
- `NOT_VERIFIED`: unavailable environment, source or test.

Do not confuse documented features with available features in the installed FFmpeg build. A C API feature is not necessarily exposed by the chosen Rust crate methods.

## Operational precautions

The tool does not execute media operations. `capabilities` executes read-only `ffmpeg`/`ffprobe` identification commands on the current environment, if installed. Actual SRT/RTMP/HLS/SCTE-35 test traffic requires isolated staging and explicit permission. No real stream credentials should be included in prompts, generated command lines, docs caches or commits.

See `docs/ai/ffmpeg/INSTALL.md` for initial setup, and `docs/ai/INSTALL.md` for the ongoing NoAr AI toolkit workflow.
