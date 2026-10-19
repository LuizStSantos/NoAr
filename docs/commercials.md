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

Scheduled events made only of commercials never show in the guide; their
"show in the guide" switch is off and locked in the Scheduler.

## Marking a folder

In **Armazenamento**, open the folder and choose **Marcar como pasta de
comerciais** (offered on folders without a program sheet). The folder gets a
"comercial" tag in the folder tree and in the Player library, and the switch on
its card removes the mark.

The mark is the hidden file `.comercial` inside the folder, so it stays when the
folder is renamed or moved. The storage root itself cannot be marked.

## API

| Method | Endpoint | Purpose |
| --- | --- | --- |
| `PUT` | `/api/file/{id}/commercial` | Mark or unmark a folder: `{"path": "Breaks", "commercial": true}` |
| `POST` | `/api/file/{id}/browse` | Lists `commercial_folders` among the subfolders, and `commercial`, the commercials folder the browsed folder lies in |
| `POST` | `/api/playlist/{id}/programs` | Tells, per clip, whether it is a `commercial` |
