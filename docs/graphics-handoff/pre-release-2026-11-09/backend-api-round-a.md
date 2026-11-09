# Backend API/application council review — round A

Reviewer: `noar-backend-reviewer` (one independent BACKEND voter).
Revision: `f925f608671c17c32f4788993b84d044a6621907` (`claude/branch-review-opz967`). Comparison: `origin/main` at `dd4529e2935b07d7a5bf8deb764421e4d201fc41`; stable release supplied by lead: `v2.2.1` at `6cf680f6…`.

Read-only review of application/API integration, graphics model lifecycle and scheduling/refactor boundaries. Read root/backend instructions, reviewer protocol, audit quality gates/checklist and repository map; inspected affected source and relevant existing API tests. No product source changes, build, server, migration or runtime test execution. Parent owns runtime verification. All findings below are VERIFIED_STATIC; actual output consequences remain NOT_VERIFIED at runtime. No ballot is cast before peer challenge.

## API-A1 — HIGH — One unavailable permanent overlay blocks the entire channel from starting

- **Locations:** `backend/app/src/player/output/playout.rs:325`; `backend/app/src/graphics.rs:283` and `:291`; `backend/app/src/graphics/channel/graphics.rs:353`, `:360`, `:404`.
- **Evidence:** `player()` propagates `load_channel_ticker()` with `?` before opening the output. Every ticker-loading path subsequently calls `load_permanent_templates()`. That loop propagates specification/image/source errors and render errors from any independent permanent composition. `layer_image()` rejects a missing file at `backend/app/src/graphics/templates/graphics.rs:634`. Ordinary Storage removal (`backend/app/src/file/local.rs:367`) can delete that referenced file without altering the model. Logo and L-shape loaders otherwise handle failed overlay loads without failing the programme startup.
- **Trigger/impact:** Save a valid always-on image template, remove/rename its image, then start/restart the channel. An optional overlay error returns before output creation, preventing broadcast; hiding that permanent model at runtime does not remove it from the startup loop.
- **Recommended change:** Isolate invalid overlay load failures from channel output startup; log/disable or retain the last available overlay per model, and continue loading healthy models. Preserve fatal errors only when output itself cannot start.
- **Concrete next check:** In disposable staging, create one valid permanent image and one healthy permanent box, remove the image through Storage, restart playout, and verify output/receiver progress plus the healthy composition continue with an explicit overlay warning.

## API-A2 — MEDIUM — Waiting/in-flight takes can publish after template deletion or an all-out command

- **Locations:** `backend/app/src/graphics.rs:231`–`:255` and `:262`; `backend/app/src/api/routes/graphics.rs:255`–`:265`, `:332`–`:352`, `:375`.
- **Evidence:** `DuringLShape::Wait` spawns a detached task capturing template/spec and publishes after the picture window ends. The task has no stored handle, cancellation token, model existence/assignment check or edit-generation validation. Deletion captures only already-active take IDs and removes those. The all-out endpoint operates on the engine's current active queue only. Immediate takes likewise render and publish without the edit lock that deletion uses, so deletion can race an in-flight render.
- **Trigger/impact:** Turn on L-shape, queue a waiting take, delete its template (or request all graphics out), then turn L-shape off. Captured obsolete content can appear on air after the operator removed it. A concurrent nonwaiting take can similarly publish after deletion's active snapshot.
- **Recommended change:** Give pending takes owned identities/cancellation and serialize or validate publication against template edits/deletion; cancel relevant pending work on deletion/all-out and channel lifecycle operations.
- **Concrete next check:** API-level disposable test for wait → delete → L-shape off and wait → all-out → L-shape off; assert no take appears. Add a controlled render barrier to check take/delete interleaving without relying on timing.

## API-A3 — MEDIUM — New temporary graphics use pending output dimensions while the old output is still running

- **Location:** `backend/app/src/graphics.rs:165`–`:170`.
- **Evidence:** `draw_and_take()` reads `manager.config.output` dimensions. Configuration updates replace `manager.config` before the requested restart occurs (`backend/app/src/utils/config/update.rs:278`), while `manager.running_config()` retains the dimensions actually on air. Permanent templates, bound graphics and dynamic refresh explicitly choose running dimensions; the initial temporary/manual/scheduled take path does not.
- **Trigger/impact:** While 1280×720 output is running, save 1920×1080 settings and leave the restart pending; take a static graphic at x=50%. It is rendered at x=960 rather than x=640 against the still-running frame. Static compositions stay misplaced/cropped; dynamic compositions may jump when their next refresh uses the running dimensions.
- **Recommended change:** Resolve render dimensions from `manager.running_config()`, falling back to saved dimensions only when no output is running, consistently across all take paths.
- **Concrete next check:** Start disposable 1280×720 output, save a different output size without restarting, take static and clock graphics at known percentages, inspect decoded output coordinates, then restart and verify the newly saved geometry.

## API-A4 — MEDIUM — Controlled image indices drift when template conversion omits preceding layers

- **Locations:** `backend/app/src/player/output/playout.rs:184`–`:189` (L-shape), `:91`–`:97` (logo); repeated at `backend/app/src/graphics.rs:457`–`:464` for dynamic L-shape refresh.
- **Evidence:** The application computes `primary` from every visible template layer. `graphic_spec_at()` removes Video layers (`backend/app/src/graphics/templates/graphics.rs:705`–`:712`), no-fill rectangular boxes (`:717`–`:720`) and empty image fields (`:770`–`:773`). The engine then interprets `primary` as an index in the smaller `spec.layers` vector; L-shape `render_model_art()` changes that indexed layer (`backend/engine/src/compositor/graphics/lshape/graphics.rs:671`), or inserts full-frame fallback artwork when the index is missing.
- **Trigger/impact:** Author an L-shape ordered `[video, image, text]`, set `control_layer_id` to the image, and issue an external/clip artwork replacement. Application primary=1 but engine layers=`[image,text]`, so replacement overwrites the text and leaves the original image. Reordering layers changes external-control behavior despite stable layer IDs. A visible empty box before a logo target causes the same index mismatch.
- **Recommended change:** Carry the controlled layer's stable ID through conversion, or have conversion return the exact engine-layer index after omission; share this mapping for initial and dynamic loads.
- **Concrete next check:** Unit/API fixture with `[video,image,text]` and explicit image target, replace artwork and assert the original image slot changes while text/picture persist. Include omitted empty-box-before-logo coverage and dynamic refresh.

## Other observations and evidence limits

- The scheduler playback split appears predominantly a responsibility-preserving extraction; no separate actionable scheduler-core regression was established in this round.
- HTTP adapters use database handles and the application graphics domain rather than embedding SQL. Existing graphics integration tests cover ownership, permanent controls, deletion, font assets, data sources and a scheduled take's own end; this review makes no claim that those tests were executed or that relevant tests are absent.
- Migrations/auth/files/media engine are separately owned specialist reviews; cross-boundary evidence above is limited to the application consequences.
- Await peer digest/challenge and final deduplicated findings before any independent confirmation/readiness ballot.
