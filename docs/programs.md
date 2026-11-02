# Assembled programs

A program delivered in blocks (block 1, block 2, …) is assembled once in
**Storage** into one file, with commercial breaks between the blocks. The
file carries the name you choose; drag it into a playlist or schedule it like
any clip.

## Making one

1. *Storage*: open the folder that should hold it and press **New program**.
2. Enter the **title** (shown in the playlist and the guide) and the **file
   name**.
3. **Add block** for each block, in order, picking the clips in the storage.
4. **Break after this block** adds a break; pick its commercials. They must
   come from a [commercials folder](commercials.md), so they play and are
   signalled like any commercial (SCTE-35 when it is on).
5. Click a **live** in the storage (a folder with the broadcast icon) to add
   it as a part, for its default length; change the minutes beside it.
6. Reorder with the arrows, remove parts, then **Save program**.

The editor shows each block's and break's length and the total. Each clip
plays its part after its [trim](trim.md) and with its own marks (stretches
without logo, L-shape, breaks marked inside the video), as they are when it
airs: a trim made in Storage after the program was saved applies too.

A live part airs at the live's current address for the minutes given, like a
live in a playlist; it connects when its turn comes.

## Using it

- **Playlist**: drag the program from the library like a clip. It shows as
  one entry with its title and total length.
- **Scheduler**: pick it as the clip of a media event.
- **Guide (XMLTV)**: one programme with the title, breaks included.
- **As-run log**: every block and commercial on its own line.

## Changing it

Click the program in Storage, change it and save. Playlists and scheduled
events point to the file, so the change applies to every airing still to
come: the playout reads the program when it airs it. A playlist read in the
editor or for the guide takes the program's current length and title, and
scheduled events that air it take its new length when it is saved. If the
length changes, what follows in that day's playlist starts earlier or later.

A program can't contain another program, holds up to 500 clips and
runs up to 24 hours. If its file can't be read when it airs, its playlist slot
keeps its length and fails over like a missing clip.

## File format

`Storage/…/<name>.program` (JSON), written by the editor:

```json
{
  "title": "Jornal da Noite — ep. 12",
  "segments": [
    { "kind": "block", "source": "Received/jornal/b1.mp4", "in": 0.0, "out": 1320.5 },
    { "kind": "break", "clips": [{ "source": "Commercials/a.mp4", "in": 0.0, "out": 30.0 }] },
    { "kind": "block", "source": "Received/jornal/b2.mp4", "in": 0.0, "out": 1104.0, "file": 1104.0 },
    { "kind": "live", "source": "Lives/Jornal/signal.live", "in": 0.0, "out": 1800.0 }
  ]
}
```

`in`/`out` are the part of each clip that airs, in seconds; `file` is the
clip's whole length, which lets a later trim apply. A `live` part names the
live's `.live` file and airs for `out` seconds.

## API

| Method | Endpoint | Description |
| --- | --- | --- |
| `GET` | `/api/schedule/{id}/programs?path=<file>` | Read a program file. |
| `PUT` | `/api/schedule/{id}/programs` | Create or replace one: `{"path": "Programs/x.program", "title": "…", "segments": [{"kind": "block", "source": "…"}, {"kind": "break", "clips": ["…"]}, {"kind": "live", "source": "Lives/x/signal.live", "duration": 1800}]}`. The clip lengths are read by the server; returns the saved file. |
