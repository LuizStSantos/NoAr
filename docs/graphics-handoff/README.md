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

## Follow-up part 3 — approved stages a–e

The approved implementation uses a normal sqlx migration `00029_...`.
There is no separate migration registry. Conversion runs automatically after
`MIGRATOR.run` for every channel. Once conversion is installed, the engine
must read models/bindings only; the old columns remain unchanged, without
per-field fallback. Conversion and redirecting the old settings pages to
models/bindings must land together so saving a logo still changes the output.

Rollback means restoring the pre-migration copy made by
`backup_before_migrations` **and** the previous image. It does not mean opening
the upgraded schema with the previous binary. Stage b must demonstrate that
restore using a disposable installation of the exact 2.2.1 binary and document
the operator procedure in `docs/docker.md` or `docs/releasing.md`.

The agreed stages, each with its own commit, tests, captures and handoff:

- **a — completed:** permanent logo/ticker infrastructure in engine and DB
  handles; schema exists only in test fixtures. No conversion and the current
  application's logo path stays unchanged. Evidence is recorded below.
- **b — completed:** normal 00029 migration, automatic all-channel conversion,
  redirected legacy page writes, real HLS, playout/backend restarts and rollback
  using the automatic backup with the exact released 2.2.1 image. Evidence below.
- **c — completed:** compatibility coverage for clip logo/L-shape
  marks, graphics events and scene reversal, keyed and authenticated routes,
  external replacement/placement and transitions, desktop output and XMLTV.
- **d — completed:** aligned before/after HLS frame comparisons in logo and
  L-shape regions, including transitions, health and channel logs.
- **e — completed:** after migration and compatibility validation, remove
  Logo, L-shape and Quick scenes from the menu.

## Stage c — shared compatibility state and output proof

The legacy authenticated and keyed controls, quick scenes, scheduled action
snapshots and clip marks use the converted model handles. The comparison with
the official 2.2.1 binary passed **76 exact HTTP status/body cases**, including
replacement/placement state, malformed requests and authentication. Both XMLTV
cases passed: an explicit guide icon and the canonical-logo fallback, isolated
from transient on-air replacements.

Fixed desktop output's stale startup-logo dependency: it now composites the
active model/clip/external logo into the frame, with no second static plane.
Artwork replacements and image-only clip overrides preserve the model's free
picture window; explicit legacy corner/scale and existing SDK methods retain
their geometry. Each new playlist clip reads current model settings while
keeping the dimensions of the running output. Channel L-shape controls return
409 if configured artwork cannot actually be loaded.

Verification: `cargo fmt --all`, the required no-default/all-target Clippy
command and full no-default test suite passed with GStreamer 1.26.2 and Rust
1.99.1: **681 passed, zero failed, 14 ignored**. The desktop-cpu build and its
all-target Clippy check passed; **33 desktop tests passed**. Actual desktop
window captures verified show/hide, replacement, placement and restoration.
Frontend source did not change in this stage; pt-BR, phone and four browser
regressions were verified in the preceding library fix (`aa720c87`).

Eleven frames from fresh completed HLS `.ts` segments passed decoded-pixel
checks and healthy on-air samples. The logo's single-blend opacity was checked;
clip image/position and external priority work, marked gaps suppress the logo,
clip L-shape geometry works, and clearing marks restores the canonical model.
The actual future scheduled event used its saved `GraphicsScene` actions even
after editing the named scene, then reversed at its end. Frozen legacy logo,
L-shape and scene columns stayed byte-for-byte unchanged during the runner.

