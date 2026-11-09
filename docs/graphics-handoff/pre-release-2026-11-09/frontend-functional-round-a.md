# Frontend functional review — round A

Reviewer: `noar-functional-reviewer`, exactly one frontend council voter. Read-only source review of `/workspace/NoAr`, HEAD `f925f608`, compared with `origin/main`, including the current label/backend worktree edits observed on 2026-11-09. No product edits, builds, servers, migrations, Git mutations, or browser clicks performed. Report persistence is the only write.

Read `AGENTS.md`, `CLAUDE.md`, `.claude/agents/noar-functional-reviewer.md`, `.claude/audit/QUALITY_GATES.md`, `.claude/audit/INTEGRATION_QUALITY_GATES.md`, and the repository map. Map is navigation only; findings below are checked against current source. The cloud runtime skill/status was read; runtime is connected, but no application/browser staging backend was exercised by this reviewer. All click behavior and output effects remain **NOT_VERIFIED**. No phase-three ballot is cast before peer challenge.

## Findings

### FF-01 — HIGH — A waiting take has no usable cancellation control when temporary on-air is empty

Evidence: **VERIFIED_STATIC**; browser/output reproduction **NOT_VERIFIED**.

- `frontend/src/components/graphics/GraphicsOnAir.vue:181` handles `state === 'waiting'` only by a transient success toast. It stores no pending state or cancellation affordance.
- `frontend/src/components/graphics/GraphicsOnAir.vue:395` disables the main OUT control when the ordinary selected template has no `latest` temporary instance. `frontend/src/components/graphics/GraphicsOnAir.vue:416` disables ALL OUT when no non-leaving temporary instance appears in `onAir`.
- `backend/app/src/graphics.rs:319` queues a wait-mode graphic while L-shape is active, returns `Waiting` at line 356, and later publishes it after L-shape ends. Pending takes are absent from the active `graphics_on_air` response (`backend/app/src/graphics.rs:179`).
- The existing cancellation operation is `POST /out` without `take_id`: `backend/app/src/api/routes/graphics.rs:370` calls `cancel_pending()` before removing active takes.

Impact: With L-shape active and no temporary graphic active, an operator can queue a wait-mode model but cannot retract it using either visible OUT control. Ending L-shape can subsequently put the unwanted model on air. Repeated TAKE also queues more pending instances without a persistent visible queue.

Next proof: In disposable staging, enable L-shape; select an ordinary template configured `during_lshape=wait`; TAKE; assert waiting response and empty on-air list; assert OUT/ALL OUT disabled. End L-shape and verify the queued take appears. Then exercise an enabled cancellation control and assert the pending take never publishes. Provide persistent pending status and a usable all-out/cancel action, or expose pending takes in status.

### FF-02 — MEDIUM — Library temporary ON AIR badges never refresh after initial load

Evidence: **VERIFIED_STATIC**; browser reproduction **NOT_VERIFIED**.

- `frontend/src/components/graphics/GraphicsLibrary.vue:151` initializes `onAir` during `loadLibrary()`.
- Its two-second poll (`frontend/src/components/graphics/GraphicsLibrary.vue:165`, registered at line 202) requests only `graphicsApi.channel()` and updates assigned channel models at line 176. It never refreshes temporary `onAir`.
- The library's ordinary-template ON AIR badge reads that static array at `frontend/src/components/graphics/GraphicsLibrary.vue:301`.

Impact: Open Templates while a short temporary graphic is active and its badge remains ON AIR after expiry. Conversely, a later scheduled/API take gets no badge. Operators receive stale transmission state until navigation/reload (or another complete library load).

Next proof: In staging, take a short-lived ordinary model, open Templates, wait beyond its duration plus two polls, and compare the badge with `/on-air`; then trigger a safe new take via another test client while Templates stays open. Poll the temporary status together with assigned status and protect both with the existing request-version/channel checks.

## Compact control matrix

`VerifiedStatic` means handler/route/API wiring was inspected, not that the action passed. Every row's actual click, persistence, or transmission behavior is **NOT_VERIFIED**.

