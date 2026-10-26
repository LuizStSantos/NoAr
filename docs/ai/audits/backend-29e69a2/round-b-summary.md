# Backend Round B relay summary (lead-relayed; no direct agent-to-agent messaging occurred)

IDs F-001..F-035 unchanged; vote on each as written. Lead is non-voting.

## Lead-verified facts (static)
- F-001: write_playlist/delete_playlist join unvalidated `date` parts (backend/app/src/utils/playlist.rs:43-65,148-156); save_playlist allows Role::User (api/routes/playlist.rs:107-111); read_playlist alone validates with NaiveDate.
- F-002: `.layer(rate_limit)` is added last, so it is outermost and runs before RealIpLayer and GrantsLayer (main.rs:168-174).
- F-004: is_processing uses load (control.rs:191) then store (:197); cleared only after the awaited command (:203-205).
- Test evidence: `cargo fmt --all -- --check` PASS. `--all-features` clippy/test could not build here (alsa-sys missing: NOT_AVAILABLE). CI-equivalent `--no-default-features` clippy/test are still running after installing the CI's FFmpeg dev packages; results go into the final report, not into ballots.

## Peer arguments
- F-001, F-002: security supports HIGH+blocker; F-002 also defeats F-017's protection. Fix must cover all four date-join sites (playlist.rs x2, import.rs:35, generator.rs:258).
- F-003: reliability and backend support HIGH+blocker (no catch_unwind; is_alive stays true; JoinHandle never observed).
- F-004: backend: cancellation half is the stronger part; keep HIGH, blocker conditional on verifying handler drop. reliability: reconsider blocker if drop confirmed.
- F-005, F-006: engine keeps HIGH non-blocker.
- F-007: reliability keeps blocker for release readiness only.
- F-008: owner (reliability) revises HIGH -> MEDIUM (auto_update defaults to 0).
- F-009: backend and database support MEDIUM (DB update at schedule.rs:204 committed before ? returns skip invalidate()).
- F-010: database supports, suggests LOW-MEDIUM (grace window limits damage).
- F-012: engine reviewer: unreadable clips become Ok(ClipResult::Fallback) (engine/src/lib.rs:1018-1036); Err only for PlaybackRestart, output write failure or failed fallback (lib.rs:1041-1052), where restart is correct. Suggests LOW, non-blocker. backend: unproven, keep MEDIUM-unverified.
- F-015: owner revises to LOW/uncertain (wedge needs a syscall ignoring the interrupt callback; not proven).
- F-016: owner revises to LOW (fails only on corrupt TS/PID collision; out message carries a duration, scte35.rs:464-469).
- F-017, F-018, F-019, F-034: security supports as filed.
- F-022: database: only read-before-write deferred tx; channel creation is rare; MEDIUM non-blocker.
- F-023: owner revises MEDIUM -> LOW (explicit admin action; product decision).
- F-024: database links to F-008: snapshot before migrating should be a release gate.
- F-025: reliability: auth is partly covered (16 assertions in auth.rs, ~30 401/403 refs in tests/); other gaps stand.
- F-027: citation docker-compose.yml:1 is a header comment; the watchtower image line is lower. F-030: axum::serve is at main.rs:192 in this ref. Findings stand.
- F-032: security and backend: User start/stop/restart is deliberate (control.rs:177-180), so User access is policy; only the unbounded duplicate trigger queue is a defect. Keep LOW.
- F-035: security keeps LOW (first run only).
