# NoAr full backend audit — ref 29e69a2 (backend identical to adc2061)

Council: BACKEND (5 voters: backend, media-engine, security, database, reliability). Lead facilitated, non-voting; Round B relayed by lead (no direct agent messaging).
Tally: `tally_audit_votes.py --council backend` → readiness **BLOCKED** (deny 5/0/0); confirmed blockers F-001, F-002.

## Verification ledger
| Check | Status |
|---|---|
| cargo fmt --all -- --check | PASS |
| cargo clippy --no-default-features --all-targets -D warnings (CI set) | PASS |
| cargo test --no-default-features (CI set) | PASS (674 passed, 0 failed, 15 ignored) |
| cargo clippy/test --all-features | NOT_AVAILABLE (alsa-sys dev library missing in container) |
| Docker build, migrations on old DB, synthetic streams (HLS/SRT/RTMP/UDP, SCTE-35) | NOT_RUN |
| API 401/403 runtime matrix, restart/updater on staging | NOT_RUN |

## Confirmed (≥3 confirm)
- F-001 HIGH blocker — playlist `date` unvalidated → authenticated User path traversal write/delete of *.json; malformed date panics the request (utils/playlist.rs:43-65,148-156; import.rs:35; generator.rs:258). Lead-verified. Note: 2 of the 3 confirms (backend, database) cite the lead's verification rather than an independent re-read; the security reviewer verified independently.
- F-002 HIGH blocker — rate_limit outermost (main.rs:168-174): IP key spoofable via X-Forwarded-For or shared 127.0.0.1; authenticated bypass never fires. Same caveat as F-001 on independence.
- F-025 MEDIUM — critical-path test gaps (SCTE-35 integration, DST fall-back, restart, migration upgrade fixtures). Auth is partly covered.

## High-severity, UNRESOLVED (too many out-of-scope abstentions; none dismissed)
- F-003 (blocker as filed) panic in run_channel leaves channel silently off air (2 confirm).
- F-004 process_control busy flag racy / not cancellation-safe (2 confirm).
- F-005 mid-clip failure fills whole duration; F-006 single bad packet aborts clip/live (1 confirm each, engine owner).
- F-007 (blocker as filed) docker/release workflows publish without validate gate (1 confirm, reliability owner).
- F-008 auto-update restart without on-air guard; owner and database agree MEDIUM (opt-in).

## Notable votes / dissent
- F-012 dismissed by the engine reviewer (clip failures return Ok(Fallback), lib.rs:1018-1036); backend keeps it MEDIUM-unverified.
- F-032: security dismisses (User operating schedules is policy); backend confirms only the unbounded duplicate-trigger queue.
- Owners revised down: F-015 → LOW/uncertain, F-016 → LOW, F-023 → LOW.

## Independent quality-gate view (QUALITY_GATES.md)
Stop conditions met regardless of vote: confirmed security blockers F-001/F-002; unverified broadcast-continuity risks F-003/F-005/F-006; release pipeline F-007. Overall: **NOT_READY**.

## Suggested fix order
1. F-001 validate `date` with NaiveDate at all four sites (+ unit tests for traversal/absolute/short dates).
2. F-002 reorder layers so RealIp/Grants run before rate_limit; use ConnectInfo + trusted-proxy list.
3. F-003 supervise run_channel (catch panic → reset is_alive, restart with backoff, alarm).
4. F-007 gate docker.yml/release.yml on validate; test the published feature set.
5. F-004 compare_exchange + RAII guard, run command in a spawned task.
6. F-005/F-006 subtract elapsed time from fallback; tolerate InvalidData packets.
