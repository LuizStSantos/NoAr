# Graphics work: handoff

Branch: `claude/branch-review-opz967`. Read this file, then
[`../graphics-roadmap.md`](../graphics-roadmap.md), `CLAUDE.md` and
`backend/AGENTS.md` before changing anything.

## Integrated fire test — 2026-11-08

**Result: `PASS_WITH_CAPTURE_RECHECK`, not a clean PASS.** One actual channel
ran the complete 900-second scenario against disposable SQLite and local
synthetic sources. All 43 final checks passed. The raw runner result remains
`FAIL` because one short clip-mark screenshot sampled an exit animation;
43 consecutive decoded frames independently verified the expected artwork,
lower-left video and border. Both images and the original incident are kept.
This does not certify frame-accurate transition timing.

Executed: Scheduler created the SRT live and timed graphic through Chromium;
SRT, RTMP, UDP and HLS entered automatically; Clock/Continue returns, a scheduled
commercial, clip-specific logo/artwork, logo swap/move, scene, L-shape with GC,
manual/scheduled graphic overlap, CSV crawl update, clock, animated GIF,
playout restart and five short GCs. The restart preserved permanent models and
CSV value `5.20`. Independent audio checks found 880 Hz in all selected live
segments and 550 Hz in the commercial.

The archive contains 451 original TS segments in two output generations. Both
fully decode; video DTS advances by 40 ms and audio by at most 21.344 ms, with
A/V tail differences below 59 ms. No intra-segment continuity/transport errors
were found. Cross-file muxer counter restarts are recorded separately.
The primary backend log has no `ERROR` or fatal lines, but **seven warnings**, including
one 1.0 s pacing reset during deliberate SRT loss. During audio-only UDP, the
video-only ROI held its picture for up to **21.513 s** before recovery. These
faults had explicit grace windows; this is not seamless recovery certification.
Whether video loss should switch to waiting/filler sooner remains a follow-up.

The initial development runtime 1.26.2 reproduced its known native decodebin3
abort. Docker already ships corrected GStreamer core/base/libav **1.26.11**;
those exact libraries were extracted read-only from the available stable image.
The final run used the unchanged decoder and the **same NoAr binary hash** as
the initial run. The provisional decoder experiment was removed: no backend,
API, migration or UI product source changed for this task.

Executed on the aligned runtime: fmt, exact no-default-features Clippy/all
targets/deny warnings, Rust suite **721 passed / 14 ignored**, build, **13**
picture/soak observer tests and **4** fixture/preflight tests. Node/Python syntax,
ESLint and changed JavaScript Prettier checks passed. A separate browser proof
waited for two presented video frames on dark pt-BR desktop and phone, confirmed
live UDP and a uniquely labelled GC, verified all 8 saved models through the
API, and captured Scheduler and the Models viewport. The earlier black capture after reload is retained as transient evidence.

[Report, logs and all prints](fire-test-2026-11-08/README.md),
[summary](fire-test-2026-11-08/summary.json),
[analysis](fire-test-2026-11-08/analysis.json),
[raw result](fire-test-2026-11-08/raw-report.json),
[reproducible harness](../../qa/fire/README.md).

Prints: [On air](fire-test-2026-11-08/fire-onair-verified-pt-BR.jpg),
[phone](fire-test-2026-11-08/fire-onair-verified-mobile-pt-BR.jpg),
[Scheduler](fire-test-2026-11-08/fire-scheduler-pt-BR.jpg),
[Models](fire-test-2026-11-08/fire-models-pt-BR.jpg),
[HLS L-shape/GC](fire-test-2026-11-08/output-fire-lshape-and-gc.jpg),
[aligned clip mark](fire-test-2026-11-08/output-fire-clip-lshape-aligned.jpg),
[original transition snapshot](fire-test-2026-11-08/output-fire-clip-lshape.jpg).
Desktop screenshots are 1440×1000; phone is 390 px wide/full-page; output is 640×360.

NOT_RUN: 24-hour soak, physical SDI/NDI, desktop audio, WAN impairments,
multi-channel capacity, output RTMP/SRT/UDP and transition parity. The runner and browser driver
were loaded before the snapshot/readiness refinements; its exact loaded-source
hash was not captured. Final helper hashes identify the committed tools, not
an unperformed clean online rerun of that later adjustment.

## Shared checkerboard and matching README screenshot dimensions

Templates, On air Preview and Composer now share one neutral transparency
checkerboard in `TemplatePreview.vue`. The blue/green scenery backgrounds were
removed from the library and On air. Composer adds its existing alignment grid
and optional safe-area guides to the shared pattern. Program shows the real
video; these preview backgrounds are not part of saved models or encoded output.

All **14 README gallery PNGs were recaptured at exactly 1440×1000 pixels**,
in dark mode, from the production frontend with the owned Example TV demo and
its actual running backend. Templates now uses a viewport capture too. The
helper fixes device scale to 1 and asserts PNG width/height after every capture,
plus theme, document width and the shared preview's computed background styles.
The screenshot guide and README captions describe scrolling/filtering groups.

Executed: Vue types, ESLint, changed-code Prettier, QA types, production build,
architecture checks and **38 Chromium browser tests passed**. The new regression
checks all three preview backgrounds at 1440×1000 and 390×844 in dark pt-BR,
retains Composer's two grid gradients and safety guides, and confirms Program
does not use the authoring preview. The gallery capture passed all 14 screens,
with two advancing real-video checks, no page errors and `/health` `ok` / `on_air`.
All PNG dimensions, image hashes and preview-source hashes were checked again.

[Gallery evidence](../images/readme-captures.json),
[UI evidence](preview-checkerboard-verification.json),
[browser suite](preview-checkerboard-browser-test-log.txt).

Screenshots:
[Templates desktop](preview-checkerboard-templates-1440-pt-BR.jpg),
[On air desktop](preview-checkerboard-onair-1440-pt-BR.jpg),
[Composer desktop](preview-checkerboard-composer-1440-pt-BR.jpg),
[Templates phone](preview-checkerboard-templates-390-pt-BR.jpg),
[On air phone](preview-checkerboard-onair-390-pt-BR.jpg),
[Composer phone](preview-checkerboard-composer-390-pt-BR.jpg).
Phone QA captures are full-page evidence; the README gallery uses only the
fixed-size PNGs in `docs/images/`.

No Rust/server source changed. NOT_RUN for this UI correction: Rust/media-renderer
suites, new HLS raw-frame parity, Firefox/Safari and physical devices. Earlier
scrolling renderer evidence below retains its original scope.

## Scrolling text and live information

