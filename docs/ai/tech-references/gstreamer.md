# GStreamer reference navigation

GStreamer is NoAr's mandatory media runtime on the integration branch. The
canonical research workflow lives in
[noar-gstreamer](../../../.claude/skills/noar-gstreamer/SKILL.md); Codex uses the
[shared adapter](../../../.agents/skills/noar-gstreamer/SKILL.md).

| Question | Project entry point | Official reference |
| --- | --- | --- |
| Runtime setup and CPU frame conversion | `backend/engine/src/gstreamer.rs` | Application development, VideoInfo/VideoConverter Rust APIs |
| Client inputs, caps, seeking and timestamps | `input/gstreamer.rs`, `input/preload.rs`, `playout.rs` | AppSink, state changes, seeking and synchronization |
| Persistent encoding and separate destinations | `output/gstreamer_output.rs`, `output/pipeline.rs` | AppSrc, segments, encoder/parser/muxer pad caps |
| SRT/RTMP/UDP reconnects | `output/link.rs` | Installed srtsink/rtmp2sink/udpsink properties and units |
| HLS, WebVTT and recording | `output/gst_hls.rs`, `output/recording_native.rs` | splitmuxsink, MPEG-TS and HLS RFC 8216 |
| Service metadata and SCTE-35 | `output/gst_metadata.rs`, `output/scte35.rs` | GStreamer MPEG-TS APIs and observed output packets |
| Application scheduler | `backend/app/src/player/schedule.rs` | Existing NoAr Clock/Continue contracts and native runtime evidence |

Paths in the engine column after the first row are relative to
`backend/engine/src/`. Confirm them in current source and use the repository map
for symbols. `backend/engine/src/utils/media_capabilities.rs` detects native
plugins; legacy public `Ffmpeg*` names preserve API compatibility.

Check the native version with `pkg-config` and each factory with
`gst-inspect-1.0`. Pin Rust documentation to the version in Cargo.lock. Some
official web pages describe newer native releases; compare every property with
the installed plugin. If network policy blocks a documentation host, use the
installed bindings and introspection, and report the unavailable reference.

The standalone Python prototype in `qa/gstreamer` is separate evidence. Verify
the integrated NoAr process, actual source transitions, wall-clock receiver
progress, A/V packet timing and persistent MediaMTX sessions before reporting
continuity. No runtime claim follows from this navigation guide.
