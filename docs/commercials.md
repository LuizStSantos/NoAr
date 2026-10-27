**Commercials folders**
================

Mark a folder as a commercials folder and everything in it, subfolders
included, plays as a commercial wherever it is used: in the daily playlists, in
folder mode, in scheduled events and in days made with Generate. Nothing has to
be marked clip by clip, and what was scheduled before the folder was marked
follows the rule too.

A commercial:

- is not listed in the [XMLTV guide](xmltv.md): its time is added to the
  programme around it, like any ad;
- takes the logo off while it plays. The logo comes back, with its fade, on the
  next clip that is not a commercial, also after a scheduled break that
  interrupted a programme.

With [SCTE-35](ad_breaks.md) on, each run of commercials is also marked as
an ad break in the SRT or UDP output, for a FAST platform or an ad server to
replace.

Scheduled events made only of commercials never show in the guide; their
"show in the guide" switch is off and locked in the Scheduler.

## A break inside a video

When a programme file has a commercial break edited into it, mark the
stretches where the logo goes off instead: in **Storage**, open the
video's **Trim, logo and breaks in this video** (or the same button in the Player's clip editor), choose the **Logo** tab
and mark, while it plays, where the logo goes off and where it comes back, or
type the times. Each stretch then has two handles on the bar, as the
[trim](trim.md) does: drag them to the exact frame, or select one and use the
arrow keys (Shift moves a second). Up to 16 stretches, in seconds of the video itself, so they
hold whatever part of the video a playlist plays. The logo fades out and back
in with its usual fade, wherever the video plays, from the next time it starts.

The stretches are kept next to the video in `<name>.marks.json`, like its
`.nfo` sheet, and follow it when it is renamed, moved or deleted in Storage:

```json
{ "hide": [{ "from": 750.0, "to": 990.5 }] }
```

Videos with marks show a "marks" tag in Storage. A live with a
break of its own uses the [logo API](api.md#logo-on-air) instead.

The same panel marks the break itself for SCTE-35, so FAST platforms and ad
servers can replace it: see
[breaks marked inside a video](ad_breaks.md#breaks-marked-inside-a-video).

## Marking a folder

In **Storage**, open the folder and choose **Mark as a commercials
folder** (offered on folders without a program sheet). The folder gets a
"commercial" tag in the folder tree and in the Player library, and the switch on
its card removes the mark.

The mark is the hidden file `.comercial` inside the folder, so it stays when the
folder is renamed or moved. The storage root itself cannot be marked.

## API

| Method | Endpoint | Purpose |
| --- | --- | --- |
| `PUT` | `/api/file/{id}/commercial` | Mark or unmark a folder: `{"path": "Breaks", "commercial": true}` |
| `POST` | `/api/file/{id}/browse` | Lists `commercial_folders` among the subfolders, and `commercial`, the commercials folder the browsed folder lies in |
| `POST` | `/api/playlist/{id}/programs` | Tells, per clip, whether it is a `commercial` |
