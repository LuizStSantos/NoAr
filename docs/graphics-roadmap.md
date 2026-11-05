# Graphics roadmap

Status: **in progress**. Template storage, rendering, the take / out API,
On air, the library and the Composer UI are implemented. Migration of the
existing channel graphics into templates remains pending. See the
[implementation handoff](graphics-handoff/README.md) for scope and test evidence.
This document records the
agreed design for the new Graphics tab, so the work can land in small releases
without breaking what operators use today.

## Goals

- Operators build their own graphics (lower thirds, topic bars, sponsor bugs,
  QR codes, L-shapes) as **templates**, instead of the fixed logo / L-shape /
  ticker set of today.
- The same templates are used everywhere: on air by an operator, inside a
  clip at marked times, from rundowns and from external controllers.
- The system is open: channels define their own templates, fields, tracks and
  rundowns. Nothing in the UI assumes one channel's way of working.
- On-air continuity comes first. Every phase keeps today's logo, L-shape,
  ticker, scenes and graphics schedule working, and keeps their API routes.

## What exists today

- Engine compositor (`backend/engine/src/compositor`): `logo`, `lshape`,
  `text` (static and runtime text, ticker) and `blend`/`overlay` helpers.
- Order in `backend/engine/src/playout.rs`: the L-shape is applied first, the
  picture is shrunk into its corner, then the logo and texts are drawn **over
  the full frame**.
- App: `GraphicsScene` (logo and L-shape changes), `graphics_events`
  (scheduled scenes), logo marks inside clips, and the Graphics view with the
  pages live, logo, lshape, subtitles, scenes and api.

## Design decisions

1. **Template = layers + fields.** A template is a list of layers (text,
   image, box, clock, ticker, video corner) with position and size in percent
   of the frame, entry/exit animation presets and named `{fields}`. The
   Composer defines the fields; every place that uses a template shows exactly
   those fields.
2. **The L-shape is a graphic, not an always-on element.** It is a template
   kind that can be taken and removed like any other. Only the logo, clock and
   ticker stay as "always on" channel elements.
3. **Open tracks for graphics inside a clip.** A clip's graphics timeline has
   tracks the user creates, renames and deletes; any template can go on any
   track. A channel can save its track set as the default for new clips.
4. **Graphics during an L-shape.** Each placed graphic chooses what happens
   when it overlaps an L-shape: *follow the picture* (default; drawn inside the
   shrunk picture at the same scale), *wait until the L-shape ends*, or *skip*.
   In the engine, *follow* means compositing that graphic onto the picture
   **before** the L-shape shrinks it, which changes today's order for those
   graphics only. The logo and ticker keep their current behaviour.
5. **Graphics leave the Player view.** The Player shows a read-only status
   line and links to Graphics › On air, where the operator works with a
   preview.
6. **Control API.** A graphics key and `take`, `out`, `update`, `next` and
   `state` endpoints per graphics channel, for Stream Deck, vMix, OBS or
   `curl`. Existing logo, L-shape and scene endpoints stay.

## Phases

| Release | Scope |
| --- | --- |
| 2.3 | Templates and On air: template storage and API, engine renderer for text/image/box layers with fields and animation presets, operator rundown with take/out, migration of the current logo, L-shape and ticker settings into the new tab. |
| 2.4 (UI implemented) | Composer: visual editor with drag, resize in % or px, proportional lock, layers list (rename, hide, delete, undo), image upload and animations. |
| 2.5 | Rundowns: several rundowns per channel, guests list and CSV import for fields, duplicate, "when to use". |
| 2.6 | Graphics inside clips and series: open tracks, in/out marks anchored to start, end of opening, middle or end, apply to a folder of episodes, L-shape overlap rule. |
| 2.7 | Data and API: control API, webhooks, data sources (RSS, CSV, HTTP) for fields, command log and as-run entries. |

## Verification per phase

- Unit tests for template parsing, field substitution and timing.
- Engine tests on synthetic frames for layer placement, animation and the
  L-shape overlap rule.
- The soak test (`qa/soak`) stays green, and a staging run with real output
  confirms graphics appear on air without stalls before each release.
