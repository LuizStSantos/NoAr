# Graphics work: handoff

Branch: `claude/branch-review-opz967`. Read this file, then
[`../graphics-roadmap.md`](../graphics-roadmap.md), `CLAUDE.md` and
`backend/AGENTS.md` before changing anything.

## Done and verified

| Commit | What |
| --- | --- |
| `9a7f2961` | `graphics_templates` table (migration `00028`), CRUD API `/api/graphics/{id}/templates`. |
| `e25f693b` | Engine draws templates (`backend/engine/src/compositor/graphics.rs`): text, box, image, rounded corners, fade / slide / wipe, duration. Blended **before** the L-shape, so a lower third follows the picture. API `take`, `out`, `on-air`; `during_lshape` = follow / wait / skip. |
| `54b605b5`, `f458f88f`, `44259da3` | Graphics tab: **On air** operator screen (preview, real program monitor, Put on air / Take out / Next, on-air list, live logo / L-shape / scene controls) and **Templates** library. Menu grouped: Graphics vs Channel settings. |

Evidence (real NoAr, HLS output, synthetic clip):

- `output-lower-third.jpg`: lower third taken through the API.
- `output-lower-third-during-lshape.jpg`: same lower third with the L-shape on;
  it shrinks with the picture.
- `ui-on-air.jpg`, `ui-templates.jpg`, `ui-on-air-phone.jpg`: the new screens.

Full suite passed at `e25f693b`: app 374 lib + 33 api_routes + 5 as_run,
engine 252. `vue-tsc`, ESLint and Prettier pass on the frontend commits.

## Composer backend (WIP base, revalidated)

The Composer backend was the last WIP commit. It has now been revalidated
with GStreamer 1.26.2 and Rust 1.99.1: the exact server Clippy command below
passes, and the full suite passes (375 app library, 33 API, 5 as-run and
253 engine tests; ignored cases remain ignored).

- `LayerKind::Video` in `backend/app/src/utils/graphics.rs`: at most one per
  template. A template with it is an **L-shape**: its other layers are the art
  around the picture, the video layer is the picture window.
- Engine: `PictureWindow` in `GraphicSpec`; `render_graphic` draws the art once
  into an `LShapeGraphic` (`LShapeGraphic::from_parts` in `lshape.rs`);
  `GraphicsControl::picture()` gives the L-shape level (0 to 1, switch time
  `LSHAPE_SWITCH_SECONDS`); one L-shape template on air at a time.
- `playout.rs`: an L-shape template on air replaces the channel's L-shape.
- `take_template`: the L-shape rule now also counts an L-shape template on
  air.
- `GET /api/graphics/{id}/fonts`: font families for text layers.
- Tests added: engine `a_picture_template_is_an_lshape_with_its_layers_as_art`,
  app `a_video_layer_makes_the_template_an_lshape`. Both passed in the full suite.

## Composer UI (implemented)

The visual editor lives in `GraphicsComposer.vue`; route `graphics-composer`
loads it directly. `GraphicsLibrary.vue` contains only the library.
`TemplatePreview.vue` draws the canvas. No migration or engine change is
needed for these UI changes.