| Proof | Capture |
| --- | --- |
| Canonical logo, one blend | [Channel logo](output-compat-c-canonical-logo.jpg) |
| Clip image and position | [Clip logo](output-compat-c-clip-logo.jpg) |
| External image/move takes priority | [External logo](output-compat-c-external-logo.jpg) |
| Restore clip image/position | [Clip restored](output-compat-c-clip-logo-restored.jpg) |
| Marked logo gap | [Logo hidden](output-compat-c-clip-logo-hidden.jpg) |
| Clip L-shape at bottom-left/60% | [Clip artwork/window](output-compat-c-clip-lshape.jpg) |
| Marks removed | [Channel restored](output-compat-c-clip-cleared.jpg) |
| External art preserves free window | [Replacement art](output-compat-c-free-window-replacement.jpg) |
| Clip art inherits free window | [Clip art](output-compat-c-free-window-clip.jpg) |
| Scheduled action snapshot on air | [Event active](output-compat-c-scheduled-scene.jpg) |
| Scheduled action reverses at end | [Event ended](output-compat-c-scheduled-reverted.jpg) |
| Desktop before this fix | [Missing logo](output-compat-c-desktop-before.jpg) |
| Desktop fixed, configured logo | [Single logo](output-compat-c-desktop-canonical.jpg) |
| Desktop hide | [Hidden](output-compat-c-desktop-hidden.jpg) |
| Desktop replacement and move | [Replacement](output-compat-c-desktop-replacement.jpg) |
| Desktop restore | [Restored](output-compat-c-desktop-restored.jpg) |

[Complete matrix](compat-c-matrix.md), [HTTP evidence](compat-c-api-evidence.json),
[HLS hashes/pixels/health](compat-c-output-evidence.json),
[desktop evidence and limitations](compat-c-desktop-evidence.json), and
[retained staging runners](../../qa/graphics/README.md).

A final backend restart with file logging enabled returned healthy on-air
output. The nonempty `noar_1.log` contained zero errors; the fresh HLS frame
is [here](output-compat-c-restarted.jpg), with [restart/log evidence](compat-c-restart-evidence.json).

The desktop virtual PulseAudio/ALSA device later logged underrun/overrun
warnings, an I/O error and a clip-boundary sync error; the visual checks do
not establish desktop audio stability. Desktop GPU, physical outputs,
recording, window resizing, A/V sync and a long soak were not tested. Logo
cut/fade/crossfade transitions passed engine regressions; aligned integration
comparison of transition frames was deferred to **stage d**, now recorded
below. **Stage e** menu removal was not part of stage c. Clock remains excluded.

## Desktop baseline check before stage d

Repeated the same synthetic desktop scenario on `main` at `dd4529e2` and this
branch at `ee2ad7cb`: 240 seconds each, the same 180-second 640×360/25 fps
gray clip with 440 Hz AAC, initial fake time/source seek, desktop-cpu renderer,
Xvfb `:99` at 1280×720×24, fresh PulseAudio daemon with the same clocked null
sink (48 kHz, stereo), ALSA configuration and logo commands at the same elapsed
times. No compilation ran during either measurement. Backend/Cargo source on
that main SHA matches released 2.2.1 revision `d4a1a0a8`; documentation and QA
trees differ. Both runs reported healthy output during sampling.

| Observed in 240 seconds | Main | Branch |
| --- | --- | --- |
| Buffer underrun/overrun messages | 17 | 7 |
| ALSA `snd_pcm_avail_delay` I/O errors | 15 | 6 |
| Clip-boundary sync offsets (seconds) | -20.870, -17.601 | -15.973 |

The same failure classes occur without the graphics changes. The earlier
`-11.942 s` is not a deterministic expected number: offsets and warning counts
vary with the virtual audio clock. This paired run does not show an increase
on the branch. Record this as a pre-existing limitation of the virtual desktop
environment, not proof of desktop audio stability or a newly introduced
graphics regression. Physical audio/GPU/long-soak behavior remains untested.

[Paired evidence, binary/media hashes and health samples](desktop-reference-d-evidence.json),
[main application log](desktop-main-d-server.txt),
[branch application log](desktop-branch-d-server.txt),
[main PulseAudio log](desktop-main-d-pulse.txt),
[branch PulseAudio log](desktop-branch-d-pulse.txt).
The retained runner is [`qa/graphics/desktop.py`](../../qa/graphics/desktop.py).

## Stage d — exact pre-encode and aligned HLS parity

