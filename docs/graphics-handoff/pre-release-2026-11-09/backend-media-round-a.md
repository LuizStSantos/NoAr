# Backend council: noar-media-engine-reviewer — round A

Revision: `f925f608671c17c32f4788993b84d044a6621907`, compared with `origin/main` `dd4529e2935b07d7a5bf8deb764421e4d201fc41` (released v2.2.1). Read-only source review; no source/git changes, native execution, builds, benchmarks or media runs by this voter. No ballot cast; awaiting peer digest.

Read root AGENTS.md, CLAUDE.md, backend/AGENTS.md, reviewer definition, audit quality/integration gates, repository map, noar-gstreamer adapter/canonical skill and GStreamer reference map. Resolved bindings are gstreamer-rs 0.24.5 with v1_26. Parent supplied native core/base/libav 1.26.11 provenance; extracted runtime `/workspace/noar-cloud/fire-runtime-1.26.11/usr/local/share/noar/gstreamer-build.txt` agrees. Native properties were not independently inspected by this voter. Official AppSink reference fetch attempted, blocked by sandbox (`Operation not permitted`); source call behavior and explicit ten-second timeout suffice for findings below.

## M1 — Image-model movement now rasterizes/decode images on the playout frame thread

- Severity: HIGH. Status: VERIFIED_STATIC; receiver interruption magnitude NOT_VERIFIED.
- Primary location: `backend/engine/src/compositor/graphics/logo/graphics.rs:203`; caller `backend/engine/src/playout.rs:1340`.
- `apply_overlays` synchronously invokes `base.moved_to` when remote/clip placement changes or the configured model pointer changes. On main, `moved_to` only clones the prepared frame and changes x/y. Branch's model path rebuilds the full spec with `from_model -> render_graphic -> render_layer`; ordinary image layers call native `load_image` (`rendering/graphics.rs:214`), whose AppSink wait is up to ten seconds (`gstreamer.rs:196`). This puts file access, a GStreamer pipeline and text rasterization directly in the output-producing thread. Cached GIFs do not eliminate ordinary image decoding.
- Trigger: converted or authored image logo; issue remote/clip position change while playing. With a clock/data layer, each dynamic refresh changes the model pointer and makes the frame-thread movement rerender recur. It also rerenders when a replacement image is currently selected, because moved configured-base preparation precedes replacement selection.
- Blast radius: the channel's shared video/audio production and all its destinations, not just logo appearance. A ten-second timeout is a source upper-bound wait, not an observed stall.
- Targeted verification: synthetic clip, bound image+clock model, move the logo; compare output packet wall-clock gaps, frame completion latency, native image decoder creation and source transition progress against main. Exercise unreadable/stalled image preparation only in disposable staging.
- Repair direction: move prepared layers with coordinate offsets or publish prepared positioned variants off the frame thread; retain generation checking for dynamic edits.

## M2 — Model refresh performs native image/raster work while holding compositor-control mutexes

- Severity: HIGH. Status: VERIFIED_STATIC; receiver interruption magnitude NOT_VERIFIED.
- Primary location: `backend/engine/src/compositor/graphics/logo/graphics.rs:929`; matching L-shape path `backend/engine/src/compositor/graphics/lshape/graphics.rs:346`.
- Logo refresh takes the same mutex consumed by per-frame `logo_control.frame()` (`playout.rs:1311`) and then calls `replaced_image` at lines 946/956 and potentially `moved_to` at 959. Those calls rerasterize the full model and may synchronously decode native images. L-shape refresh likewise holds its per-frame `frame()` mutex across `render_model_art` at lines 363/371. Calling the refresh from `spawn_blocking` does not prevent the output thread from waiting on these mutexes.
- Trigger: bound logo or L-shape containing changing clock/data values while a clip/external image override is active; also manual edits refreshing those models. Dynamic graphics refresh checks every 250 ms (`backend/app/src/graphics.rs:328`) and republishes changed specs.
- Comparison: main's `LogoControl::set_configured` explicitly decodes outside its lock, and the old L-shape setter similarly prepares graphics before publishing. New model-aware override refresh introduces this work under frame-critical locks.
- Blast radius: channel output progress and API/control responsiveness. Distinct from M1: it can block output even when no remote position exists and the rasterizing worker is off-thread.
- Targeted verification: run dynamic clock/data refresh with active clip image or external image override; measure lock wait and packet progress. A deterministic synchronization test can pause preparation before publication and prove frame snapshots remain obtainable.
- Repair direction: snapshot override inputs, prepare variants outside the mutex, then publish only if the control generation is still current.