**Composer UI (Vue)**, route `graphics-composer` (`/graphics/composer?template=<id>`,
   no id = new). Follow `composer-design.html` (open it in a browser; it is a
   static mockup). It includes:
   - Top bar: template name, Undo, Play entry, **Save template** (name, group,
     fields shown read-only), Open in On air.
   - Left: layer list (kind, name, field chips, eye to hide, x to delete with
     Undo note), **+ add layer** menu: Image, Text, Shape (box), **Video in the
     corner** (L-shape picture window, one only).
   - Centre: 16:9 canvas (reuse `TemplatePreview.vue`), drag to move, handles
     for width only, height only and both; **Proportional** checkbox (off by
     default); snapping to centre and 90% safe area; readout of x / y / w / h.
   - Right: layer name, Delete layer; position and size with a **% / px**
     toggle (px = of 1920x1080 or the channel output); Scale field and 50 / 75 /
     100 / 150 buttons; centre horizontal / vertical, full width; text content
     with `{field}` help, font (from `/fonts`), size, bold, colour, align, fill,
     radius; image: upload to storage folder `Graphics/` (reuse
     `sendFileInChunks` from `composables/chunkedUpload.ts`, endpoint
     `/api/file/{id}/upload?path=Graphics`) or pick with `StoragePicker.vue`, or
     a `{field}`; animation in / out (none, fade, slide_left, slide_right,
     slide_up, slide_down, wipe, scale), duration ms, delay ms; template
     duration and "during an L-shape" rule.
   - Timeline under the canvas: one bar per layer showing delay and animation.
   - Wire: Templates cards get **Edit**, the **+ New template** button opens
     the Composer (admins only; API refuses others), On air gets "Edit
     template". Saving uses POST / PUT; 409 = name taken.

Browser evidence on a disposable local SQLite instance covers drag and all
three resize handles, proportional geometry, percent/pixel conversion at the
channel output size, scale, centering, visibility, deletion and undo; image
upload to `Graphics/`, decoded storage preview and image selection; fields,
font and colour; one video window; animation controls and timeline scrubbing;
POST/PUT persistence and reload. Three retained Playwright regressions in
`qa/e2e/tests/operator.spec.ts` also cover admin entry points, name conflicts,
failed saves and failed loads. English, Brazilian Portuguese and a 390 px
viewport were exercised. Frontend build/type checks and ESLint pass.

The browser preview illustrates presets rather than proving on-air output.
The existing engine renders `scale` as a fade, and picture-window templates
use the L-shape transition. Images use the engine's existing PNG, JPEG, WebP
and GIF formats. Browser checks are distinct from the real HLS proof below.

## Follow-up part 1 — Composer refactor and safe upload

Completed on the branch after `6c1cab86`:

- Extracted `GraphicsComposer.vue` without changing the editor controls or
  API contract. Functions use explicit blocks and separate logical steps.
- Uploads construct the `File` using a name cleaned with the Linux server's
  `sanitize_upload_filename` rules, including control characters and UTF-8
  truncation. The saved layer uses that exact name.
- A name conflict retries as `name-2.png`, `name-3.png`, and so on; it does
  not overwrite an image another template may use. The backend still
  atomically refuses an overwrite race.
- Passed `npx vue-tsc --build`, ESLint on `frontend/src`, Prettier on the
  changed code, and all three Composer tests in `qa/e2e/tests/operator.spec.ts`.
  The upload test also checks cleaned names, different bytes at the new
  path and unchanged bytes at the original path.
- Tested against the real backend with a fresh disposable SQLite database.
  Chromium verified creation, library Edit, dragging and resizing, duplicate
  image upload, video corner, saving/reloading and a 390 px layout without
  horizontal overflow. Desktop JPEGs are 1440 px wide; the phone is 390 px.

Screenshots: [new template](composer-new.jpg), [edit](composer-edit.jpg),
[drag and resize](composer-drag-resize.jpg), [image upload](composer-upload.jpg),
[video corner](composer-video-corner.jpg), [save](composer-save.jpg),
[phone](composer-phone.jpg).

No Rust changed in this part, so the Rust suite was not rerun. The real HLS
proof and the approval-gated legacy migration are separate parts below.

## Follow-up part 2 — real template L-shape HLS proof

Completed with the existing NoAr binary, Rust 1.99.1 and GStreamer 1.26.2;
no Rust or migration changes were required. A second fresh SQLite database,
synthetic 640x360/25 fps H.264/AAC clip, local HLS output and real channel file
logging were used. No production data or destination was involved.