Passed the [criteria fixed before evaluation](../../qa/graphics/parity-criteria.md)
using the actual main and branch applications. Two byte-identical copies of
the same disposable 2.2.1/00027 database and the same deterministic 40-second
640×360/25 fps source were used. Main stayed at 00027; branch startup applied
00028/00029 and converted the channel automatically. Folder mode starts both
at source PTS zero; no wall-clock API scheduling or best-fit image/time shifting
was used. The source grid and visible frame-number marker expose movement,
scaling and missing frames.

The identical [capture-only patch](../../qa/graphics/preencode.patch) was applied
only to isolated test worktrees, not the shipped application. It writes the
accepted compositor frame immediately before encoder packing, after overlays,
PTS assignment, duplicate filtering and pacing. Captures are tightly packed
YUV420P; stride padding is excluded. No pixel, PTS or transition clock is changed.
When switching source worktrees with a shared Cargo target, the application
and engine packages were explicitly cleaned/rebuilt to avoid stale artifacts;
the evidence records different binary hashes and the capture-patch hash.

**All 1000 frames at PTS 0–999 matched byte for byte before encoding.** All
1000 decoded HLS frames matched on the exact 40 ms PTS grid after removing the
constant MPEG-TS origin. Source frame IDs 0 and 999 were verified. No test frame
was dropped, spatially aligned, rescaled or shifted in time. In the logo,
whole-frame L-shape and ticker regions, **HLS RGB error was zero on every
frame**, including fades and picture motion. The fixed lossy-codec allowance
(MAE ≤0.75, p99.9 ≤8, max ≤32) was not needed or changed.

The logo gap is 8–10 seconds, with its configured one-second fade during
7–8 and 10–11 seconds. The L-shape mark is 14–20 seconds, with complete
1.2-second entry and exit phases (logo suppression plus picture motion).
The reference application has no persisted ticker after migration 00008;
the ticker region stayed unchanged, and conversion created an empty, disabled
binding. This proves absence is preserved, not parity for newly authored text.

| Frame / time | Main | Branch | Difference ×8 |
| --- | --- | --- | --- |
| 0 / 0 s, configured logo | [Before](output-parity-d-0000-main.jpg) | [After](output-parity-d-0000-branch.jpg) | [Diff](output-parity-d-0000-diff.png) |
| 187 / 7.48 s, logo fading out | [Before](output-parity-d-0187-main.jpg) | [After](output-parity-d-0187-branch.jpg) | [Diff](output-parity-d-0187-diff.png) |
| 262 / 10.48 s, logo fading in | [Before](output-parity-d-0262-main.jpg) | [After](output-parity-d-0262-branch.jpg) | [Diff](output-parity-d-0262-diff.png) |
| 370 / 14.8 s, picture shrinking | [Before](output-parity-d-0370-main.jpg) | [After](output-parity-d-0370-branch.jpg) | [Diff](output-parity-d-0370-diff.png) |
| 420 / 16.8 s, settled L-shape | [Before](output-parity-d-0420-main.jpg) | [After](output-parity-d-0420-branch.jpg) | [Diff](output-parity-d-0420-diff.png) |
| 480 / 19.2 s, picture growing | [Before](output-parity-d-0480-main.jpg) | [After](output-parity-d-0480-branch.jpg) | [Diff](output-parity-d-0480-diff.png) |
| 495 / 19.8 s, logo returning | [Before](output-parity-d-0495-main.jpg) | [After](output-parity-d-0495-branch.jpg) | [Diff](output-parity-d-0495-diff.png) |
| 999 / 39.96 s, final source frame | [Before](output-parity-d-0999-main.jpg) | [After](output-parity-d-0999-branch.jpg) | [Diff](output-parity-d-0999-diff.png) |

[All per-frame metrics, hashes, PTS and health samples](parity-d-evidence.json),
[all common exact pre-encode frames](parity-d-common-preencode.tar.gz),
[both actual HLS segment sets](parity-d-hls-segments.tar.gz),
[main channel log](parity-d-main-channel.txt),
[branch channel log](parity-d-branch-channel.txt),
[main server log](parity-d-main-server.txt) and
[branch server log](parity-d-branch-server.txt).
Both channels stayed healthy and their nonempty channel logs had zero errors.
Representative captures include 23 matched frame pairs and their differences;
the complete comparison covers every frame, not just these examples.

