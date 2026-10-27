**Closed captions (WebVTT)**
================

NoAr carries closed captions in WebVTT format in its **HLS** output.

The captions of a video are read from a `.vtt` file next to it with the same
name (`Programme 01.mp4` → `Programme 01.vtt`). Clips without one use the
*empty file* set in **Graphics › Subtitles**, where subtitles are
turned on, and where the track name, its language and whether players show it
by default are set.

The output must be in **HLS** mode; other outputs carry no subtitles.

The track name in the master playlist needs FFmpeg's `libavformat` 61.9.100 or
newer. With an older one, NoAr still writes the subtitles and only leaves the
name out.
