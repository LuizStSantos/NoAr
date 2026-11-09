# Backend reliability reviewer — Round B peer challenge

Reviewer identity: `noar-reliability-reviewer`, independent BACKEND council voter.
Repository: `LuizStSantos/NoAr`; original review `f925f608`, current fixes in the shared worktree on `claude/branch-review-opz967`; base `dd4529e2` (2.2.1 product source).

## Scope and verification

Read repository/backend instructions, reviewer definition, quality gates, the prior independent reliability report, the deduplicated backend peer digest, current graphics lifecycle/publication and permanent-loader code, API regression fixtures, the native/graphics soak adapter, picture observer and CI verification path. This is **VERIFIED_STATIC**, not an executed Rust, browser, stream, rollback or soak result. No product edits, migrations, builds, servers or Git mutations were performed. No Round C ballot is cast here.

The implementation helper is actively addressing publication gaps identified below. Source can therefore be intermediate. Final tests must use the completed binary and identify its hash.

## Challenges to the original findings and proposed fixes

- **B-002 — pending takes:** The original detached, unbounded waiter was a concrete ownership defect. The current queue has a 16-command admission cap, RAII removal, cancellation tokens, edit/output generations and a final checked-publication helper. API tests cover out-all, edit, delete, lifecycle drop/restart and capacity. These tests exercise real router/service/database behavior in-process, but do not by themselves prove an actual output restart. Runtime visibility operations must use the non-invalidating publication lock: cancelling a waiter merely because an operator turns the L-shape off defeats Wait. The new HTTP `waiting_takes_survive_runtime_controls_and_scheduler_wait_polling` fixture targets that distinction, unlike the original direct engine-control test. Parent must execute it after the helper finishes.
- **B-003 — optional permanent failure:** `load_permanent_templates` now reads bindings/templates with `?`, then isolates each model's spec/render/publication error and preserves the failed model's prior persistent slot. An unavailable model does not prevent another healthy permanent model from loading. The added `unavailable_permanent_artwork_does_not_block_healthy_models_or_hide_database_faults` fixture separately checks last-frame retention, fresh-output skip, healthy model survival and actual closed-pool error propagation. This is stronger than merely swallowing every error at channel startup. Native decode corruption and real restart evidence remain separate checks.
- **B-005 — unapplied dimensions:** Temporary render dimensions now prefer the manager's running configuration, consistently with permanent/dynamic paths. This addresses the demonstrated saved-config versus active-output mismatch. An actual resolution-save-without-restart take/output capture remains useful evidence; source review alone does not establish visible output.
- **B-001:** Manual count/value/record limits and aggregate expansion budget are now checked before expanded strings are built. The bounded arithmetic regression avoids deliberately allocating a denial-of-service payload. Security owns the detailed trust-boundary assessment.
- **B-004/B-006/B-007/B-008/B-009:** Source fixes make filtered target indices explicit, avoid native work on logo movement/frame-critical control locks and retain prepared motion identity. Focused renderer tests target the intended invariants. Native output timing and performance remain unverified by this reviewer; a static no-lock trace does not prove the absence of stalls.
- **B-010:** Keep the conditional qualification. A partial pre-31 state is a concrete defensive schema fixture, but operator reachability was not demonstrated; zero surviving bindings/scenes remains ambiguous. It should not be described as a reproduced production migration failure.
- **B-011:** Preserving percentage geometry when the incoming legacy projection is unchanged is the appropriate compatibility contract. Separate position/scale tests and previous-output-dimension propagation matter; a full no-op save must not quantize Composer geometry.

## Residual publication gaps raised during Round B

### REL-B1 — MEDIUM — old dynamic role refresh can publish across a lifecycle change

`backend/app/src/graphics.rs`, `refresh_dynamic_graphics`, takes the async editing lock and checks `version`, then awaits native work in role branches (`refresh_logo`, `set_template_controlled`, or ticker rasterization/publication). Lifecycle invalidation is synchronous and does not wait for that editing lock. A version change during the awaited work can therefore occur after the application check but before shared-control publication. Engine generations reject superseded work only after that engine update has started; if the old worker begins after a new run reload, it can become the newest engine update itself.

This is an evidenced static interleaving, **not runtime-reproduced**. Normal cooperative teardown usually joins dynamic work before restart, but timeout/drop teardown is precisely why the lifecycle guard exists. The manual-take `publish_if_current` gate does not alone cover dynamic role publication. A final generation-aware commit after preparation, plus a deterministic delayed-worker regression, is the requested check. Relayed directly to the parent and lifecycle implementation helper; fix in progress.

### REL-B2 — MEDIUM — startup permanent loading can race an authored reload

`backend/app/src/player/output/playout.rs` calls `load_channel_ticker`/`load_permanent_templates` during startup without the API editing/publication lock. The loader snapshots saved models, awaits native rendering, then calls `set_persistent` and `retain_persistent`. An API delete/edit can complete its own serialized reload in between, after which the startup snapshot can publish removed/stale model pixels and retain their IDs until another reload.

This is **VERIFIED_STATIC** for the unguarded caller and awaited publication sequence; no runtime failure is claimed. Serialize the startup load with authoring, or validate its captured generation at commit. A blocked-render startup-versus-delete/edit regression should assert that removed or edited content cannot win late. Relayed directly to the parent and helper; fix in progress.

Neither residual is evidence of program-frame loss or a proven production outage. They must nonetheless be resolved or explicitly gated before claiming completed graphics lifecycle validation.

## Soak and observability review

The optional native adapter keeps the explicit opt-in, a new outside-repository work directory, overridden disposable DB/setup paths and owned process groups; it does not discover or terminate existing services. Default Docker behavior remains separate. The duration gate rejects early/interrupted runs, and process/observer cleanup failures become incidents. A fixed video-only ROI lies inside both full-frame and the authored top-left 75% L-shape video, while logo, lower third and ticker are outside it; animated overlays therefore cannot by themselves hide frozen programme video in that observer.

Important evidence limits:

- The existing fault allowances suppress picture failures during bounded SRT loss (40 s), UDP audio-only loss (50 s), startup and restart. Report a pass as recovery **outside these declared allowances**, not as proof that no picture was held during injected loss.
- Graphics receipts and on-air state checks prove API admission/state. They do not assert artwork pixels or ticker movement. Actual HLS frame captures/region checks should support any visible-composition claim.
- The 90-minute duration, final binary hash, graphics step/receipt counts, ROI checks, transport progress, `/health` and channel log must come from the completed actual run. Neither mocked harness tests nor historical fire evidence establishes this.
- CI uses two Rust test threads to avoid starving native deadline tests; it validates the server's no-default-feature configuration, not desktop or 24-hour operation. This review did not execute CI.

## Handoff

No new confirmed HIGH in this scoped Round B review. REL-B1/REL-B2 remain pending implementation/re-review and executed regressions at this report's snapshot. Overall runtime/release gate is **NOT_READY** while the final Rust checks, live-loss A/B, processing measurements, browser proof and actual 90-minute soak are pending. This is not an advisory ballot or release authorization.
