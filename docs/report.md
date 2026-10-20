**Report**
================

NoAr records what actually goes on air, one entry per item, as it happens.
The report is built from these entries for the time that already aired, and
from the playlist and the Scheduler for what is still to come.

A report covers whole playlist days: from the playlist's day start (for
example 06:00) to the same time the next day.

## The Relatório tab

**Relatório** in the menu lists a day, or a period of up to 62 days, item by
item: start, end, length, title, kind (program, commercial, live, waiting
screen, live ingest), where it came from (playlist or Scheduler event) and how
it went. A line marks now: above it the actual times, below it the plan.

- **Filters**: the whole range, only what already aired, or only what is to
  come; the kinds to list; and a field to narrow the report to a file (a
  commercial), a whole program or commercials folder, a live or a Scheduler
  event. Several can be chosen at once. Narrowed to items, the list numbers
  each time on air and names the programme it aired after.
- **Period**: from one day to another, or this week, this month or last
  month, day by day with the totals, or every item.
- **Downloads**: a PDF (A4, to print or send, with the summary and every item)
  and a spreadsheet (CSV in UTF-8 that opens in Excel and LibreOffice, one line
  per item with the file path). Both hold what the list shows, with the
  filters chosen, so the report of one commercial for a month is one file.

Before NoAr recorded what aired (see below), the list shows what was planned,
with a notice; stretches of more than a minute with nothing on record show as
off air.

## What is recorded

An entry opens when an item starts and closes when it ends:

| Item | Recorded as |
| --- | --- |
| Clip of the playlist (or of the folder, in folder mode) | `program`, or `commercial` for clips from a [commercials folder](commercials.md) |
| Clip of a scheduled event | `program` or `commercial`, with the event |
| Scheduled live | `live`, with the event and the live folder's `signal.live` |
| Live ingest taking over | `ingest`, named after the listener; its address is left out, as it can hold a stream key |
| Filler or black | `filler`, with the missing clip or the live without signal it stands in for |

Each entry keeps its storage path, the planned length and, for scheduled
events, when the event was due, so a break that waited for the end of a clip
shows both times.

## How an item ends

| Status | Meaning |
| --- | --- |
| `completed` | Played to its planned end |
| `cut` | Ended early. The reason is `event` (a scheduled event cut in), `ingest` (live ingest took over), `skipped` (skipped in the Player or the event was stopped) or `signal_lost` (a scheduled live lost its signal) |
| `failed` | The clip could not be played; black or the filler aired instead |
| `stopped` | The playout stopped. After a crash the end is unknown: the entry ends where the item would have ended, or when NoAr started again, whichever is earlier, with the reason `interrupted` |
| `on_air` | Still on air |

Entries are kept in the database (`as_run` table) with times in Unix
milliseconds. Only what airs from this version on is recorded; earlier days
have no record of what actually aired.

## What is still to come

From now on, the report lists the plan: the playlist clips, moved by the
scheduled events the way the player airs them (an event that waits for the end
of a clip starts after it; afterwards the programme continues where it stopped
or on the clock). The current day also follows the player's time shift, so the
plan picks up where the item on air ends. Days without a playlist only have
their scheduled events.

## API

All routes need a login with access to the channel.

| Method | Route | Purpose |
| --- | --- | --- |
| `GET` | `/api/report/{id}?from=YYYY-MM-DD&to=YYYY-MM-DD` | Report of the playlist days `from` to `to`, both included, at most 62; `to` defaults to `from` |

```json
{
  "timezone": "America/Sao_Paulo",
  "day_start": "06:00:00",
  "now": 1792494000000,
  "recorded_since": 1792490693147,
  "days": [{"date": "2026-10-20", "start": 1792476000000, "end": 1792562400000, "playlist": true}],
  "items": [{
    "day": "2026-10-20",
    "start": 1792490693147,
    "end": 1792490769586,
    "title": "Novela das Seis",
    "sub_title": "Capítulo 12",
    "source": "Novela das Seis/cap 12.mp4",
    "program": "Novela das Seis",
    "kind": "program",
    "origin": "playlist",
    "event_id": null,
    "event_title": null,
    "scheduled_at": null,
    "state": "aired",
    "status": "cut",
    "reason": "skipped",
    "detail": null,
    "planned_seconds": 316.9,
    "replaces": null
  }]
}
```

Times are Unix milliseconds. `state` is `aired`, `on_air` (its `end` is when
it is due to end), `planned`, or `unrecorded` for the plan of a time before the
first record. A stretch of more than a minute without any record, while the
playout was stopped, shows as an item of kind `gap`. `program` is the storage
folder of the program the clip belongs to, when a `.nfo` describes it.
