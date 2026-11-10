# Closed captions (WebVTT)

NoAr publishes optional WebVTT subtitle tracks in its **HLS** output. Captions
come from a timed text file supplied with each video. They are separate from
Composer text/data layers and their text is not burned into the picture.

## Prepare and upload a subtitle

Save a UTF-8 `.vtt` file beside its video, with exactly the same base name:

```text
Programme 01.mp4
Programme 01.vtt
```

Use **Storage → Upload files** or drag both files into the same folder. The
picker includes `.vtt`; choose **All files** to find or download the subtitle
again. The file is `Programme 01.vtt`, not `Programme 01.mp4.vtt`. On
case-sensitive Linux storage, match the base-name case and use lowercase `.vtt`.

A minimal example is:

```vtt
WEBVTT

00:00:01.000 --> 00:00:05.000
Welcome to the programme.

00:00:06.000 --> 00:00:10.000
This is a separate subtitle track.
```

Leave a blank line after `WEBVTT` and between cues. Advanced WebVTT `STYLE`
and `REGION` blocks are not preserved by the current cue parser. Cue end times
must be later than their start times. Times refer to the original video; the output adjusts
them to the clip's position/trim in the programme.

## Configure the channel

1. Select **HLS** in the channel's output settings.
2. Open **Graphics → Subtitles** and enable **Use subtitles**.
3. Set the track name (for example `Português`) and language (`pt-BR`).
4. Keep the configured **Empty file** available for videos without a subtitle.
   The default asset is an empty WebVTT file. A manually created empty file
   contains just `WEBVTT` and a blank line.
5. Choose whether the subtitle track should be selected by default, then save.
   If the playout is already running, apply the pending restart so the HLS
   output and master playlist use the new settings.

Clips without a matching sidecar use the configured empty file; they do not
inherit the previous clip's captions. NoAr does not extract embedded subtitle
tracks or import `.srt` through this feature; provide a WebVTT sidecar.

## Watch the subtitles

Open the **HLS master playlist** in a player supporting WebVTT subtitles, then
select its CC/subtitle track. For channel 1 the usual master URL is
`/public/1/live/master.m3u8` on the NoAr server; use the URL for your
configured channel/output when it differs. The master declares the configured
name, language and default selection, and links a separate subtitle playlist.
Players may override the default according to their language/preferences.

The low-latency Program/Player monitor uses the application's preview transport;
use the HLS master to check the delivered subtitle track. Other output modes
such as desktop, SRT or UDP do not carry this WebVTT track.

For troubleshooting, check that the two filenames and folder match, the VTT
file has valid timing, HLS is enabled, the pending restart has been applied and
the client opened the master rather than only the video rendition playlist.