Text layers now support continuous horizontal movement in Composer: static,
right to left or left to right, with speed in output px/s and repeat spacing
in output pixels. Literal text, operator fields and existing CSV/RSS/JSON
field mappings use the same layer path. Sources still select one record;
this does not concatenate a complete feed, rotate records or add vertical rolls.
Use the [operator guide](../graphics-data-sources.md#scroll-text-and-live-information)
for authoring, units and limits.

The engine prepares a bounded single-line glyph strip outside the frame thread.
Frames blend cached pixels inside the layer rectangle, applying text/fill
opacity together once. Entry/exit animation and delay remain layer controls.
Temporary/permanent updates retain the existing take clock; L-shape artwork
updates inherit the preceding artwork epoch. Existing JSON without `scroll`
stays static; no migration or frozen legacy-column update is needed.
The edit canvas stays still; its timeline previews movement. On air animates
the browser preview and keeps Program as the actual output.

Executed in an owned disposable SQLite installation with **GStreamer 1.26.2**:

- `cargo fmt --all`, exact no-default/all-target Clippy with denied warnings,
  **721 Rust tests passed / 14 ignored**, and normal backend build.
- Vue types, ESLint, changed-code Prettier, QA types and production frontend
  build passed; architecture checks found no boundary violations.
- **37 browser tests passed** against the production frontend, including
  scroll direction, speed/gap/undo, saving/reloading source mappings, record
  selection, manual overrides, invalid speed and dark pt-BR desktop/phone.
  The focused scroll test passed again after the final engine opacity change.
- Actual **640×360/25 fps HLS**: manual leftward text at half opacity, mapped
  rightward text, permanent leftward text, live CSV replacement, hide and restore.
  Completed TS segments retained two frames one second apart. Motion matched
  **80 ± 2 px/s**, with zero error in the matched glyph profiles. The 2 px
  tolerance covers 4:2:0 snapping, frame scheduling and compression; this is
  a displacement check, not encoder byte identity or legacy migration parity.
  Half-opacity white text over blue measured RGB **129,129,255**, confirming
  the text/fill group does not apply opacity twice. Hidden frames had no text.
- A real application restart preserved the model and cached source exactly,
  restored the permanent model and produced moving text in fresh HLS segments.
  All recorded health checks were `ok` / `on_air`; the scoped HLS log had
  **zero ERROR entries**. The final On air browser capture checked advancing
  actual video, moving source preview and no page errors.

[Verification and source hashes](scroll-verification.json),
[Rust log](scroll-rust-verification-log.txt),
[browser suite](scroll-browser-test-log.txt),
[HLS measurements](scroll-hls-evidence.json),
[restart](scroll-restart-evidence.json),
[On air browser](scroll-onair-browser-evidence.json),
[health](scroll-health.json), [scoped channel log](scroll-channel-log.txt).

Screenshots: [Composer](composer-scroll-pt-BR.jpg),
[phone](composer-scroll-phone-pt-BR.jpg), [On air](scroll-onair-pt-BR.jpg),
[manual half opacity](output-scroll-manual-left.jpg),
[one second later](output-scroll-manual-left-later.jpg),
[rightward data](output-scroll-data-right.jpg),
[permanent model](output-scroll-permanent-left.jpg),
[updated USD 5.20](output-scroll-data-updated-later.jpg),
[hidden](output-scroll-hidden.jpg), [restored](output-scroll-restored.jpg),
[after restart](output-scroll-restarted.jpg).
All are at most 1440 px wide. Browser-test models were removed from the owned
fixture before the final On air screenshot to show the scrolling example alone.

NOT_RUN for this addition: Firefox/Safari, live public RSS/JSON providers,
physical desktop/SDI/NDI devices, GPU encoders, a long-duration soak and a new
2.2.1 frame-parity/API comparison. Provider parsing/last-good unit tests remain
part of the executed Rust suite; actual scrolling output used manual text/CSV.

## README dark-mode gallery refresh

The project [README](../../README.md) now describes the layered Composer,
custom groups, clocks, fonts, data sources, template scheduling and Storage
downloads. Continuous text scrolling was still pending at that revision;
the delivery above supersedes that status.

All **14 gallery screenshots** were captured from the actual English UI in
dark mode, at most 1440 px wide, using an owned disposable Example TV database
and synthetic media. The capture run checked `ffp-dark` on every screen,
document width, advancing video in Player and Graphics On air, and `/health`
(`ok`, channel `on_air`). It recorded no browser page errors. The gallery adds
Composer, On air and Data sources. Templates originally used a full-page capture
for all groups; the fixed-size gallery above supersedes that capture.

See the [capture evidence](../images/readme-captures.json) for image hashes,
dimensions, video checks and application revision, and the
[reproduction guide](../../qa/screenshots/README.md) for isolated setup.
Screenshot helpers were syntax-checked; the capture helper was formatted.
The final seeder refinements were applied through the same API operations in
the owned fixture, but the final seeder revision was not rerun end to end on
another fresh database. No product or Rust source changed; the Rust and full
browser regression suites were not rerun for this documentation update.

## Cross-project organization follow-up

The follow-up expands maintainability work beyond Graphics to application
configuration, scheduled playback, XMLTV, engine output policy/desktop recording,
and frontend playlist, Scheduler, Storage, reports and output discovery.
See the [project handoff](../maintenance-handoff/README.md) for this revision's
executed checks and scope, and the [maintenance guide](../maintenance.md) for
source ownership. Earlier Graphics evidence below retains its original scope.

## Storage downloads and WebVTT subtitle guidance

Storage now offers native browser downloads in file rows, image cards and the
image viewer. Each click obtains a fresh channel/path-scoped access ticket;
the browser streams the existing file under its server filename. Folder rows
have no download action. A failed ticket request reports the error and leaves
the action usable. Existing preview/authentication routes are unchanged.

The upload picker now includes `.vtt`, and the backend accepts its browser MIME
`text/vtt`. The first browser run exposed that MIME rejection; it was corrected
before the clean full-suite repeat. The [Storage guide](../storage.md) explains
current-file downloads and [Closed captions](../closed_captions.md) documents
UTF-8 sidecars, same-name pairing, HLS settings, empty-file fallback and client
CC selection. Composer data fields refresh in place; continuous text/feed
scrolling controls were **still pending** in that delivery; see the addition above.

Verification on an owned disposable SQLite installation with **GStreamer
1.26.2**: fmt, exact no-default/all-target Clippy, **714 Rust tests passed / 14
ignored**, normal backend build, Vue types, ESLint, changed-file Prettier, QA
TypeScript, production frontend build and architecture boundaries passed.
All **34 browser tests passed**, including MP4/PNG/WebVTT byte comparisons,
Unicode filenames, image-viewer downloads, phone actions and ticket-error
recovery. The real HLS check uses a separate Video.js QA player on the actual
master output, not the NoAr low-latency monitor. It checks the uploaded accented
cue, `pt-BR` track/default metadata and the transition to a video without a
sidecar. Both cue display and empty fallback **passed**; `/health` reported
`on_air` / `ok`, the scoped channel log has **zero ERROR entries**, and the
browser reported no page errors. Delivered master/segment hashes are retained
in the executed evidence.

[Browser and HLS evidence](storage-subtitles-browser-evidence.json),
[Rust log](storage-subtitles-rust-verification-log.txt),
[browser suite](storage-subtitles-browser-test-log.txt),
[HLS channel log](storage-subtitles-channel-log.txt),
[HLS health](storage-subtitles-health.json).

Screenshots:
[Storage desktop](storage-download-files-pt-BR.jpg),
[Storage phone](storage-download-files-phone-pt-BR.jpg),
[Images phone](storage-download-images-phone-pt-BR.jpg),
[subtitle settings desktop](subtitles-hls-settings-pt-BR.jpg),
[subtitle settings phone](subtitles-hls-settings-phone-pt-BR.jpg),
[HLS caption displayed](output-webvtt-player.jpg),
[HLS empty fallback](output-webvtt-empty-fallback.jpg).
[Delivered master](output-webvtt-master.m3u8) and
[subtitle playlist](output-webvtt-playlist.m3u8) are captured evidence; their
relative segment URLs belong to the disposable run. Example delivered segments: [with cue](output-webvtt-cue.vtt) and
[empty](output-webvtt-empty.vtt). All screenshots are at most 1440 px wide.

NOT_RUN for this change: Firefox/Safari subtitle clients, physical output
devices, long-duration playback and continuous scrolling. WebVTT is an optional
HLS subtitle track; this does not add burned-in subtitles or SRT import.

## Graphics architecture and maintenance refactor

The application now owns a `graphics` feature module. Logo, L-shape and model
rendering live under the engine's Graphics domain. Shared scene/replacement/
placement actions are application services, used directly by the scheduler and
adapted by the old HTTP routes. DB conversion, compatibility projection and
resource persistence have dedicated owners; the real migration tests moved
with them. No migration or wire contract changes are intended.

Composer has one document/history owner and separate typed panels for canvas,
layers, appearance, field mapping and timeline, plus focused geometry/preview
composables. Unreachable old logo/L-shape/ticker editors were removed; active
subtitles and controller examples have dedicated Graphics components.

The [maintenance guide](../graphics-maintenance.md) includes the domain diagram,
source ownership, extension recipes, state/compatibility rules and the official
Rust/Vue/architecture references consulted. CI now checks source-level service,
HTTP, data and engine boundaries; its seven policy tests passed.

Final post-move checks so far: **714 Rust tests passed / 14 ignored**, exact
no-default/all-target Clippy, fmt, desktop-cpu all-target compilation and normal
backend build. Vue types, ESLint, Prettier and production build passed; all
**33 browser tests passed** in a fresh disposable installation. The final API
explanation revision passed **one affected API regression** again
([log](maintenance-final-api-regression-log.txt)). Actual pt-BR desktop/phone
checks also passed for guides, drag/save/reload,
subtitles save/reload/restore and read-only controller examples.
[Browser measurements](maintenance-composer-evidence.json),
[Rust log](maintenance-rust-verification-log.txt),
[browser suite](maintenance-browser-test-log.txt),
[architecture checks](maintenance-architecture-evidence.json),
[research references](maintenance-research.json).
Final compatibility passed **76 exact legacy HTTP responses** and both XMLTV
cases against released 2.2.1. Real HLS passed animated GIF/transparency, timed
text and picture-window ownership, manual-take preservation and overlapping
startup-disabled logo leases; the 400 ms fade has eight intermediate frames in
each direction. Its isolated channel log has zero ERROR entries.

The repeated static fixture passed **1,000 byte-identical raw YUV frames** and
**1,000 aligned HLS frames**, including logo fade and picture-window transitions.
Observed HLS error is zero; fixed limits remain MAE ≤ 0.75, p99.9 ≤ 8, max ≤ 32.
No position, size or timing adjustment was used. Segments 0–20 match; post-roll
segment 21 differs outside tested PTS 0–999. The 2.2.1 ticker region is empty;
new GIF/data/clock features are not claimed as historical ticker parity.
The capture worktree has only the approved pre-encode observer; the normal
binary has no hook. One initial capture failed before its first frame because
the test output directory was missing. Its logs are retained separately; a
clean DB/directory repeat passed.

[Verification index](maintenance-verification.json),
[legacy API/XMLTV](maintenance-compatibility-evidence.json),
[HLS/health metrics](maintenance-hls-evidence.json),
[HLS scoped channel log](maintenance-channel-log.txt),
[full frame metrics/hashes](maintenance-parity-evidence.json),
[comparison sheet](maintenance-parity-review.jpg),
[reference health](maintenance-parity-main-health.json),
[candidate health](maintenance-parity-branch-health.json),
[reference log](maintenance-parity-main-channel-log.txt),
[candidate log](maintenance-parity-branch-channel-log.txt),
[initial test-setup failure](maintenance-parity-setup-log.txt).
Individual sampled frames/differences are `output-maintenance-parity-*`.

Review **27 screenshots** in the [28-page PDF](maintenance-review.pdf),
[HTML gallery](maintenance-review.html) or [image index](maintenance-review-index.json).
The actual advancing Program, corrected clock/permanent-model API explanation,
subtitles and Composer were captured in pt-BR at 1440/390 px. Physical desktop/
device output, a formal soak and remote CI execution were not run in this
refactor. External feeds/Google Sheets and rollback installation were not
repeated; earlier evidence and limitations remain historical below.

| Owner | Before | After |
| --- | ---: | ---: |
| Composer route/document | 1,682 lines | 978 lines plus focused panels/composables |
| Application Graphics facade | 1,933 lines in utils | 525 lines in its own feature module |
| Graphics DB repository facade | 1,729 lines | 463 lines plus conversion/compatibility/resources |

These counts include adjacent tests where present; they describe ownership,
not a performance claim. Larger unrelated subsystems keep their existing owners.
The scope is a project-wide structural inventory and targeted refactoring with
runtime evidence, rather than an exhaustive line-by-line security audit.

## Composer checkerboard and optional safety guides

The Composer canvas now uses a neutral transparency checkerboard with a 10%
coordinate grid. **Safety margins** toggles white action guides 5% from each
edge and yellow text guides 10% from each edge. The checkbox starts off and the
aids stay local to the editor, outside template data and On air. Guides are
above artwork and do not intercept layer gestures. See the
[operator guide](../logo.md#composer-canvas-guides).

Verification: Vue types, full frontend ESLint, changed-file Prettier and the
production frontend build passed. All **three original Composer regressions**
passed in a fresh disposable installation. Additional actual pt-BR browser
interaction at 1440/390 px verified checkbox toggling without API writes,
5%/10% guide positions, no horizontal overflow, dragging through the guides,
saving/reloading authored layers, guides above opaque full-frame artwork and
absence from On air. No browser page errors were observed.
[Browser measurements](composer-guides-evidence.json),
[regression log](composer-guides-browser-regressions-log.txt),
[guides over full-frame artwork](composer-safety-margins-over-artwork-pt-BR.jpg).
No Rust, API or migration files changed in this follow-up; media-output and
Rust suites were not repeated for this editor-only change.

| View | Guides off | Guides on |
| --- | --- | --- |
| Desktop, pt-BR | [Checkerboard](composer-checkerboard-pt-BR.jpg) | [Safety margins](composer-safety-margins-pt-BR.jpg) |
| Phone, pt-BR | [Checkerboard](composer-checkerboard-phone-pt-BR.jpg) | [Safety margins](composer-safety-margins-phone-pt-BR.jpg) |

## Animated artwork, natural image insertion and template schedules

Implementation: `e80a27bf` on `claude/branch-review-opz967`.

This approved follow-up covers three operator requests:

- **Composer images:** first upload/storage selection uses original pixels at
  the channel output size, fitting down proportionally only when necessary.
  Same-output artwork becomes X/Y 0 and width/height 100%; replacements retain
  authored geometry. Actual browser interaction exercised full-frame insertion,
  Undo, collision-safe re-upload, save/reload and desktop/phone layouts.
- **Animated GIF:** native GStreamer decodes cached frames off the frame thread;
  durations and transparency are preserved in logo, ordinary/permanent artwork
  and L-shape backgrounds/overrides. Loops share a phase for matching loaded
  file/geometry and continue through hide/show/refresh. New decoding or restart
  starts a new sequence; browser GIF preview is independent. Limits are 10 MiB,
  256 frames, 60 seconds, 32 MiB per decoded animation and 64 MiB per composition.
  See [image behavior](../logo.md#image-sizing-and-animated-gif).
- **Scheduler:** new events choose a Composer model, preview, manual fields and
  source records, with second-level times and repetition. Legacy action events
  retain their compatible editor and snapshot semantics. Migration `00034` only
  adds the optional template reference; its test installs the real preceding
  migrations, preserves old action bytes and repeats the migration runner.
  A temporary event owns its take; end/edit/remove/disable retires only that
  instance. Permanent visibility leases restore initial state after the last
  overlap without changing startup enabled values. Saved permanent fields are
  used, not per-event overrides. See [scheduler semantics](../scheduler.md#scheduled-graphics).

The disabled-logo schedule path also enters/exits through its layer animations.
A manual enable during pending exit supersedes deferred cleanup. Tests cover
0/halfway/settled fade states, hidden/disabled restoration and manual precedence.
New backend graphics modules keep the required `graphics.rs` basename. No old
migration checksum or frozen legacy configuration is changed.

Verification on GStreamer 1.26.2: **714 Rust tests passed / 14 ignored**, fmt,
exact no-default/all-target Clippy and backend build passed. Vue types, ESLint,
Prettier, QA TypeScript and production build passed. The fade/preview revision
passed **33 browser tests** in a new empty fixture; the final ticker/controller
revision passed all **four affected browser regressions** again. Actual pt-BR
1440 px/390 px interaction exercised fields, record 2, a manual headline,
save/reload, full-frame insertion/Undo, actual advancing Program and deletion.

Actual HLS on the final normal binary proved:

- Changing permanent and scheduled GIF frames with transparent pixels; a saved
  manual GC survives the scheduled GIF's end.
- Scheduled text and animated L-shape entry/exit; its corner video returns to
  full frame, retaining the manual GC.
- Overlapping schedules of a startup-disabled logo: the first end keeps it on,
  the last restores off. The startup flag stays **0**. Its 400 ms fade has eight
  observed intermediate HLS frames both entering and leaving.
- Composer's uploaded 640×360 image becomes X/Y 0 and width/height 100; real HLS
  fills all four corners with the intended artwork, within RGB tolerance 12.
- A complete backend restart preserves the permanent GIF's ID and layers and
  produces both animation states again.
- Deleting a referenced template retains the event record; its editor reports an
  unavailable model and disables Save until a valid selection is made. The
  deletion warning distinguishes model references from legacy action snapshots.

[Verification index](animated-schedule-verification.json),
[HLS metrics/health](animated-schedule-hls-evidence.json),
[browser authoring](animated-schedule-browser-evidence.json),
[full-frame pixels](natural-full-output-evidence.json),
[restart](animated-gif-restart-evidence.json),
[deletion](scheduled-template-deletion-evidence.json).
Final compatibility on this implementation passed **76 exact HTTP responses**
and both XMLTV cases against released 2.2.1:
[API/XMLTV evidence](animated-schedule-compatibility-evidence.json).
The unchanged deterministic 40-second static-layout scenario also passed all
**1,000 raw YUV frames byte for byte** and **1,000 aligned HLS frames** with
observed error zero in the logo, full-frame L-shape and ticker regions, including
logo fades and L-shape transitions. No geometry or timing adjustment was used.
The unchanged HLS limits are MAE ≤ 0.75, 99.9th percentile ≤ 8 and maximum ≤ 32.
Segments 0–20 match; segment 21 is post-roll outside tested PTS 0–999. The
2.2.1 ticker region is empty; new GIFs and scheduled models use the separate
actual HLS proofs above. The isolated capture worktree adds only the approved
pre-encode observation hook; the normal binary has no capture hook.
[Metrics and binary hashes](animated-schedule-parity-evidence.json),
[comparison sheet](animated-parity-review.jpg),
[reference health](animated-parity-main-health.json),
[candidate health](animated-parity-branch-health.json),
[reference log](animated-parity-main-channel-log.txt),
[candidate log](animated-parity-branch-channel-log.txt).

Review this update's **34 screenshots**, including pt-BR desktop/phone and real
HLS, in the [35-page PDF](animated-schedule-review.pdf),
[HTML gallery](animated-schedule-review.html) or
[image index](animated-schedule-review-index.json). Individual images are at
most 1440 px wide; sampled parity frames/differences are also saved as
`output-animated-parity-*`.

The isolated HLS execution has **zero channel ERROR entries** in its
[scoped log](animated-schedule-channel-log.txt). The fixture's
[prior accumulated log](animated-schedule-prior-channel-log.txt) preserves an
earlier pre-final `Clip begin out of sync` (-12.044 s) during concurrent
build/browser work. It is **not classified as an environment limitation** without
a same-load baseline. This short isolated run is not a performance/soak proof;
physical/desktop GIF output was not tested.

| Part | Actual capture |
| --- | --- |
| GIF layer | [Composer](composer-animated-gif-pt-BR.jpg), [phone](composer-animated-gif-phone-pt-BR.jpg) |
| Natural full-output image | [Desktop](composer-natural-full-output-pt-BR.jpg), [Undo](composer-natural-size-undo-pt-BR.jpg), [phone](composer-natural-full-output-phone-pt-BR.jpg), [HLS](output-natural-full-frame.jpg) |
| Animated L-shape | [Composer](composer-animated-lshape-pt-BR.jpg), [phone](composer-animated-lshape-phone-pt-BR.jpg) |
| Scheduler model, records and manual field | [Desktop](scheduler-template-fields-pt-BR.jpg), [phone](scheduler-template-fields-phone-pt-BR.jpg) |
| Saved schedules | [Desktop](scheduler-template-list-pt-BR.jpg), [phone](scheduler-template-list-phone-pt-BR.jpg) |
| Actual Program | [Desktop](animated-gif-program-pt-BR.jpg), [phone](animated-gif-program-phone-pt-BR.jpg) |
| Referenced model deletion | [Desktop](templates-delete-scheduled-pt-BR.jpg), [phone](templates-delete-scheduled-phone-pt-BR.jpg) |
| GIF changes in real HLS | [Red](output-gif-permanent-red.jpg), [green](output-gif-permanent-green.jpg) |
| Scheduled GIF/text | [On](output-scheduled-gif-on.jpg), [out](output-scheduled-gif-off.jpg) |
| Scheduled animated L-shape | [Red](output-scheduled-animated-lshape-on-red.jpg), [green](output-scheduled-animated-lshape-on-green.jpg), [out](output-scheduled-animated-lshape-off.jpg) |
| Overlapping logo schedules | [On](output-scheduled-logo-overlap-on.jpg), [out](output-scheduled-logo-overlap-off.jpg), [fade in](output-scheduled-logo-fade-in.jpg), [fade out](output-scheduled-logo-fade-out.jpg) |
| GIF after backend restart | [Red](output-gif-after-restart-red.jpg), [green](output-gif-after-restart-green.jpg) |

## Open templates: final parity and review package

Implementation: `7f51b4df` on `claude/branch-review-opz967`, following stages
`23d73aa4` (free templates/defaults), `58716285` (shapes/clock), `750ea75c`
(fonts) and `80c9c9cd` (sources). No main merge, release or main push.

The final corrected revision was run against the same disposable 2.2.1 database
fixture and deterministic 640×360, 25 fps, 40-second synthetic media used for the
released-code reference. The isolated capture worktree adds only the approved
pre-encode observation hook; shipped engine sources have no capture hook.
All **1,000 YUV frames match byte for byte**, and all **1,000 PTS/content-aligned
HLS frames match exactly** in the logo, full L-shape and ticker regions, including
logo fade and L-shape entry/exit. No position, size or timing adjustment is used.
The fixed HLS limits remain MAE ≤ 0.75, 99.9th percentile ≤ 8 and maximum ≤ 32;
observed error is zero. Segment hashes 0–20 match; post-roll segment 21 differs
outside the tested PTS 0–999 interval. An empty settled ticker region in 2.2.1
and absence of clocks/mapped feeds in this fixture are explicit limits; new
features have their own HLS evidence below.

[Full metrics and binary hashes](open-final-parity-evidence.json),
[transition comparison sheet](open-final-parity-review.jpg),
[reference health](open-final-parity-main-health.json),
[candidate health](open-final-parity-branch-health.json),
[reference channel log](open-final-parity-main-channel-log.txt),
[candidate channel log](open-final-parity-branch-channel-log.txt).
Both captures report healthy output and zero channel ERROR entries. Full sampled
frames and amplified difference images are stored as `output-open-final-parity-*`.

The consolidated review provides **57 full screenshots** (pt-BR desktop/phone,
Composer, forms, fonts, sources, operator screens and actual HLS), plus a cover:
[58-page PDF](open-templates-review.pdf), [HTML gallery](open-templates-review.html)
and [image index](open-templates-review-index.json). The pre-fix deletion image
is explicitly marked as a bug reproduction, not desired output. Screenshots
identify disposable fixtures; the gray Program background is actual synthetic
video, and Composer's blue/green background is an authoring aid.

Current Rust verification is 708 passed / 14 ignored, with fmt, exact Clippy and
build passed on GStreamer 1.26.2. Vue types, full ESLint, Prettier, QA TypeScript
and production frontend build passed again. All **31 browser tests passed**
again in a new empty disposable installation, including first-run single-logo
assertions before fixtures. [Verification index](open-final-verification.json),
[Rust log](open-final-rust-test-log.txt), [browser log](open-final-browser-test-log.txt).

External RSS/JSON providers and a real published Google sheet remain **NOT RUN /
NOT AVAILABLE** due to cloud DNS failure. Local parsers/HTTP fixtures and actual
CSV/HLS updates passed; no network or TLS bypass was used. Private Sheets OAuth,
physical outputs and a formal soak are not included. Desktop audio's previously
recorded cloud-environment limitation is unchanged; that scenario is not repeated
in this open-templates follow-up.

## Open templates: final removal regression verification

Deleting an assigned logo now also clears external/clip image and position
controls; removing an L-shape binding clears its external/clip artwork and
placement and returns to full screen. Ordinary saves and replacement assignments
retain shared transient controls. Unassigning preserves the model and any
independent permanent setting. No migration or frozen legacy column changes.

GStreamer 1.26.2: fmt, exact no-default/all-target Clippy, full tests and backend
build passed: **708 tests passed, 14 ignored**. The HTTP deletion regression first
failed with an external/clip image, then passed with the cleanup. The new HTTP
L-shape test covers external artwork/placement and unassignment. All **76 exact
legacy HTTP responses and both XMLTV icon cases** passed again against released
2.2.1 in disposable installations. [Contract evidence](open-final-compatibility-evidence.json).

Actual HLS reproduced the leftover logo before the fix (old-logo region maximum
difference from the gray source: 106). After deletion with the corrected binary,
that region is exactly gray (maximum difference 0); removing an externally
replaced/moved L-shape also yields full-frame gray (maximum difference 0).
Capture health is OK and the channel log contains zero ERROR entries.
[Evidence](template-removal-evidence.json), [channel log](template-removal-channel-log.txt).

| Check | Capture |
| --- | --- |
| Bug reproduction before cleanup (not desired output) | [Before](output-template-delete-before.jpg) |
| Deleted logo and replacement are gone | [After](output-template-delete-after.jpg) |
| Removed L-shape binding returns to full screen | [Full screen](output-template-unassign-lshape.jpg) |

The final raw/HLS parity repetition and consolidated screenshot gallery are
linked in the section above. Stage 4's 707-test count below records its earlier
implementation revision.

## Open templates: stage 4 — data sources and live fields

**Graphics → Data sources** now provides uploaded/published UTF-8 CSV (including
public Google Sheets CSV URLs), RSS/Atom and HTTP JSON with JSON Pointer record
selection. Composer's Save panel maps named text/image fields to a source, column
and record. On air selects records and accepts nonempty manual overrides for
temporary takes; clearing an input restores automatic preview values, applied
on the next take. Permanent compositions use their saved mappings.

Migration `00033_graphics_data_sources.sql` persists configuration, uploaded CSV,
last-successful rows/status and revision/fetch guards. IDs are not reused, so late
responses cannot affect a newly created source. Earlier migrations are unchanged.
Deleting a referenced source returns 409 with model names. Removing a placeholder
prunes its mapping. Images name existing Storage files; remote images are not
imported. There are no default sources or extra startup templates: a fresh
installation/new channel still gets only the removable small Logo.

Fetching, parsing and rendering run outside the frame thread. URL polling runs
with playout; Refresh now also works off air. CSV replacement updates its values.
Last-good rows/timestamps survive provider/schema errors and backend restart.
Missing initial data blocks automatic take/permanent publication explicitly.
The UI reports errors and last success. Source URLs are administrative metadata
and are blanked for regular users. See [operator guide](../graphics-data-sources.md)
and [API contract](../api.md#graphics-data-source-api) for limits/semantics.

Verification: GStreamer 1.26.2; fmt, exact no-default/all-target Clippy, backend
build, **707 Rust tests passed / 14 ignored**; Vue types, full ESLint, Prettier,
QA TypeScript and production build passed. **31 Playwright tests passed**, plus
two source regressions after the layout refinement and one API/localization
regression. pt-BR desktop/390 px screenshots show authored mappings, source
status, record/manual controls, advancing Program, readonly Player and API
examples. The final mapping panel spans the form width.

Actual synthetic HLS checks, with saved segment hashes and region metrics:

- A CSV image/text/clock permanent composition changes USD 5.10 → 9.99.
- Removing a referenced CSV column keeps the previous text/image and timestamp;
  the clock continues changing. Restoring valid data updates output again.
- Browser controls select EUR and MANUAL. A subsequent source update changes
  the headline during the **same take ID**, with elapsed time still increasing.
  Clearing the override and taking again displays EUR 88.88 automatically.
- Source rows, model identity and static layout survive full backend restart;
  the static text region matches exactly in decoded HLS (MAE/max 0 in this run).
  A new CSV value after restart changes actual output to USD 8.18.
- Capture health is OK; the channel log has zero ERROR entries. The unattended
  pause is not treated as a formal soak test.

[Machine-readable evidence](data-sources-evidence.json).

**External-feed limitation:** public RSS/JSON fetch attempts failed/timed out at
DNS resolution in both the executor and container. No proxy, policy or TLS
verification was bypassed. Parsers and bounded local HTTP fixtures passed, but
external RSS/JSON provider integration and a real published Google sheet are
**NOT AVAILABLE / NOT RUN** here. Do not interpret CSV/HLS success as that proof.
Public-address protection requires working server DNS, even with an HTTP proxy.
Private Google OAuth and custom request headers are outside this delivery.

| Part | Capture |
| --- | --- |
| Current fresh installation: only Logo | [Desktop](open-templates-final-fresh-pt-BR.jpg), [phone](open-templates-final-fresh-phone-pt-BR.jpg) |
| Data sources | [Desktop](data-sources-csv-pt-BR.jpg), [phone](data-sources-phone-pt-BR.jpg) |
| Composer mapping and live preview | [Desktop](composer-data-mapping-pt-BR.jpg), [phone](composer-data-phone-pt-BR.jpg) |
| Real Program | [Desktop](data-onair-pt-BR.jpg), [phone](data-onair-phone-pt-BR.jpg) |
| Last-good status | [Desktop](data-last-good-pt-BR.jpg), [phone](data-last-good-phone-pt-BR.jpg) |
| Record/manual selection | [Manual](data-manual-operator-pt-BR.jpg), [phone](data-manual-phone-pt-BR.jpg), [automatic](data-automatic-operator-pt-BR.jpg) |
| Initial/change/failure/recovery in HLS | [Initial](output-data-initial.jpg), [changed](output-data-updated.jpg), [last good](output-data-last-good.jpg), [recovered](output-data-recovered.jpg) |
| Same take with live/manual data | [Manual](output-data-manual-record.jpg), [updated headline](output-data-manual-live-update.jpg), [automatic price](output-data-automatic-record.jpg) |
| Restart and new update | [Restart](output-data-after-restart.jpg), [new value](output-data-after-restart-updated.jpg) |
| Current library, Player and API | [Library](open-templates-final-library-pt-BR.jpg), [Player](open-templates-final-player-pt-BR.jpg), [API](open-templates-final-api-pt-BR.jpg) |
| External-provider DNS failure | [Explicit status](data-source-network-limit-pt-BR.jpg) |

Full historical API/raw/HLS parity repetition follows as a separate verification
step. Physical output and desktop audio checks are not repeated in this stage.

## Open templates: stage 3 — uploaded fonts

Composer now offers **Upload font** beside text/clock font selection. Validated
single-face TTF/OTF uploads (10 MiB each, 32 per channel) receive immutable UUID
families; duplicate filenames never replace an existing model's font. Migration
`00032_graphics_fonts.sql` stores bytes and metadata in SQLite, included in normal
database backup/restore. The engine restores channel faces at startup; browser
FontFace loads the same bytes. No manual installation inside Docker is needed.
Unknown/other-channel aliases cannot be saved; unavailable browser faces show a
warning. Missing glyphs may still use ordinary font fallback. Deleting templates
preserves fonts; there is no font deletion endpoint in this stage.

Verification: GStreamer 1.26.2, fmt, exact no-default Clippy, backend build and
**699 Rust tests passed, 14 ignored**. Vue types, full ESLint, Prettier, QA types
and production build passed. **29 Playwright tests passed** in a fresh data
directory. Font tests cover malformed bytes, duplicate filenames, channel
ownership, immutable storage and exact engine face-byte selection. Control
tests now install real migrations rather than a handwritten partial schema.
The browser regression uses semantic select lookup and waits for the saved route
before reload; a separate fixture rerun corrected reuse of old playlist files.

Actual pt-BR desktop/phone authoring used an uploaded family absent from system
fonts for text and a clock, saved/reloaded both and decoded advancing Program.
Actual HLS text and uploaded-font clock survived a full backend restart; the
static text region matched with the recorded compression-aware metrics. Capture
health was OK and the restarted log has zero ERROR entries.
[Evidence](uploaded-font-evidence.json). Full historical legacy/parity, physical
outputs and soak checks have not been repeated in this stage. Data sources are
the remaining approved implementation stage.

| Part | Capture |
| --- | --- |
| Upload/select a font for text | [Composer](composer-uploaded-font-pt-BR.jpg) |
| Same uploaded family in a clock | [Composer](composer-clock-font-pt-BR.jpg) |
| Phone | [Composer](composer-font-phone-pt-BR.jpg) |
| Actual Program | [On air](uploaded-font-onair-pt-BR.jpg) |
| Persisted uploaded font in HLS | [Before restart](output-uploaded-font-before-restart.jpg), [after restart](output-uploaded-font-after-restart.jpg) |

## Open templates: stage 2 — shapes and live clock

Composer now provides rectangle, rounded rectangle, ellipse/circle and line
layers with fill, stroke and stroke width. Existing rectangle rendering without
stroke remains unchanged. A real Clock layer offers five time/date formats and
an explicit IANA timezone, following the channel timezone when left empty.
Preview uses a server time reference; output uses server civil time independently
of clip position. The refresh worker runs outside the frame thread and preserves
take duration, animations and requested visibility. Render failures retain the
last frame and warn in the channel log; this is not a precision time signal.
No default clock or extra startup template is created.

Verification with GStreamer 1.26.2: fmt, exact no-default Clippy, backend build,
**697 Rust tests passed / 14 ignored**, Vue type build, full ESLint, Prettier,
QA TypeScript and production build passed. **28 Playwright tests passed**.
Two initial seek/SRT timing tests failed during concurrent browser activity;
both passed individually, then the entire suite passed with one test thread.
Scoped clock tests cover timezone, midnight and daylight-saving rollback;
engine tests check exact shape pixels and preservation of take/fade state.

Actual pt-BR desktop/phone browser interaction saved/reopened all four shapes
and the clock, with advancing real Program video and no page exceptions.
Decoded synthetic HLS shows changing seconds before and after a full backend
restart, correct shape fill/exteriors and hide/show retaining requested state
across refreshes. Capture health was OK; the restarted capture log has no ERROR
entries. [Evidence and compression-aware pixel checks](shapes-clock-evidence.json).
The retained output checks use color tolerance on decoded HLS; exact primitive
pixel geometry is checked before encoding by unit tests. This stage has not
repeated the historical 76-response/2.2.1 parity, desktop/physical or soak runs.
No playout pause/seek control was exercised. Fonts and sources remain pending.

| Part | Capture |
| --- | --- |
| Shape controls and composition | [Composer](composer-shapes-pt-BR.jpg) |
| Live clock authoring | [Desktop](composer-clock-pt-BR.jpg), [phone](composer-clock-phone-pt-BR.jpg) |
| Real Program | [Desktop](clock-onair-pt-BR.jpg), [phone](clock-onair-phone-pt-BR.jpg) |
| Actual changing HLS | [First](output-clock-before-restart-first.jpg), [later](output-clock-before-restart-later.jpg) |
| Visibility preserved during refresh | [Hidden](output-clock-hidden.jpg), [shown](output-clock-shown.jpg) |
| Persistence and live refresh after restart | [First](output-clock-after-restart-first.jpg), [later](output-clock-after-restart-later.jpg) |

## Open templates: stage 1 — templates and first-run defaults

The user approved [the next revision](open-templates-plan.md), then required
fresh installations and new channels to contain **only one removable Logo
example**. Existing installations retain all authored/converted models. This
approved revision supersedes the historical reserved-group UI and the earlier
clock exclusion. Clock is now requested, but not implemented in this stage.

Migration `00031_graphics_open_templates.sql` separates completed conversion from
role existence and adds startup visibility for user-authored permanent models.
It uses normal sqlx migration tracking; `graphics_channel_state` records completed
channel import, not a separate migration ledger. Earlier migration checksums and
frozen processing columns remain unchanged. Deleting all role bindings/models
therefore does not import old artwork again on restart.

The library uses authored names and group labels for every composition. Groups
are labels, not rendering modes or persistent empty folders. Removing/moving the
last model removes that empty group from the filter. Compatible free models can
be Always on air: up to **16 per channel including compatibility permanent
roles**, checked before saving and enforced with SQLite triggers. User permanent
compositions draw in ascending template-ID order, in full-frame space after
L-shape. They retain their own requested visibility through pixel replacement
and take-all-out does not remove them. A temporary take of a permanent model is
rejected, preventing duplicate instances.

Composer's Save form offers an optional logo/ticker/L-shape assignment and an
explicit image-layer target for legacy replacement/placement. Assignment changes
neither the authored name nor its group. Replacing a role retains its old model
as temporary, avoiding a second permanent copy. Missing legacy editor targets
open the library. Role APIs, scenes and clip overrides continue to address their
selected model rather than frozen processing layout.

**Delete template** works on assigned models too. The focus-trapped confirmation
loads current use, permanent visibility and potentially affected scheduled action
snapshots before confirmation. It removes channel/scene bindings, takes active
temporary instances out, and preserves uploaded assets and scheduled snapshots.
Those events may still execute their saved actions later. The database operation
is transactional; runtime reload follows commit. A reload failure is reported,
not presented as an atomic rollback of database and output; restart recovers the
persisted state. Capacity/render preflight and a database limit prevent invalid
permanent saves from leaving a partially persisted 17th model.

Verification with GStreamer 1.26.2:

- `cargo fmt --all`, exact no-default Clippy with denied warnings, complete
  no-default tests and backend build passed: **691 passed, 14 ignored**.
  Added tests cover first-run single-logo state, deletion/restart without
  resurrection, cross-channel access, explicit image targets, permanent state,
  capacity/concurrent-writer enforcement and rejected artwork without a saved row.
- Vue type build, full ESLint, Prettier, QA TypeScript and production build passed.
  **27 Playwright tests passed** against the final disposable backend/database.
  The existing playlist-failure test now waits for initial day loading before
  adding its fixture, avoiding an unrelated setup race.
- pt-BR desktop/phone browser interaction created image+text compositions with
  user groups, selected the second image for API control, deleted an assigned
  model, verified the missing editor redirect and decoded advancing Program video.
- Actual HLS frames before and after backend restart show the free permanent
  composition, its artwork/text and an empty former-logo corner. The removed
  template and logo assignment remain absent. Capture-time health is OK and
  the channel log contains no errors. [Evidence](open-templates-evidence.json).

| Part | Evidence |
| --- | --- |
| New installation: only Logo | [Desktop](open-templates-fresh-pt-BR.jpg), [phone](open-templates-fresh-phone-pt-BR.jpg) |
| Free permanent composition | [Composer](composer-open-free-permanent-pt-BR.jpg) |
| Optional role and controlled image | [Composer save](composer-open-optional-assignment-pt-BR.jpg) |
| Authored groups/names | [Desktop](open-templates-custom-groups-pt-BR.jpg), [phone](open-templates-custom-groups-phone-pt-BR.jpg) |
| Assigned deletion and use | [Confirmation](open-templates-delete-assigned-pt-BR.jpg) |
| Actual Program | [On air](open-templates-live-permanent-pt-BR.jpg) |
| API tutorial uses authored names | [API](open-templates-api-names-pt-BR.jpg) |
| Actual decoded HLS | [Before restart](output-open-templates-before-restart.jpg), [after restart](output-open-templates-after-restart.jpg) |

This stage has not repeated the full historical 76-response/2.2.1 parity or
physical/desktop-output runs. The new shapes, clock, font upload and data-source
page remain next, not completed by this stage-1 commit.

## Composer startup fixes and operator screen review

This follow-up fixes the fresh-installation reports after the layer revision.
Composer uses the existing browser-compatible UUID helper rather than calling
`crypto.randomUUID` directly. The regression explicitly removes that API on
loopback; adding image, text, shape and video layers, saving and reloading pass.
A real LAN HTTP origin could not be reached under the cloud network policy;
that origin itself is not claimed as tested.

First-run conversion previously preceded Storage asset copying and could freeze
the default logo as a full-frame layer. The stock image now resolves through the
same packaged-asset locator used by Storage. New channel creation converts only
after its outputs exist. The 42×42 px default uses 12 px top/right margins.
Custom paths never borrow the stock image geometry; existing authored models
are not rewritten, and no migration checksum changed. For an already oversized
saved logo, select its primary image in Composer, use **Restore image size and
top-right position**, review and save. This explicit operation supports Undo,
limits oversized assets to the output and does not save automatically.
Permanent logo/ticker models reject video windows in the menu, matching the
existing backend contract; L-shape and temporary models support that layer.

On air no longer embeds the legacy live logo/L-shape editor or quick scenes.
Its picker separates assigned channel models from ordinary GCs; bound controls
use their bindings, not temporary take. **Always on air** operates requested
logo/ticker visibility; selecting Channel L-shape offers entry/full-screen.
Scenes remain available through **Templates › Scenes › Apply**. The Player
shows a read-only model/temporary status line with **Open Graphics**. It has no
graphics switches, placement panel or scene buttons.

**Preview** illustrates the selected saved model, including the stock logo when
there are no ordinary templates. **Program** uses the existing real Player
preview and its stopped/wait/error/unsupported states. Requested visibility is
not proof that a logo is currently drawn: clip marks, breaks and L-shape can
suppress it. Clock and version 2.5 rundowns remain excluded.

Review all **32 current screenshots** in [the gallery](review-gallery.md) or
[the 33-page PDF](review-gallery.pdf), including desktop, 390 px phone, Composer,
channel models, stopped/live On air, Player and decoded HLS frames.

Verification for this follow-up:

- GStreamer 1.26.2: `cargo fmt --all`, exact no-default Clippy with denied
  warnings and `cargo test --no-default-features` passed: **686 passed,
  14 ignored**. Two added database regressions cover missing first-run storage,
  correct new-channel output dimensions and preservation of a deliberate
  later full-frame edit. The backend binary was built and run.
- `npx vue-tsc --build`, full ESLint, changed-file Prettier, production frontend
  build and QA TypeScript passed. All **26 Playwright tests** passed against a
  fresh disposable backend/database. Global setup verifies the untouched stock
  logo and bound-model-only Preview before custom fixtures are seeded.
- pt-BR browser captures exercise actual drag/resize, all four layer kinds,
  upload, fields, animation, save/reload, natural-size logo repair, role controls
  and desktop/phone layouts. The evidence below records decoded Program video
  dimensions and advancing playback time, rather than relying on a black box.
- HLS evidence and capture-time health/log results are recorded in
  [the review evidence](review-evidence.json). These are actual `.ts` decoded
  frames, not Composer illustrations. The stock logo's measured content box
  is exactly `(586,12)–(628,54)` at 640×360: one 42×42 logo. The RGB comparison
  includes the saved 0.7 opacity; model position/size are exact. Documented
  compression limits are mean RGB error ≤8 and p99 ≤50, including chroma edges.

This is a scoped fresh-installation/operator regression run, not another full
2.2.1 frame-by-frame parity run. The earlier 76-response and stage-d parity
records remain separate evidence. Physical outputs, long-soak/desktop audio
stability and a full Docker image rebuild were not repeated. No production
installation or user media was used.

## API tutorial and disposable-machine testing

The External control (API) page now explains channel layers, startup visibility
versus current on-air state, transient image/position overrides, the independent
XMLTV icon and ordinary template take/out. It distinguishes the existing graphics
key from login-token authorization. Selectors use real channel/template IDs and
saved field names; opening the tutorial only performs reads. Examples cover role
on/off, complete-model GET/jq/PUT for `always_on`, take, on-air and out. Exit
animation can keep a template listed as `leaving=true` until it finishes.

Verification for this frontend/documentation follow-up:

- `npx vue-tsc --build`, full ESLint, changed-file Prettier, production frontend
  build and the E2E TypeScript check passed. No Rust source changed; Rust checks
  were not repeated for this follow-up.
- The new Playwright regression passed against the disposable backend: real IDs,
  logo/ticker/L-shape selection, temporary field examples, no writes on navigation,
  mobile width, a failed model fetch and retry. This is one targeted new test;
  the earlier 22-test baseline below is a separate recorded run.
- [Browser evidence](api-models-browser-evidence.json): pt-BR desktop/phone,
  no JavaScript page errors and no horizontal overflow. Captures:
  [model explanation](api-models-explanation-pt-BR.jpg),
  [authenticated controls](api-models-auth-pt-BR.jpg),
  [take/out](api-models-temporary-pt-BR.jpg),
  [phone](api-models-phone-pt-BR.jpg). No token/key values appear in these captures.
- [Executed examples](api-models-curl-evidence.json): 16 command groups copied
  from the rendered page passed using a valid login token, including actual
  role on/off, complete GET/jq/PUT, field substitution, temporary take/on-air/out
  and waiting for exit completion. Original role visibility and template
  documents were restored; health stayed OK. This validates the API commands,
  not a new frame-by-frame renderer comparison.
- [Disposable Compose validation](graphics-test-compose-evidence.json): the
  local image, project-specific fresh volume, port 8787 and disabled updater
  were checked with `docker compose config`. The complete Docker image build
  was not repeated; browser/API checks used the existing GStreamer staging
  backend. The production frontend build was run.

The user explicitly wants an empty test-machine installation without manual
backup or parallel stable operation. [The test guide](../graphics-testing.md)
stops stable, clones `claude/branch-review-opz967`, builds locally and starts
[`docker-compose.graphics-test.yml`](../../docker-compose.graphics-test.yml).
It opens setup on port 8787 and uses its own empty named volume. This recipe
exercises a fresh installation; migration of existing 2.2.1 data remains the
separate proof below. No release or image publication is involved.

## Layer-based channel models (current revision)

This revision supersedes the native-layout storage and dedicated logo/L-shape/
ticker editors described in the historical stages below. Channel models now use
Composer layers for both editing and application rendering. Clock support remains
outside this delivery.

| Concern | Authoritative state after startup conversion |
| --- | --- |
| Logo | Bound template `layers`: image path, percent geometry, opacity, entry/exit animation and timing. |
| L-shape | Bound template `layers`: composed artwork and a video layer with the picture window. |
| Ticker | Bound template text layers. |
| Permanent visibility | `graphics_channel_bindings.enabled`, exposed as `always_on` for logo/ticker. Library off is a transient hide; turning an unconfigured role on also enables its startup binding. Saving a hidden model preserves its transient hide. |
| Compatibility image target | `graphics_channel_bindings.control_layer_id`: stable image ID, preserved across layer reordering and clients that omit metadata. Deleting that layer selects the next image once. |
| Scenes | Ordered scene bindings and `settings.scene` action snapshots; these are actions, not a second layout source. Scheduled events retain their action snapshots and reversal. |

Migration `00030_graphics_layer_layout.sql` adds the conversion version and stable
image target to the existing bindings. Automatic conversion runs after the normal
sqlx migrator, inside the existing all-channel transaction. It upgrades both a
2.2.1 database and an installation already converted by 00029. Existing model IDs,
bindings, enabled states, additional layers and freely edited L-shape windows are
preserved. The 00029 logo expressions become percent geometry at the channel's
output size; cut/fade become layer animations. Role-template layout `settings`
become `{}` and are never read again after the binding's completed conversion.
Repeating startup does not overwrite edited layers. Frozen legacy processing
columns remain untouched; previous migration checksums remain unchanged.

The legacy configuration API projects a compatibility DTO from these layers.
Writes update the controlled layer/window and binding in one transaction;
unrelated configuration saves preserve custom layers, animations and free windows.
External API/key/scene replacements clone the composed definition and replace only
the controlled image. Logo placement translates its composition relative to that
image. Clip overrides inherit the model artwork/window unless their own placement
is explicit; external overrides retain precedence and clearing them restores the
clip/model. These transient operations do not rewrite the template. Application
output renders model layers; the engine's native constructors remain available to
SDK callers, without an extra application logo plane.

Legacy timing has two defaults when no duration was configured: 400 ms for external
logo controls, 1000 ms for clip marks. Conversion preserves both as `animation_ms`
and optional `clip_animation_ms`. Changing duration in Composer clears the clip
override so the authored duration applies consistently. Zero duration is a cut.
XMLTV still prefers the explicit guide icon, then the configured primary image,
and never a transient replacement.

Library **Edit** opens Composer for logo, L-shape and ticker. **Always on air** is
available for the two permanent roles. Selecting the video layer exposes corner
and picture-size controls; dragging still supports a freely positioned window.
Scenes keep their action editor inside Graphics. Legacy page URLs resolve their
binding and redirect to Composer; no native-layout editor remains in this flow.
The independent guide image has its own **Graphics › XMLTV logo** page, with
storage selection, preview and a separate save. It uses the existing
`processing.guide_logo` setting, not the on-air image layers.

Current-revision browser captures (pt-BR; 1440 px desktop, 390 px phone):

| Part | Desktop | Phone |
| --- | --- | --- |
| Logo layers | [geometry](composer-layers-logo-pt-BR.jpg), [animation and Always on air](composer-layers-logo-permanent-pt-BR.jpg) | [logo](composer-layers-phone-logo-pt-BR.jpg) |
| L-shape layers/window | [L-shape](composer-layers-lshape-pt-BR.jpg) | [L-shape](composer-layers-phone-lshape-pt-BR.jpg) |
| Ticker | [text](composer-layers-ticker-pt-BR.jpg), [Always on air](composer-layers-ticker-permanent-pt-BR.jpg) | [ticker](composer-layers-phone-ticker-pt-BR.jpg) |
| XMLTV logo tab | [guide icon](composer-layers-xmltv-pt-BR.jpg) | [guide icon](composer-layers-phone-xmltv-pt-BR.jpg) |

Current-revision runtime verification:

- GStreamer **1.26.2**: `cargo fmt --all`, the exact
  `cargo clippy --no-default-features --all-targets -- --deny warnings`, and
  `cargo test --no-default-features` passed. Totals: **684 passed, 14 ignored**
  (384 app library, 34 HTTP, 5 as-run, 261 engine). New regressions cover real
  00029→00030 migration, transactional rollback, stable IDs/targets, no later
  settings reads, edited free windows, zero-duration cuts and exact logo pixels.
- `npx vue-tsc --build`, full ESLint, changed-file Prettier and `npm run build`
  passed. Seven graphics browser regressions passed, including the three
  retained Composer tests, assigned controls/navigation/scenes and XMLTV saving
  without changing logo layers. The complete browser suite then passed
  **22/22** on a separate freshly initialized database.
  [Full-suite and restart evidence](layers-e2e-evidence.json) confirms both
  saved playlists, scheduled events and graphic bindings survive a backend restart.
  The harness now seeds its owned logo/guide assets and initial scene.
- [76 exact legacy HTTP responses and both XMLTV cases](layers-api-evidence.json)
  match the official 2.2.1 baseline. [Browser evidence](layers-browser-evidence.json)
  records saves/reloads, storage selection and desktop/phone layout, with no
  JavaScript page errors.
- [15 real HLS captures](layers-output-evidence.json) verify clip logo image/
  position/hide/restore, external overrides, clip L-shape artwork/placement,
  free-window preservation, extra composed layers across replacements/moves,
  scheduled action snapshots and reversal. [Channel log](layers-controls-channel.txt)
  contains no errors. [All 12 frozen graphic columns](layers-frozen-columns-evidence.json)
  still match the pre-00030 backup, including after browser saves and another
  [original-fixture restart](layers-restart-evidence.json); role IDs also remain
  unchanged and output returns healthy on air. 2.2.1 has no persisted ticker
  text to import.
- Composer saves were also observed in actual HLS:
  [new logo image](output-composer-layers-saved-logo.jpg),
  [static ticker](output-composer-layers-saved-ticker.jpg), and
  [model L-shape with ticker above it](output-composer-layers-saved-lshape.jpg).
  [Pixels, segment hashes and health](layers-composer-output-evidence.json)
  confirm the saved image replaces the old logo and is suppressed during L-shape.
- [Current parity evidence](layers-parity-evidence.json): fresh copies of the same
  2.2.1/00027 database, same deterministic 40-second 640×360/25 fps source and
  marks; actual main `dd4529e2` versus migration 00030. **1000/1000 pre-encode
  YUV frames and 1000/1000 PTS-aligned HLS RGB frames are byte-identical**.
  Every frame's logo, L-shape and ticker region passed, including fade and
  picture transitions. Maximum RGB error is **0**; no geometry/time shifts or
  threshold relaxation were used. The unchanged compression limits remain
  MAE≤0.75, p99.9≤8, maximum≤32, in addition to mandatory raw equality.
  HLS segment indices 0–20 also have identical bytes; only final partial
  post-roll segment 21 differs (1.56 s versus 1.60 s after 42 s), outside the
  0–40 s test interval, due to stop/flush wall time. Segment filenames include
  per-process identifiers. Raw/TS sets remain under
  `/workspace/noar-cloud/layers/parity/{main,branch}` and are retained in the
  [5.7 MiB proof archive](layers-parity-raw-hls.tar.gz) (2000 raw frames, 44 HLS
  segments, deterministic media, health and channel logs; no databases or credentials).
  [Checks and archive SHA-256](layers-checks-evidence.json) record provenance.
  Matched samples/differences
  are in [layers-parity/](layers-parity/). Examples:
  [logo fade](layers-parity/output-parity-d-0187-branch.jpg),
  [L-shape entry](layers-parity/output-parity-d-0370-branch.jpg),
  [settled L-shape](layers-parity/output-parity-d-0420-branch.jpg).
  [Main health](layers-parity-main-health.json), [branch health](layers-parity-branch-health.json),
  [main channel log](layers-parity-main-channel.txt) and
  [branch channel log](layers-parity-branch-channel.txt) contain no channel errors.
  The empty ticker region remains equal; a newly authored ticker has no
  historical 2.2.1 counterpart.
- [Fresh desktop pair](layers-desktop-evidence.json): 240 s each, same media,
  duration, Xvfb/PulseAudio/ALSA setup and command schedule. Main logged 2
  buffer warnings and 1 ALSA I/O error; branch logged 1 and 0. Neither logged
  a clip-sync offset in this run. These virtual-device counts vary; the earlier
  paired main sync failures remain a documented environment limitation, not
  proof of audio stability. [Main frame](layers-desktop-main.jpg),
  [branch frame](layers-desktop-branch.jpg), [main log](layers-desktop-main-server.txt),
  [branch log](layers-desktop-branch-server.txt). This is a CPU desktop check;
  GPU, physical outputs, long soak and hardware A/V synchronization remain untested.
- [Rollback after 00030](layers-rollback-evidence.json): the actual automatic
  pre-update v27 backup was copied byte-for-byte and opened with official
  `ghcr.io/luizstsantos/noar:2.2.1`; schema stayed at 00027, authenticated
  configuration was readable and `/health` reported on-air output. A v29
  backup can only return to a compatible branch image, not released 2.2.1.
  [Rollback procedure](../docker.md#restoring-before-graphics-conversion-00029)
  keeps the verified pre-update database and previous image together.

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

1. Validate external RSS/JSON providers and an actual published Google sheet on
   a network with working public DNS. The four approved implementation stages
   (free templates, shapes/clock, fonts and data sources) are recorded above.
2. Repeat the 76-response and raw/HLS legacy comparison for the final revision.
   Desktop audio, physical outputs and formal long-soak checks remain explicit
   limitations; they are not implied by the screenshots.
3. Later phases: rundowns 2.5, open graphics tracks in clips 2.6 and broader
   control/webhook/data integration 2.7. The newly approved sources/clock work
   advances that scope; it does not claim those entire phases complete.

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
