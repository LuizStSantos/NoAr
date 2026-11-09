# Backend reliability review — round A

Reviewer: `noar-reliability-reviewer` (independently invoked backend voter). Reviewed revision: `f925f608` on `claude/branch-review-opz967`; comparison base: `origin/main` `dd4529e2935b07d7a5bf8deb764421e4d201fc41`. Date: 2026-11-09 UTC.

## Scope and evidence

STATIC REVIEW only. Read root/backend AGENTS, CLAUDE, reviewer definition, quality gates/checklist and repository map. Inspected changed app graphics orchestration, scheduler ownership, source-refresh cancellation, engine graphics publication/blending, related API tests, CI publication dependencies, DB startup backup/migration behavior and release guidance. No product edits, migration changes, server execution, compilation or stream workloads performed by this reviewer. Parent owns live-source A/B, 90-minute soak and processing-bench evidence. Those results are not asserted here.

## R-A-REL-01 — HIGH — queued manual takes survive deletion and take-all-out

Primary location: `backend/app/src/graphics.rs:231` (detached task through line 255).

The `DuringLShape::Wait` branch captures the template/spec/manager in an untracked `tokio::spawn`, waits up to one hour, then calls `draw_and_take` without checking cancellation, template existence or edit generation. The response returns only `Waiting`, with no queue identifier. `take_graphic_out` only calls the engine's current-take removal (`backend/app/src/api/routes/graphics.rs:359`–367), and template deletion only enumerates already-on-air takes (`:255`–265); neither can reach the queued future. Therefore: turn an L-shape on; queue a Wait template; delete that template or issue take-all-out while waiting; turn the L-shape off. The saved stale spec is still published on air. Repeated requests also create independent unbounded waiting tasks. Scheduled takes deliberately avoid this pattern (`backend/app/src/player/graphics_schedule/graphics.rs:254`–264).

Impact / blast radius: an affected channel can unexpectedly air removed or explicitly cleared material later, potentially for an unlimited take duration; queued tasks retain the channel manager across normal lifecycle changes.

Verification: `VERIFIED_STATIC` for missing ownership and deterministic control flow; `NOT_VERIFIED` runtime. Existing integration test `backend/app/tests/api/graphics.rs:356`–406 verifies only wait-then-show, and invokes out only after the queued take has aired.

Next check: in disposable API staging, queue an indefinite Wait template, call out and/or delete its template before clearing L-shape, then assert no graphic appears after 600 ms. Add restart/stop cancellation and bounded-queue cases. Remediation should assign cancellation ownership to queued takes and synchronize final publication with template edits.

## R-A-REL-02 — HIGH — one unavailable permanent graphic prevents channel startup

Primary location: `backend/app/src/player/output/playout.rs:325`.

`player` propagates `load_channel_ticker(...).await?` before `open_playout` (`:341`), and ticker loading also loads every unbound permanent template (`backend/app/src/graphics/channel/graphics.rs:353`, `:360`, `:404`). `load_permanent_templates` propagates each spec/render failure (`backend/app/src/graphics.rs:283`–298). A saved permanent image can subsequently be removed normally: storage removal (`backend/app/src/file/local.rs:366`–370) does not inspect graphics references, while `layer_image` rejects missing files (`backend/app/src/graphics/templates/graphics.rs:634`–637). Thus an otherwise valid channel with one permanent image model cannot open its output after that asset is removed and the channel restarts. Supervisor retries do not change the persisted broken reference. Logo and L-shape loading immediately above handle failures as warnings, whereas the new ticker/permanent path fails the playout.

Impact / blast radius: complete output outage for the affected channel after restart, caused by optional graphics artwork becoming unavailable; other channel managers are outside this demonstrated path.

Verification: `VERIFIED_STATIC` for error propagation and accessible asset-removal path; `NOT_VERIFIED` runtime. Test coverage checks rejection of missing artwork when creating a permanent template (`backend/app/tests/api/graphics.rs:701`) but does not cover deletion after successful creation followed by restart.

Next check: in disposable staging, create a valid permanent image composition, delete its image via Storage, restart, and observe output/health. Desired recovery should skip/retain failed optional artwork and expose the graphics error while program output continues. Test missing mapped image assets and corrupt image decode as companion faults.

## R-A-REL-03 — MEDIUM — temporary takes use saved dimensions while output awaits restart

Primary location: `backend/app/src/graphics.rs:165`–170.

`draw_and_take` renders temporary graphics using `manager.config.output` dimensions. Saving a resolution change immediately replaces that config while returning `requires_restart` to the caller (`backend/app/src/utils/config/update.rs:259`–285), so the old output can still be running. A temporary take in this interval is accepted and listed as on air, but engine blending skips it when frame dimensions differ (`backend/engine/src/compositor/graphics.rs:841`–842; picture window likewise `:788`–792). Permanent and dynamic graphics already resolve dimensions through `running_config`, demonstrating the required separation. A static temporary template is not repaired by dynamic refresh.

Impact / blast radius: temporary graphics disappear despite a successful take response on a channel with a pending resolution restart; this does not demonstrate a program-output failure.

Verification: `VERIFIED_STATIC`; runtime `NOT_VERIFIED`.

Next check: run a channel at 1920x1080, save 1280x720 without restarting, take a static box template, and compare API on-air status with captured output. Resolve temporary render dimensions from `running_config` consistently.

## Safeguards and limits

- CI validates architecture boundaries, tests, Clippy and formatting before the image/release jobs. Its Rust test/build path uses `--no-default-features`; this review did not execute CI or infer desktop/24-hour certification from it.
- Existing startup uses SQLite `VACUUM INTO` for pending-migration backups and keeps three copies. Backup errors are logged and migration proceeds; this is inherited behavior, not counted here as a new branch defect. A production rollback must restore the pre-upgrade database with its matching image. No migration was edited or renumbered, particularly 00029–00034.
- New refresh work uses four concurrent source tasks and explicit abort on cancellation, bounded HTTP/download size/time and parser work off the frame thread. Static review is not proof of outage recovery.
- Missing broadcast certifications already marked NOT_RUN are evidence limits, not additional findings.

Round B peer digest and final deduplicated issue list are pending. No advisory readiness or per-issue ballot cast in this round.

