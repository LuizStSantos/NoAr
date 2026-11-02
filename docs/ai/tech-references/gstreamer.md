# GStreamer reference navigation

GStreamer is NoAr's mandatory media runtime on the integration branch. The
canonical research workflow lives in
[noar-gstreamer](../../../.claude/skills/noar-gstreamer/SKILL.md); Codex uses the
[shared adapter](../../../.agents/skills/noar-gstreamer/SKILL.md).

| Question | Project entry point | Official reference |
| --- | --- | --- |
| Runtime setup and CPU frame conversion | `backend/engine/src/gstreamer.rs` | Application development, VideoInfo/VideoConverter Rust APIs |
| Client inputs, caps, seeking and timestamps | `input/gstreamer.rs`, `input/preload.rs`, `playout.rs` | uridecodebin3, hlsdemux2, audiorate, AppSink, state changes and synchronization |
| Persistent encoding and separate destinations | `output/gstreamer_output.rs`, `output/pipeline.rs` | AppSrc, segments, encoder/parser/muxer pad caps |
| SRT/RTMP/UDP reconnects | `output/link.rs` | Installed srtsink/rtmp2sink/udpsink properties and units |
| HLS, WebVTT and recording | `output/gst_hls.rs`, `output/recording_native.rs` | splitmuxsink, MPEG-TS and HLS RFC 8216 |
| Service metadata and SCTE-35 | `output/gst_metadata.rs`, `output/scte35.rs` | GStreamer MPEG-TS APIs and observed output packets |
| Application scheduler | `backend/app/src/player/schedule.rs` | Existing NoAr Clock/Continue contracts and native runtime evidence |

Paths in the engine column after the first row are relative to
`backend/engine/src/`. Confirm them in current source and use the repository map
for symbols. `backend/engine/src/utils/media_capabilities.rs` detects native
plugins; legacy public `Ffmpeg*` names preserve API compatibility.

Live/file playback uses `uridecodebin3` so adaptive HLS uses `hlsdemux2`.
Selected-stream messages also identify audio-only and video-only inputs.
Native `audiorate` preserves the sample clock across MPEG-TS PCR adjustments
and millisecond FLV timestamps before frames enter the playout timeline.
When converting its nanosecond PTS to integer audio samples, round to the
nearest sample. Flooring a quantized 1024-sample timestamp can put it one
sample behind, causing unnecessary PCM trimming or padding. The native audio
normalization regression also checks the Rust adapter's resulting sample PTS.
Graphics/discovery helpers can still use older native GStreamer elements;
they do not select another playback engine. Relevant official references:
[uridecodebin3](https://gstreamer.freedesktop.org/documentation/playback/uridecodebin3.html),
[hlsdemux2](https://gstreamer.freedesktop.org/documentation/adaptivedemux2/hlsdemux2.html),
[audiorate](https://gstreamer.freedesktop.org/documentation/audiorate/index.html).

For MPEG-TS HLS startup, inspect `guard_hls_transport_rollover` before changing
clock offsets. Native 1.26.2 adaptive HLS forces its internal TS demuxer to
ignore PCR; an audio timestamp assigned to an extra 33-bit cycle can make the
adaptive track insert a 26-hour gap. NoAr unwraps those compressed timestamps
against their current segment, only inside `hlsdemux2`. Relevant versioned
sources are the [adaptive parser setup](https://github.com/GStreamer/gstreamer/blob/1.26.2/subprojects/gst-plugins-good/ext/adaptivedemux2/gstadaptivedemux-stream.c)
and [adaptive track gap handling](https://github.com/GStreamer/gstreamer/blob/1.26.2/subprojects/gst-plugins-good/ext/adaptivedemux2/gstadaptivedemux-track.c).
The sample reader also uses its bounded read timeout to let both negotiated
decoder tracks arrive before choosing their timestamp order. Single-track
sources and stalled tracks still make progress without an unlimited wait.

Check the native version with `pkg-config` and each factory with
`gst-inspect-1.0`. Pin Rust documentation to the version in Cargo.lock. Some
official web pages describe newer native releases; compare every property with
the installed plugin. If network policy blocks a documentation host, use the
installed bindings and introspection, and report the unavailable reference.

The standalone Python prototype in `qa/gstreamer` is separate evidence. Verify
the integrated NoAr process, actual source transitions, wall-clock receiver
progress, A/V packet timing and persistent MediaMTX sessions before reporting
continuity. No runtime claim follows from this navigation guide.
