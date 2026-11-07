# Graphics roadmap

Status: **in progress**. Template storage, rendering, take/out, On air,
the library and Composer are implemented. Channel logo, L-shape and ticker
layouts now use Composer layers too. Migration 00030 upgrades both 2.2.1 and
already converted 00029 installations automatically, preserving bindings,
identity and frozen legacy columns. Legacy APIs and clip/scene controls act
on these models. Logo/L-shape/ticker Edit opens Composer; scenes retain an
action editor in Graphics. The approved open-templates stages now include free
groups/assignments and deletion, shape primitives, live clocks, uploaded fonts
and CSV/RSS/JSON field mappings. Fresh installations start with only one
removable Logo example. Data-bound text refreshes in place; continuous scrolling
text/feed controls are still pending in Composer. WebVTT subtitles are a
separate HLS track, described in [Closed captions](closed_captions.md). See the
current handoff for executed checks and limits.

See the [implementation handoff](graphics-handoff/README.md) for the current
source-of-truth contract, runtime evidence and historical stages a–e. The
synthetic main-versus-candidate comparison includes exact pre-encode frames,
aligned HLS and the logo/L-shape transitions. Broader deployment and physical
output validation remain separate from that scoped proof.

This document records the agreed design so the work can land in small
releases without breaking existing operator workflows.

Open-templates stage 1 now uses authored groups/names, optional compatibility
assignments and image targets, assigned-model deletion and free permanent
compositions. New installations/channels start with only one removable Logo
example; updates preserve existing models. A separate completed-import state
prevents deleted models returning at restart. See the current handoff evidence.

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
  (scheduled scenes), logo marks inside clips, and Graphics navigation with On air, Templates, XMLTV logo, Subtitles and External control.
  Bound logo/L-shape/ticker templates open Composer; scenes have an action editor.

The current operator follow-up removes the legacy logo/L-shape and quick-scene
panels from On air. Its saved-model Preview works with channel models alone;
Program uses the Player's real preview component, including stopped/wait/error
states. Logo/ticker requested visibility and channel L-shape entry/exit act on
bindings. The Player now has the read-only status line described below.
Scenes remain in the library. Clock is approved in the next revision; version 2.5 rundowns remain outside
this follow-up. See the handoff screenshot gallery for executed browser evidence.

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
   when it overlaps an L-shape: _follow the picture_ (default; drawn inside the
   shrunk picture at the same scale), _wait until the L-shape ends_, or _skip_.
   In the engine, _follow_ means compositing that graphic onto the picture
   **before** the L-shape shrinks it, which changes today's order for those
   graphics only. The logo and ticker keep their current behaviour.
5. **Graphics leave the Player view.** The Player shows a read-only status
   line and links to Graphics › On air, where the operator works with a
   preview.
6. **Control API.** A graphics key and `take`, `out`, `update`, `next` and
   `state` endpoints per graphics channel, for Stream Deck, vMix, OBS or
   `curl`. Existing logo, L-shape and scene endpoints stay.

## Phases

| Release              | Scope                                                                                                                                                                                                                                     |
| -------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 2.3                  | Templates and On air: template storage and API, engine renderer for text/image/box layers with fields and animation presets, operator rundown with take/out, migration of the current logo, L-shape and ticker settings into the new tab. |
| 2.4 (UI implemented) | Composer: visual editor with drag, resize in % or px, proportional lock, layers list (rename, hide, delete, undo), image upload and animations.                                                                                           |
| 2.5                  | Rundowns: several rundowns per channel, guests list and CSV import for fields, duplicate, "when to use".                                                                                                                                  |
| 2.6                  | Graphics inside clips and series: open tracks, in/out marks anchored to start, end of opening, middle or end, apply to a folder of episodes, L-shape overlap rule.                                                                        |
| 2.7                  | Data and API: control API, webhooks, data sources (RSS, CSV, HTTP) for fields, command log and as-run entries.                                                                                                                            |

## Verification per phase

- Unit tests for template parsing, field substitution and timing.
- Engine tests on synthetic frames for layer placement, animation and the
  L-shape overlap rule.
- The soak test (`qa/soak`) stays green, and a staging run with real output
  confirms graphics appear on air without stalls before each release.

### Approved open-template revision: shape and clock implementation

Stage 2 adds rectangle, rounded rectangle, ellipse and line controls plus live
clock layers (format and channel/explicit timezone) to Composer. Actual executed
checks and output evidence are recorded in the graphics handoff. Clock is in
scope for this approved revision, superseding the earlier exclusion. Font upload
and data sources remain separate subsequent stages.

### Approved open-template revision: fonts and sources

Stage 3 provides immutable uploaded TTF/OTF assets used by both preview and
engine. Stage 4 adds Graphics Data sources (CSV/public Sheets CSV, RSS/Atom and
HTTP JSON), layer field mappings, On air record/manual selection and last-good
records. See the [operator guide](graphics-data-sources.md) and graphics handoff
for actual executed checks. This does not implement private Google OAuth,
custom HTTP headers, rundowns or the whole broader integration roadmap.

### Animated artwork and model schedules

The approved follow-up adds animated GIF image layers and natural-size first
image insertion. Scheduler's new events select Composer models, values and
source records rather than the old logo/L-shape action form; existing action
snapshots remain compatible. Migration `00034` adds the optional model reference.
See the current handoff for the exact tested revision, actual HLS/browser proofs
and the separate unchanged-static-layout parity check.
