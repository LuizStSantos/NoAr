# Official FFmpeg reference routing for NoAr

Start from the official hub: https://www.ffmpeg.org/documentation.html. Keep only the relevant manual excerpts in the Claude context.

| NoAr question | Read official manuals | Inspect NoAr source |
| --- | --- | --- |
| SRT ingest/reconnect/latency | `protocols`, `formats`, Doxygen libavformat | `backend/engine/src/input/`, `backend/engine/src/utils/helper.rs` |
| RTMP/RTMPS output | `protocols`, `formats`, `libavformat` | `backend/engine/src/output/`, output config |
| HLS output/segments/playlists | `formats`, `protocols`, `ffmpeg`, libavformat | `backend/engine/src/output/hls.rs`, `encoded.rs` |
| PTS/DTS/clock/time bases | `ffmpeg`, `utils`, Doxygen `libavutil`/`libavformat` | `backend/engine/src/playout.rs`, `input/timestamps.rs`, `output/timestamps.rs` |
| Encoder, H.264/H.265/AV1 | `codecs`, `ffmpeg`, Doxygen `libavcodec` | `backend/engine/src/output/encoded.rs`, `utils/ffmpeg_capabilities.rs` |
| Audio loudness/resample | `filters`, `resampler`, `libswresample` | `backend/engine/src/audio_mixer/`, `analysis/` |
| Scaling, deinterlace, overlays | `filters`, `scaler`, `libswscale` | `backend/engine/src/compositor/`, `playout.rs` |
| SCTE-35 and transport signaling | `bitstream-filters`, `formats`, Doxygen, implementation source | `backend/engine/src/output/scte35.rs` |
| Recording and multi-output | `formats`, `ffmpeg`, Doxygen `libavformat` | `backend/engine/src/output/recording.rs`, `encoded.rs` |
| Input/output devices | `devices`, `libavdevice` | `backend/engine/src/output/desktop/`, feature-gated modules |
| CLI diagnostics | `ffmpeg`, `ffprobe`, `ffprobe-all` | tests, fixtures and orchestration scripts |
| FATE and testing patterns | `fate`, `developer`, Doxygen | relevant Rust unit tests and synthetic fixture tests |

Each manual ID resolves through `SOURCES.json`. The paths above are navigation hints; verify the files exist in the current NoAr branch before citing them. The official docs may not cover NoAr-specific SCTE-35 behavior; verify the implementation and applicable specifications separately rather than fabricating documentation claims.