The retained runner is [`qa/graphics/parity.py`](../../qa/graphics/parity.py).
This establishes migration parity for this deterministic HLS scenario. It does
not establish desktop audio stability, hardware outputs or a long soak.
Stage e menu/editor changes may now proceed; none were included in this stage.

## Stage e — model editors and navigation cleanup

After stage d passed (`2670a2ba`), removed Logo, L-shape and Quick scenes from
both the desktop Graphics navigation and the phone selector. The four entries
are now On air, Templates, Subtitles and External control (API).

Assigned-model **Edit** links open `GraphicsChannelEditor.vue` at
`/graphics/channel/{logo|ticker|lshape|scene}`. It gives the operator a named
editor and **Back to templates**, reusing the native controls and existing save
contracts. Logo/L-shape settings write converted models, ticker edits its own
model, and scenes edit their bound action definitions. Ordinary templates
still open the Composer. The Scenes library group remains visible when empty,
with **Manage scenes** so creating the first scene does not require an old menu.

Legacy UI links `/graphics/logo`, `/graphics/lshape` and `/graphics/scenes`
redirect to the dedicated editor, preserving `channel`. The old logo `#ticker`
link opens the ticker editor. Legacy keyed/authenticated **API endpoints have
not been removed or redirected**: the 76 exact HTTP comparisons and both XMLTV
checks passed again against 2.2.1. The first-scene browser regression also
applies the newly saved scene through `/api/control/1/scene`.

Real Chromium checks in pt-BR at 1440×1000 and 390×844 verified all four
editors, image loading, saves and reloads, canonical model image layers,
bookmark redirects, return links, menu removal, no horizontal overflow and no
JavaScript page errors. Vue type/build checks, ESLint, changed-file Prettier
and QA TypeScript checks passed. Six retained graphics browser tests passed:
the three Composer regressions, assigned controls/editors, navigation/aliases
and first-scene creation in an empty group.

| Part | Desktop | Phone |
| --- | --- | --- |
| Templates and cleaned menu | [Templates](editor-e-templates-pt-BR.jpg) | [Templates](editor-e-phone-templates-pt-BR.jpg) |
| Logo editor | [Logo](editor-e-logo-pt-BR.jpg) | [Logo](editor-e-phone-logo-pt-BR.jpg) |
| L-shape editor | [L-shape](editor-e-lshape-pt-BR.jpg) | [L-shape](editor-e-phone-lshape-pt-BR.jpg) |
| Ticker editor | [Ticker](editor-e-ticker-pt-BR.jpg) | [Ticker](editor-e-phone-ticker-pt-BR.jpg) |
| Scene editor | [Scenes](editor-e-scenes-pt-BR.jpg) | [Scenes](editor-e-phone-scene-pt-BR.jpg) |

The phone also created and saved its first scene from the
[empty Scenes group](editor-e-phone-empty-scenes-pt-BR.jpg), then applied it
through the legacy scene API: [saved phone scene](editor-e-phone-first-scene-pt-BR.jpg).
The pt-BR checks were repeated with the final built backend after the test-clock
fix, not just with the earlier stage-c binary.

[Browser evidence](editor-e-browser-evidence.json) and
[verification results](editor-e-checks.json) record the executed checks.
The final no-default Rust suite passed 681 tests, zero failed and 14 ignored;
fmt and the exact all-target Clippy command passed with GStreamer 1.26.2.
One initial run exposed an unrelated cleanup-test race: another test's worker
deleted a deliberately old temporary file during its assertion. Commit
`b2c16242` gives that scan an explicit local clock, keeping fixture files fresh
in real time. The isolated case and complete suite then passed. This is separate
from the frontend editor delivery and changes no graphics composition.

Desktop audio remains limited as recorded in the paired baseline check above.
No physical outputs, GPU, A/V sync or long soak was added to this verification.
Clock support remains outside this delivery. No merge, release or main push
was performed.

Only logo and ticker are always on in this delivery. Clock support is out
of scope. L-shape remains a manual picture-window graphic. Full-frame logo
and ticker composition must remain distinct from picture-follow ordinary GCs.

