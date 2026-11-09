# Backend council — media-engine reviewer — Round B

Reviewer: `noar-media-engine-reviewer`, independent read-only backend council voter.
Source: `f925f608` plus current uncommitted fixes, compared with `origin/main` `dd4529e2` (2.2.1 product source). Round A was read before this peer challenge. No ballot is cast here.

## Review and resolution assessment

Read the backend peer digest/finding list, repository/reviewer instructions, GStreamer adapter and canonical reference, quality gates, current engine/app call sites and focused test bodies. Resolved bindings remain gstreamer-rs 0.24.5. The retained image build provenance states core/base/libav 1.26.11 over Debian plugins 1.26.2. This voter did not execute native inspection, compilation, media, fault, browser or benchmark scenarios.

| Finding | Peer challenge and current source assessment |
| --- | --- |
| B-001 | Original manual expansion concern is credible. Current validation bounds manual field/record inputs before resolution and measures aggregate expanded text before building rendered strings. Security reviewer owns adversarial boundary review. Source resolution looks appropriate; no allocation/OOM experiment was attempted. |
| B-002 | Queue registration/cancellation and generation checks are required, but source changes alone do not prove lifecycle publication atomicity. API/security Round B already report runtime-control invalidation and lifecycle check-to-publication concerns. Those residuals must be resolved and tested independently; this voter does not dismiss them. |
| B-003 | Initial database queries still propagate failures. Optional model preparation is isolated per template and retains last-good/healthy models. This is the appropriate distinction between unavailable optional artwork and a database fault. Actual startup/restart with missing artwork remains parent-owned. |
| B-004 | `graphic_spec_for_channel_controlled` now returns the primary index in the emitted spec, excluding video/empty images/unpainted boxes. The engine also retains `RenderedLayer.source_index` when a raster below two pixels is omitted, and role blending tests that source index rather than the compacted rendered index. The two filtering boundaries are covered. No residual index mismatch found in inspected initial/dynamic publication callers. |
| B-005 | Temporary/permanent preparation consults `manager.running_config()` for actual width/height instead of assuming a newly saved output size is already running. Source resolution is sound; pending-output-change runtime still needs evidence. |
| B-006 / M1 | `LogoOverlay::moved_to` now clones the prepared overlay and changes cached integer blend offsets. It no longer reconstructs `GraphicSpec` or calls `from_model`/image decoding. Existing prepared GIF frames, text strips, layer animations and epochs remain shared by Arc identity. The frame-thread caller still invokes `moved_to`, but the inspected body contains no native image I/O or rasterization. Source resolution addresses the original HIGH finding. |
| B-007 / M2 | Logo set/refresh and L-shape model refresh now snapshot Arc identities and controls under a short mutex, prepare replacement/clip pixels outside the mutex, and validate generation plus override identities before publishing. Visibility/placement is read again at publication. New synchronous-injection tests check `try_lock` and mutate controls during preparation, and older-model-clear tests reject stale publication. Source resolution addresses the original native-work-under-frame-lock defect. These tests are deterministic lock-availability checks, not measured packet-gap evidence. |
| B-008 / M3 | `RenderedGraphic::inherit_motion` now descends into the nested picture; `update_graphic` invokes it before publishing. The nested animated artwork preserves its prepared epoch rather than resetting on every dynamic refresh. A specific temporary-picture regression covers the previously omitted path. |
| B-009 / M4 | Logo refresh transfers motion from the prior definition, clip override and external replacement independently when the refresh is non-animated. Moving the prepared variant subsequently shares those epochs. A regression gives all three variants different epochs and verifies each independently. Source resolution looks complete for the reported refresh path. |
| B-010 | Partial schema-29/30 states are a conditional persistence concern; original public-operator reachability has not been established. Do not characterize the defensive recovery fix as proof of an actual deployed corruption case. Zero bindings plus zero scenes remains ambiguous and is explicitly disclosed. Database reviewer owns current conversion/marker coverage. |
| B-011 | Independent projection checks preserve untouched authored geometry and use previous output dimensions during configuration changes. This is preferable to reprojecting rounded compatibility DTOs unconditionally. Database reviewer owns transaction and unchanged-save fixture verification. |

No additional confirmed HIGH engine defect was found in this bounded source re-review. That statement does not clear unresolved API/security residuals or establish stream readiness.

## Processing-bench scope and interpretation

The added `graphics_blend` stage wraps four disjoint frame-time blend paths: temporary models, permanent models, role/logo layers, and animated L-shape artwork. `render_graphic` preparation does not invoke these timed blend entry points. Normal builds use the existing inline no-op wrapper and perform no added clock/TLS/counter operations.

Important measurement limits:

- There are multiple calls per output frame, including calls around empty temporary/permanent loops. The stage's average-call column is **not average graphics cost per output frame**. Use aggregate stage seconds divided by actual frames, or stage share, and document the call structure.
- Static L-shape background copying/paste and its native picture scaler are outside `graphics_blend`; they must not be represented as measured by this new stage. Whole-process CPU/RSS and end-to-end output progress are needed for the requested L-shape scenario.
- All model preparation/rasterization is outside this stage, including work on blocking worker threads without processing-bench TLS. CPU/RSS sampling and action-latency observations cover costs that stage totals omit.
- Main has no layered Graphics GC API. Unsupported 1/4/16 model-GC baseline cells should remain unavailable rather than silently backporting the renderer into main. Compare common zero-overlay/legacy-L-shape scenarios and branch incremental costs explicitly.
- Shared-machine debug-profile results are controlled comparisons, not production capacity certification. Avoid concurrent compiles/browser work during timed samples.

Focused benchmark tests inspect stage ordering and confirm preparation contributes zero calls while four explicit blend paths contribute four calls. They do not validate a 1080p25 performance claim.

## Live-loss comparison challenge

The current native input/preload/output/clock files remain unchanged against main. This supports using an identical fixture, but cannot establish that the 21.513-second audio-only UDP picture hold or the SRT-loss one-second pacing reset is preexisting. The last genuinely changing decoded video ROI must be independent of moving overlays. Preserve publisher-phase timestamps, receiver samples, source/teardown logs, output PTS and `/health` for both binaries. A pacing reset is a policy response to lateness, not a root-cause diagnosis.

Do not conflate the configured six-second video-loss threshold, HLS receiver buffering, native teardown time and last repeated-frame observation. Compare the same actual duration/media/retry settings and distinguish receiver buffering from output-segment content. If either path differs, investigate before classifying it as an environment or historical limitation.

## Evidence ledger

- Current source/diff and focused regression body inspection: **PASS (static)**.
- Existing image runtime provenance/Cargo lock inspection: **PASS (retained metadata only)**.
- Parent log `candidate-rust.log` currently records successful Rust suites including engine benchmark-feature tests; these were **not executed by this voter** and later source changes still need final reruns.
- New native stream/fault A/B, 1080p25 benchmark, 90-minute graphics soak, exact output comparison: **NOT_RUN by this voter / pending parent evidence**.
- Product edits, builds, servers, Git mutations: **NONE**.
- Round C ballot: **PENDING final findings and executed evidence**. No release/readiness approval is implied.
