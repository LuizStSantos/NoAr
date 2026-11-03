# Official FFmpeg reference routing for NoAr

Start from the official hub: https://www.ffmpeg.org/documentation.html. Keep only the relevant manual excerpts in the Claude context.

NoAr does not link FFmpeg: decoding, encoding, muxing, transports, time
bases, scaling, overlays, loudness and recording all go through GStreamer and
NoAr's own frames. Route those questions to `/noar-gstreamer` and
`docs/ai/tech-references/gstreamer.md`. The FFmpeg manuals stay useful for:

| NoAr question | Read official manuals | Inspect NoAr source |
| --- | --- | --- |
| Behaviour of a codec GStreamer takes from libav (`avenc_aac`, `avenc_ac3`, `avdec_*`) | `codecs`, Doxygen `libavcodec` | `gst-inspect-1.0 avenc_<codec>`, `backend/engine/src/output/pipeline.rs` |
| Test fixtures and independent inspection | `ffmpeg`, `ffprobe`, `ffprobe-all`, `formats`, `filters` | `qa/`, tests that call `ffprobe` (`backend/engine/src/output/gstreamer_output.rs`) |
| SCTE-35 background (why NoAr writes sections itself) | `formats`, Doxygen `libavformat`, implementation source | `docs/scte35.md`, `backend/engine/src/output/scte35.rs` |
| FATE and testing patterns | `fate`, `developer`, Doxygen | relevant Rust unit tests and synthetic fixture tests |

Each manual ID resolves through `SOURCES.json`. The paths above are navigation hints; verify the files exist in the current NoAr branch before citing them. The official docs may not cover NoAr-specific SCTE-35 behavior; verify the implementation and applicable specifications separately rather than fabricating documentation claims.