## Stage a — permanent-role infrastructure, without migration

This section describes commit `65c6892f`, before the stage-b migration below.

Implemented stable `AlwaysOnSlot::Logo` and `AlwaysOnSlot::Ticker` roles in
`GraphicsControl`. They have no duration, do not consume the temporary take
queue and survive `take_all_out`, expiry and queue eviction. They can be
hidden/restored explicitly without discarding the definition. Picture-window
models are rejected as permanent roles. No clock role was added.

Permanent roles blend in full-frame space after the L-shape. Clip-logo gaps
and L-shape logo suppression affect the logo role only, not the ticker. The
normal application does **not** load these roles yet; its configured logo,
clip marks, controls and pages remain on their unchanged path in this stage.
The HLS runner deliberately shows the configured legacy logo at top-right
and test permanent models elsewhere to make their independence visible.

Added DB models and scoped handles for `graphics_channel_bindings`. The table
is created **only inside test fixtures**, not on application startup. There
is no 00029 migration or production DDL in this commit. Tests cover persisted
bindings after closing/reopening SQLite, repeated saves without duplication,
channel isolation, bound-template deletion protection, unchanged legacy logo
values, and unchanged startup with no bindings table or data conversion.

Verification with GStreamer 1.26.2 and Rust 1.99.1:

- `cargo fmt --all` — passed.
- `cargo clippy --no-default-features --all-targets -- --deny warnings` — passed.
- `cargo test --no-default-features` — 674 passed, 0 failed, 14 ignored.
- `backend/engine/examples/graphics.rs` — six 8-second stages encoded as real
  HLS using the same engine. Each source must return `ClipResult::Played`;
  fallback fails the runner. The captured legacy-logo ROI is byte-identical
  in decoded RGB in all six stages. The GC disappears after take-all-out;
  permanent models remain. Explicit logo hide leaves the ticker visible,
  and show restores the logo.

Captures from actual HLS segments:

| Stage | Capture |
| --- | --- |
| Configured legacy logo alone | [Legacy path unchanged](always-on-a-legacy-only.jpg) |
| Permanent logo and text enabled | [Permanent roles](always-on-a-permanent.jpg) |
| Temporary GC alongside permanent roles | [Temporary GC](always-on-a-temporary-gc.jpg) |
| Take all temporary graphics out | [Permanent roles remain](always-on-a-after-all-out.jpg) |
| Explicitly hide the permanent logo | [Ticker stays visible](always-on-a-hidden-logo.jpg) |
| Restore the permanent logo | [Logo restored](always-on-a-restored-logo.jpg) |

[Evidence and pixel measurements](always-on-a-evidence.json) include source
segment hashes and check results. Reproduce the isolated engine run with:

```sh
cargo run --no-default-features -p noar-engine --example graphics -- \
  /path/to/synthetic.mp4 /tmp/noar-permanent-proof /path/to/test-logo.png
```

This is an engine runner, not application/API integration of the bindings.
Its ticker sample is static text; scrolling compatibility is not established
by these captures. Native GStreamer warnings were observed for RGB matrix
metadata, splitmux keyframe requests and audio metadata at finish; all stages
played and the segments decoded successfully. This is not a clean-log soak
or proof of A/V sync. Desktop, full clip/scene/keyed-route compatibility,
XMLTV, automatic conversion and backup/image rollback remain for stages b–d.
No menu was removed.

## Stage b — automatic conversion and compatibility-page writes

Migration `00029_graphics_channel_bindings.sql` adds `graphics_channel_bindings`,
`graphics_scene_bindings` and template `settings`. Tests install the real
migration; the handwritten stage-a fixture schema is gone. Released migrations
`00001`–`00027` match tag `v2.2.1` byte for byte; `00028` is unchanged as well.
There is no separate migration registry or opt-in switch.