| View | Controls/journey | Expected action | Static evidence | Outcome / runtime status |
| --- | --- | --- | --- | --- |
| Graphics navigation | On air, Templates, Sources, XMLTV, Subtitles, API; mobile page selector | Reach scoped child route, preserve channel | `frontend/src/views/GraphicsView.vue:17`, `:73`; `frontend/src/router/index.ts:68` | VerifiedStatic / NOT_VERIFIED |
| Graphics navigation | Channel selector, dirty-settings discard/cancel | Preserve or explicitly discard settings before switching channel | `frontend/src/views/GraphicsView.vue:81`; child `RouterView` keyed by channel at `:175` | VerifiedStatic / NOT_VERIFIED |
| Legacy graphics links | live, logo/ticker hash, lshape, scenes | Redirect to consolidated operator/editor routes | `frontend/src/router/index.ts:86` onward | VerifiedStatic / NOT_VERIFIED |
| Composer | Load/new model; invalid/missing template | Load requested model or report failure without editable stale document | `frontend/src/components/graphics/GraphicsComposer.vue:281` | VerifiedStatic / NOT_VERIFIED |
| Composer | Add/remove layer, edit properties, undo, geometry/selection | Update draft with history | `frontend/src/components/graphics/GraphicsComposer.vue:141`, `:158`, `:178`, `:225`, `:253` | VerifiedStatic / NOT_VERIFIED; peer keyboard/resize findings separate |
| Composer | Play/stop, scrub, return to edit | Preview authored timing, cancel animation frame on disposal | `frontend/src/composables/graphicsComposerPreview.ts:22`; `frontend/src/components/graphics/composer/ComposerTimeline.vue:26` | VerifiedStatic / NOT_VERIFIED |
| Composer | Save template plus assignment; cancel navigation/discard | Persist document then assignment and preserve draft on declined navigation | `frontend/src/components/graphics/GraphicsComposer.vue:339`, `:591` | VerifiedStatic / NOT_VERIFIED; peer save/channel/draft-loss findings separate |
| Composer | Font upload; image upload; field/source/column/record mapping | Upload safe asset and bind layer or field | `frontend/src/components/graphics/GraphicsComposer.vue:386`, `:422`, `:113`; `frontend/src/components/graphics/composer/ComposerFieldMappings.vue:16` | VerifiedStatic / NOT_VERIFIED |
| Templates | Group filters, new/edit links, To air link | Filter model cards or open requested model/editor/preview | `frontend/src/components/graphics/GraphicsLibrary.vue:36`, `:119`, `:381` | VerifiedStatic / NOT_VERIFIED |
| Templates | Delete usage dialog; confirm/cancel | Show usage and remove only confirmed model | `frontend/src/components/graphics/GraphicsLibrary.vue:59`, `:81`, `:93` | VerifiedStatic / NOT_VERIFIED |
| Templates | Assigned switch, L-shape control, apply scene | Change assigned model through channel control | `frontend/src/components/graphics/GraphicsLibrary.vue:183`, `:351` | VerifiedStatic / NOT_VERIFIED |
| Templates | Temporary ON AIR badge | Follow active temporary instances | `frontend/src/components/graphics/GraphicsLibrary.vue:165`, `:301` | FF-02 / NOT_VERIFIED |
| On air | Select model, Next, manual values, source record, duration | Fill preview and submit selected model data | `frontend/src/components/graphics/GraphicsOnAir.vue:112`, `:155`, `:217`, `:330` | VerifiedStatic / NOT_VERIFIED |
| On air | TAKE, per-instance OUT, main OUT, ALL OUT | Publish/remove intended temporary or assigned graphics | `frontend/src/components/graphics/GraphicsOnAir.vue:155`, `:193`, `:207` | FF-01 pending cancellation gap / NOT_VERIFIED |
| On air | Permanent model buttons, live monitor/status | Control assigned state and distinguish real output from preview | `frontend/src/components/graphics/GraphicsOnAir.vue:280`, `:445`; `frontend/src/composables/channelGraphics.ts:23` | VerifiedStatic / NOT_VERIFIED |
| Sources | New/edit, CSV file/paste, URL/kind/path/interval, save | Save source with bounded file size and revision | `frontend/src/components/graphics/GraphicsSources.vue:32`, `:48`, `:64` | VerifiedStatic / NOT_VERIFIED |
| Sources | Refresh now, delete confirmation/cancel, preview rows/errors | Refresh or remove intended test source and show last-good data | `frontend/src/components/graphics/GraphicsSources.vue:89`, `:102`; `frontend/src/composables/graphicsSources.ts:68` | VerifiedStatic / NOT_VERIFIED |
| Scheduler graphics | New/edit, template/legacy actions, fields/records, date/time/repeat, save/cancel | Persist compatible event payload and refresh schedule | `frontend/src/components/schedule/GraphicsEventDialog.vue:83`, `:154`; `frontend/src/views/ScheduleView.vue:312` | VerifiedStatic / NOT_VERIFIED; execution and DB persistence NOT_RUN |
| Scheduler graphics | List edit/delete; missing-template option | Open retained event or remove confirmed test event | `frontend/src/components/schedule/ScheduleGraphicsList.vue:51`; `frontend/src/views/ScheduleView.vue:329`; `frontend/src/components/schedule/GraphicsEventDialog.vue:280` | VerifiedStatic / NOT_VERIFIED |
| Storage | Rename/create/delete/move confirmation/cancel; download | Apply intended file mutation or download requested asset | `frontend/src/composables/storageFileActions.ts:77`, `:104`, `:139`, `:172`, `:182`; `frontend/src/components/storage/StorageFileDialogs.vue:33` | VerifiedStatic / NOT_VERIFIED; disposable files required |
| Storage images | Thumbnails, retry, image viewer selection | Fetch short-lived image URLs and open selected image | `frontend/src/composables/storageImages.ts:39`, `:73`, `:114` | VerifiedStatic / NOT_VERIFIED |
| Player | Graphics status and Open graphics link | Display channel state and navigate to same-channel On air | `frontend/src/components/player/GraphicsQuickBar.vue:14`, `:36` | VerifiedStatic / NOT_VERIFIED |
| Player playlist | Save/copy/import/discard/fill/delete | Keep extracted day actions and confirmation wiring | `frontend/src/composables/playlistDayActions.ts:32`, `:113`, `:121`, `:146`, `:178` | VerifiedStatic / NOT_VERIFIED |

## Boundaries and handoff

No duplicate finding is raised for peers' Composer channel/save races, draft loss, keyboard mutation while busy, keyboard resize, or clipped handles/filter styles. Existing main already contains the extracted Storage channel/folder mutable-operation and image-cache behavior, and the zero-duration fill loop; these are not presented as newly introduced regressions. All actual storage destructive testing must use disposable data.

Focused staging priorities: FF-01 queued take/cancel/output; FF-02 time-dependent badges; Composer round-trip save and failure/retry; uploaded font/image preview after reopen; source row selection through On air and scheduler; schedule UI/API/DB persistence; Player same-channel link; disposable storage rename/move/delete/download. Parent owns staging evidence and checks after patches. Functional runtime gate is NOT_RUN, readiness is not established. Await peer challenge before any ballot.