The L-shape template has blue full-frame box artwork and a video window at
`x=10%, y=10%, width=70%, height=70%`. A separate GC template has a white box
and text, with `during_lshape=follow`. Both were taken and removed through
`/api/graphics/1/take` and `/api/graphics/1/out`.

Frames decoded from newly completed `.ts` segments, after each transition:

| Stage | Real output capture |
| --- | --- |
| Before taking the template | [Baseline](output-template-lshape-baseline.jpg) |
| Template L-shape on air | [Blue artwork and picture window](output-template-lshape-on.jpg) |
| GC follows the picture inside the L-shape | [GC inside the corner](output-template-lshape-with-gc.jpg) |
| L-shape removed, GC still on air | [GC returns to full-frame coordinates](output-template-lshape-out-with-gc.jpg) |
| GC removed too | [Original picture restored](output-template-lshape-out.jpg) |

The decoded-pixel check locates the GC at approximately `(86,225)` in the
448x252 picture window instead of its full-frame `(32,270)` position. After
removing the L-shape it returns to `(32,270)`. The white-pixel fractions in
those respective regions were 0.8685 and 0.894; the old full-frame region had
0 white pixels while the corner was active. Visual inspection agrees.

All five `/health` samples returned `status=ok`, channel `state=on_air` and
fresh output (see per-stage `output_age`). The nonempty `noar_1.log` had zero
errors. The final server log had zero errors, with two unrelated warnings:
the disposable account has no 2FA, and the release update check cannot reach
`api.github.com`. An initial environment-only UID/name mapping issue was
corrected before this fresh final run.

[Machine-readable evidence](output-template-lshape-evidence.json) includes
health samples, take state, source segment SHA-256 hashes, pixel measurements
and the channel log captured for this run. These five frames verify the
requested HLS transitions; they do not establish a long soak, A/V sync,
hardware outputs or every animation preset.

## Follow-up part 3 — plan awaiting approval

No migration, always-on implementation or legacy menu removal has started.
The proposed work must preserve logo/ticker composition in full-frame space,
ordinary `follow` graphics inside the picture, existing scene actions and
scheduled references, and the legacy logo/L-shape/scene API contracts.
Only a new migration may be added after explicit user approval. Logo,
L-shape and Quick scenes remain in the menu until that migration and its
compatibility tests are complete.

## What is left, in order

1. **Logo and L-shape as templates**: an "always on" flag on templates (logo,
   clock, ticker) drawn on every frame, and a migration that turns each
   channel's logo, L-shape graphic and quick scenes into templates without
   changing what is on air. Keep the old `/api/control/{id}/logo|lshape|scene`
   routes working.
2. **Remove** Logo, L-shape and Quick scenes from the Graphics menu once 1 is
   in.
3. Later phases (see roadmap): rundowns 2.5 (`on-air-design.html`), graphics
   inside clips with open tracks 2.6 (`video-graphics-design.html`), control API
   with key and webhooks 2.7 (`control-api-design.html`).

## How to build and check

- The host may have GStreamer < 1.26; build inside a container that has
  1.26 (the project image or `docker/gstreamer`).
- `cargo fmt --all`, `cargo clippy --no-default-features --all-targets -- --deny warnings`,
  `cargo test --no-default-features`.
- Frontend from the repo root: `npx vue-tsc --build`,
  `npx eslint --config frontend/eslint.config.ts frontend/src/...`,
  `npx prettier --write frontend/src/...`.
- UI: run the backend on `127.0.0.1:8787` with a throwaway `DB=...` and
  `npx vite`, then check in a browser (pt-BR and en-US texts both live in
  `frontend/src/locales/`).
- Never claim UI or on-air output works without having run it.

## Rules the user set

- Repository text (code, commits, docs) in English; talk to the user in
  Portuguese.
- New backend graphics files are called `graphics.rs` in their folder.
- Design first, the user approves, then code; show screenshots of each
  finished part.
- No push to `main`, no merge or release without the user asking.