Immediately after `MIGRATOR.run`, startup converts all configured channels in
one transaction before initializing their managers. Three complete role
bindings mark a converted channel: startup skips it before reading any frozen
column. Incomplete bindings or invalid legacy scene JSON abort the conversion
transaction. Existing library-name collisions receive numeric suffixes;
compatibility scene names remain in their action payload. New channel creation
installs its models in the configuration transaction too. Conversion does not
issue take/out, logo hide/show or L-shape on/off commands.

The source of truth after conversion is:

| Item | Persisted source | Runtime |
| --- | --- | --- |
| Logo | `graphics_channel_bindings` (`logo`, enabled), bound template image/opacity in `layers`, native scale/position/transitions in `settings` | `GraphicsControl.channel_logo()` owns the native compositor handle; compatibility controls share that exact handle. Application output no longer sets `OutputConfig.logo`, so there is no configured-legacy fallback plane. |
| L-shape | Bound `lshape` template: art layers and complete video-window geometry | The template renderer installs the art/window in `LShapeControl` without changing its phase, replacement or placement. The window is not snapped to a corner. |
| Quick scenes | Ordered `graphics_scene_bindings` and each bound template's `settings.scene` | Existing scene controls receive the projected action DTO. Scheduled event snapshots remain unchanged. |
| Ticker | `ticker` binding and its template layers | Loaded into the permanent ticker slot when enabled. Released 2.2.1 has no persisted application ticker configuration: conversion creates an empty, disabled model. It does not import or claim scrolling text support. |

`Processing` fields in the compatibility API are projections of these models.
The Logo, L-shape and Quick scenes pages continue using their existing endpoint,
but that endpoint writes templates/bindings in its settings transaction. It
never writes the frozen logo/L-shape/scene columns. Unchanged native settings
leave model layers untouched, including freely placed picture windows; an image
save selects the visible image layer rather than assuming index zero. Editing
bound templates reloads the runtime model. Bound-template deletion returns
HTTP 409 rather than a database error; quick scenes can be removed through the
old scene page. Assigned models are operated through channel graphics controls;
temporary `/take` returns 409 for them to prevent a second logo or ignored scene
actions. Unbound Composer templates retain their take/out behavior. Native clip controls, transient replacement/placement and
XMLTV's separate guide-icon setting retain their existing interfaces.

