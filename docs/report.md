**Report**
================

NoAr records what actually goes on air, one entry per item, as it happens.
The report is built from these entries for the time that already aired, and
from the playlist and the Scheduler for what is still to come.

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