## M3 — Dynamic temporary L-shape refresh restarts scrolling artwork's motion epoch

- Severity: MEDIUM. Status: VERIFIED_STATIC; decoded visual effect NOT_VERIFIED.
- Primary location: `backend/engine/src/compositor/graphics.rs:711`.
- For ordinary takes, scroll elapsed time uses preserved `taken_at`. Picture/L-shape takes instead embed their animated layers inside `LShapeGraphic` (`rendering/graphics.rs:335`); `apply_lshape` invokes `blend_steady`, which uses fresh `layer.prepared_at.elapsed()` (`graphics.rs:243-250`). Dynamic temporary refresh calls `update_graphic` (`backend/app/src/graphics.rs:498`), which replaces the entire graphic and never inherits the nested picture's motion epoch.
- Trigger: temporary L-shape template containing scrolling text and a changing clock/data layer. Every changed clock value rerasterizes it, restarting scroll from outside the viewport. Slow scroll may repeatedly show only its initial fragment. Channel L-shape refresh already calls `inherit_motion` (`lshape/graphics.rs:350-354`), confirming the missing lifecycle operation in the temporary path.
- Blast radius: temporary picture-template scrolling artwork only; take duration and picture entry/exit clocks remain preserved.
- Existing test gap: `scrolling_refresh_preserves_motion_phase_for_temporary_and_permanent_takes` at `graphics.rs:922` uses `picture: None` and never exercises the nested artwork epoch.
- Targeted verification: create picture+scroll spec, set original prepared epoch several seconds in the past, update the take, assert nested epoch/pixels remain continuous; then observe an actual temporary L-shape with clock and scroll for multiple refreshes.

## M4 — Bound logo dynamic refresh also restarts scrolling artwork

- Severity: MEDIUM. Status: VERIFIED_STATIC; decoded visual effect NOT_VERIFIED.
- Primary location: `backend/engine/src/compositor/graphics/logo/graphics.rs:925` (publication at 966-967).
- `refresh_model` preserves visibility/fade state but builds and publishes a new LogoOverlay without transferring the previous model's layer `prepared_at`. `blend_role` uses `layer.prepared_at.elapsed()` (`graphics.rs:326`) for scroll. Thus clock/data refresh of a bound logo composition resets any scrolling text within that same model. Preserving `switched_at` does not preserve scroll timing.
- Trigger: logo model with its controlled image plus scrolling text and clock/data layer. With remote positioning, M1's rerender causes additional epoch resets.
- Blast radius: scrolling text authored within the bound logo model; permanent generic models and ticker use preserved take clocks and are not implicated.
- Targeted verification: refresh a bound logo with scroll and changing clock, assert prepared epoch is inherited and decoded text advances across repeated refreshes. Existing dynamic-role test checks fade/hidden state, not scrolling motion.
- Repair direction: carry motion epochs into newly prepared logo model and positioned/override variants independently of fades.

## Native-source and timing scope conclusions

- `backend/engine/src/input/gstreamer.rs`, `input/preload.rs`, `output/gstreamer_output.rs`, and `utils/clock.rs` have no diff against main. Branch changes in playout are graphics composition and logo clip-transition selection; its live fault thresholds are shared.
- `LIVE_VIDEO_LOST` is six seconds (`playout.rs:40`, checked at 562). A continuing audio track can cause `synchronize_live_tracks` to repeat the held composited image. Loss of all media uses a two-second threshold (`playout.rs:587`). The intentional hold/grace design does not establish seamless recovery and does not itself explain a measured 21.513-second hold. Main/branch receiver A/B and logs must distinguish last genuinely advancing video, decoded-but-repeated input, padding, native teardown/open latency and fallback execution.
- `utils/clock.rs:12` has a shared one-second catch-up limit; lines 93-100 reanchor pacing after a larger lateness. The warning is evidence of lateness plus this policy, not evidence of a new branch regression or its root cause. Collect preceding source-open/teardown and graphics-operation times. Do not change this policy merely to eliminate the warning.
- Prepared graphics blending snapshots Arc handles under `GraphicsControl`'s mutex and releases it before pixel blending. No raw GStreamer sample ownership regression or PTS mutation introduced in these paths was found. L-shape replacement copies source frame PTS (`lshape/graphics.rs:758`). Extra graphics wall-clock work can still affect throughput while timestamps remain monotonic.
- Runtime media evidence, performance numbers, soak and all proposed regression tests: NOT_RUN by this voter, owned by parent/integration work. No claim of seamless live recovery, measured performance regression, or readiness follows from this source review.