Rollback requires the automatic pre-update backup plus the previous image,
not an old binary opening the migrated database. The concrete operator steps
are in [Docker rollback](../docker.md#restoring-before-graphics-conversion-00029).

Validation used GStreamer 1.26.2 and an actual disposable installation of
`ghcr.io/luizstsantos/noar:2.2.1` (revision
`d4a1a0a8a41f39b90de23844910998431fd42510`, pinned image digest in the evidence).
Its database at migration 00027 was copied, upgraded automatically through
00029, and the automatic pre-update backup was restored into a separate database.
The exact released image opened that restored file, authenticated the account,
read the original logo and scene, and generated healthy HLS. The restored
schema remained at 00027 with no graphics binding table.

- `cargo fmt --all`, the exact no-default/all-target Clippy command and
  `cargo test --no-default-features` passed: 679 passed, zero failed and 14
  ignored. Full results are in the [evidence JSON](migration-b-evidence.json).
- Three retained Composer Playwright tests passed in Chromium against the
  running backend. Actual clicks in the old Logo page saved `new-logo.png`,
  reloaded it and showed the new image in emitted HLS.
- DB tests cover all-channel rollback, name collisions, real-migration schema,
  idempotence, edited models, frozen-column isolation, reordered image layers
  and native even-pixel rounding at fractional L-shape scales.
- The running app preserved a freely positioned 10/10/70/50 video window through
  an old-page save and playout restart. A full backend restart preserved model
  binding IDs and the edited logo without reimporting old columns. Frozen
  `config_processing` rows stayed exactly equal as complete rows.
- The 60%-opacity logo's stable-region RGB median matches before/after migration
  and after rollback, detecting an accidental duplicate blend. The L-shape art
  region also matches. These are settled samples, not aligned frame-by-frame
  comparisons. Two buffered transition samples were discarded and recaptured.
- Every retained HLS stage reported `/health` = `ok`, channel `on_air`.
  The captured channel console logs contain zero ERROR entries. The updater
  emitted network-access warnings while checking GitHub; this is not a soak test.

| Evidence | Capture |
| --- | --- |
| Logo on released 2.2.1 | [Before migration](output-migration-b-before-logo.jpg) |
| Logo after conversion (60% opacity, one blend) | [After migration](output-migration-b-after-logo.jpg) |
| L-shape on released 2.2.1 | [Before migration](output-migration-b-before-lshape.jpg) |
| Bound L-shape model after conversion | [After migration](output-migration-b-after-lshape.jpg) |
| Edited free picture window | [Template window](output-migration-b-free-lshape.jpg) |
| Old page save preserves that window | [After old-page save](output-migration-b-after-old-page-lshape.jpg) |
| New image saved through the old Logo page | [Real output](output-migration-b-old-page-logo.jpg) |
| Playout restart preserves the model | [Restarted L-shape](output-migration-b-playout-restart-lshape.jpg) |
| Backend restart preserves the new logo | [Restarted logo](output-migration-b-backend-restart-logo.jpg) |
| Restored backup opened by official 2.2.1 | [Rollback output](output-migration-b-rollback-logo.jpg) |
| Legacy Logo page before/after saving | [Before](migration-b-legacy-logo-before.jpg), [Saved](migration-b-legacy-logo-saved.jpg) |
| Converted models in the library | [Models](migration-b-converted-models.jpg) |

At the end of stage b, compatibility coverage and desktop output remained for
stage c, now recorded above. Aligned frame-by-frame parity was subsequently verified in stage d above.
No menu has been removed; clock support remains out of scope.

## Library correction before stage c

Assigned channel models are identified by their actual bindings, never by a
library name or group label. The library separates **Channel graphics** and
**Scenes**, translates role names for display only, and shows their saved
artwork, picture window and ticker text. Ordinary GC fields, duration/L-shape
rules and take buttons are absent from these cards and their On air picker.

Logo/ticker cards use visibility switches; L-shape uses Enter/Return; scenes
use Apply. These controls share the existing engine handles. Edit opens Logo,
L-shape or Quick scenes in that delivery. Ticker editing was a static-text
section at Logo `#ticker`; stage e replaced these targets with dedicated model
editors. Ticker saves preserve other layers. Empty tickers ask for configuration.
Saving a hidden ticker does not restore it to air. Existing rows are not renamed
for localization, and no migration was changed.

The new channel-control API and its requested-visibility/restart semantics are
in [the API reference](../api.md#graphics-templates). A scene is an action, not
a permanent toggle. Thumbnails show the saved model, not the live program or
all transient overrides. Native desktop-output compatibility is distinct from
the browser viewport tests below.

Verification: fmt, exact no-default Clippy, 680 Rust tests passed (14 ignored),
Vue type build, frontend ESLint, changed-file Prettier, QA TypeScript and four
Playwright regressions passed. The running disposable backend also passed a
separate pt-BR browser interaction run at 1440 px and 390 px: role controls,
legacy state responses, all edit targets, saved ticker text, scene application
on mobile and no horizontal overflow.

Captures: [initial empty ticker](library-channel-empty-pt-BR.jpg),
[ticker editor](library-ticker-edit-pt-BR.jpg),
[channel models](library-channel-on-pt-BR.jpg),
[scene applied](library-scene-applied-pt-BR.jpg),
[phone top](library-channel-phone-top-pt-BR.jpg),
[phone controls](library-channel-phone-controls-pt-BR.jpg),
[phone scenes](library-channel-phone-scenes-pt-BR.jpg).
[Machine-readable evidence](library-channel-evidence.json).

The subsequent stage c coverage is recorded above, including legacy routes,
scheduled events, clip marks, replacement/placement, engine transitions,
desktop visual checks and XMLTV, with the stated runtime limitations.

## What is left, in order

1. Approved stages a–e are complete within the evidence and limitations above.
   Desktop audio stability, physical outputs and long-soak verification remain
   explicit limits; clock support remains excluded.
2. Later phases (see roadmap): rundowns 2.5 (`on-air-design.html`), graphics
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
