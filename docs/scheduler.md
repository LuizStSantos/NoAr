**Scheduler**
================

The scheduler plays events over whatever is on air: a commercial break, a
clip, or a live source with a start and an end. Events run once or on chosen
weekdays, and the output never stops while they take over.

Open **Scheduler** in the web interface, drag a file or a live onto the
timeline and choose how it goes on air.

## How an event enters

| Mode | `entry_mode` | What happens |
| --- | --- | --- |
| Wait for the clip | `after_clip` | The clip on air plays to its end; the event starts right after it. Nothing is cut. |
| Interrupt and return | `interrupt` | The clip on air is cut at the exact time. |

## What happens afterwards

| Mode | `after_mode` | What happens |
| --- | --- | --- |
| Continue where it stopped | `continue` | The programme resumes at the exact point it left, so the rest of the day runs later by the event's length. |
| Back on the clock | `clock` | The programme jumps to what the playlist has at the current time; the part that the event covered is skipped. |

The delay of `continue` lasts until the end of the playlist day: the next
day's playlist starts on time.

If an RTMP or SRT live ingest is on air when an interrupting event is due, the
event waits until the ingest ends.

## Lives

A live is a folder in the storage, normally created from the web interface:
**New live** in Storage (where the folder of a live also shows its address,
a signal test and its next time in the Scheduler) or in the Scheduler. Both use
the same form: name, signal, poster, wide image, guide title, description,
genre and rating; the Scheduler adds the time fields.

```text
Lives/
  Big Match/
    signal.live   one line: the source address
    tvshow.nfo    guide data, as for any program
    poster.jpg    guide poster
    fanart.jpg    wide image
```

Supported addresses: `srt://` (caller), `rtmp://`, `rtmps://`, `http(s)://`
HLS playlists, `udp://`, `rtsp://` and `rist://`. NoAr connects to the source
itself, so no port has to be opened for it.

A live event needs an end time. When it is due, NoAr first checks that the
source answers; the programme is only cut once it does. If the signal is lost
during the live, the channel's filler (or black) airs while NoAr retries; after
the event's retry time (`retry_seconds`, two minutes by default) the programme
continues.

The output never stops for a live source. About ten seconds before a live
starts, NoAr connects to it and keeps receiving, holding only what came since
the last key frame (a second or two), so the switch is immediate. That
connection is also the signal check: the source is called once, which
matters for SRT sources that accept a single caller. If the source is not
ready by then, NoAr keeps trying while the programme or the filler is airing,
and while it connects the last picture stays on screen with silence. The
first clip of an event and the next clip of the playlist are opened the same
way, shortly before their turn, and so is the clip the programme goes back
to, ten seconds before a live or an event ends; the live's connection then
closes beside the clip on air, so leaving a live is as immediate as entering
it. Stopping or restarting the playout ends a
live on air right away; the next start puts it back on air if its time is
not over.

Avoid pointing a live at one of the channel's own [live inputs](live_ingest.md)
(the same port): NoAr would be calling itself. Send the encoder to the live
input instead, which takes over the channel on its own.

## Scheduled graphics

**Schedule graphics** puts the logo and the L-shape on a timetable without
touching what plays: at the start time NoAr applies the changes, and at the
end time it undoes them (the L-shape goes off and the logo goes back to the
channel's, where the channel puts it). Without an end, the change stays until
something else changes it. An end before the start means the next day.

An event can:

- bring the L-shape in or take it off, with its graphic, the corner the
  picture goes to and its size;
- show the channel's logo, hide it, or swap it for another image;
- move the logo to one of nine places, back to the channel's place, or to a
  free `X% × Y%` position.

Scheduled graphics have their own lane on the timeline and a list under the
day's events, where they are edited or removed. They repeat like the other
events: once, every day or on chosen weekdays. NoAr checks them every second
while the playout runs; one whose start passed more than a minute ago while
the playout was stopped is applied only if its end is still ahead.

## Guide

Events appear in the [XMLTV guide](xmltv.md) at the time they air, and the
programme around them moves the same way the player moves it. Events with
`show_in_guide` off (the default for breaks) are added to the programme they
interrupt instead of being listed; events made only of clips from a
[commercials folder](commercials.md) are never listed and take the logo off
while they play. A live's guide entry uses its folder's `tvshow.nfo`,
poster and wide image.

## API

All routes need a login with access to the channel.

| Method | Endpoint | Purpose |
| --- | --- | --- |
| `GET` | `/api/schedule/{id}?date=YYYY-MM-DD` | Occurrences of a day with their state (`running`, `done`, `failed`) |
| `GET` | `/api/schedule/{id}/events` | All events |
| `POST` | `/api/schedule/{id}` | Add an event |
| `PUT` | `/api/schedule/{id}/{event}` | Change an event |
| `DELETE` | `/api/schedule/{id}/{event}` | Remove an event |
| `POST` | `/api/schedule/{id}/{event}/trigger` | Play an event now, cutting what is on air |
| `POST` | `/api/schedule/{id}/stop` | Stop the event on air and return to the programme |
| `GET` | `/api/schedule/{id}/status` | Event on air, waiting events and the next 24 hours |
| `GET` | `/api/schedule/{id}/lives` | Lives in the storage |
| `POST` | `/api/schedule/{id}/lives` | Create or update a live: `{"name", "url", "guide": {"title", "plot", "genre", "rating"}}` |
| `PUT` | `/api/schedule/{id}/lives` | Change the address of a live, keeping its guide data: `{"path", "url"}` |
| `POST` | `/api/schedule/{id}/lives/test` | Check a source: `{"url"}` |
| `GET` | `/api/schedule/{id}/graphics` | All scheduled graphics |
| `POST` | `/api/schedule/{id}/graphics` | Add scheduled graphics |
| `PUT` | `/api/schedule/{id}/graphics/{event}` | Change scheduled graphics |
| `DELETE` | `/api/schedule/{id}/graphics/{event}` | Remove scheduled graphics |

An event:

```json
{
  "kind": "media",
  "title": "Break 13h",
  "sources": ["Breaks/block-a.mp4", "Breaks/block-b.mp4"],
  "start_date": "2026-10-14",
  "start_time": "13:00:00",
  "end_time": null,
  "repeat_days": 62,
  "entry_mode": "after_clip",
  "after_mode": "continue",
  "show_in_guide": false,
  "retry_seconds": 120,
  "enabled": true
}
```

`kind` is `media` or `live`; a live takes one `.live` file in `sources` and
needs `end_time` (an end before the start means the next day). `repeat_days`
holds weekday bits: Sunday 1, Monday 2, Tuesday 4, Wednesday 8, Thursday 16,
Friday 32, Saturday 64; `0` plays once on `start_date`, `62` is Monday to
Friday, `127` every day. Paths are relative to the channel storage.

Scheduled graphics:

```json
{
  "title": "Sponsor",
  "actions": {
    "logo": "show",
    "logo_image": null,
    "logo_position": "90%:50%",
    "lshape": "on",
    "lshape_graphic": "Graphics/sponsor.png",
    "lshape_corner": "bottom-left",
    "lshape_scale": 70
  },
  "start_date": "2026-10-27",
  "start_time": "21:00",
  "end_time": "21:05",
  "repeat_days": 0,
  "enabled": true
}
```

`actions` takes the same fields as a [quick scene](logo.md) without its name:
`logo` is `keep`, `show`, `hide` or `replace` (with `logo_image`); `lshape` is
`keep`, `on` or `off`; `null` leaves a field as it is.
