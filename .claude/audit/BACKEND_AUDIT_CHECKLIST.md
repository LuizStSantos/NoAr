# NoAr full backend audit checklist

This is a **review runbook**, not proof that any check was performed. Scope the audit to a known commit SHA. Exclude production execution; use disposable data and synthetic feeds.

## A. Server and API (`backend/app`)

- Authentication and authorization on all sensitive REST, file, SSE and administrative endpoints; verify global/channel permissions across roles, including 401/403 regression coverage.
- Request validation, upload size/filename and path traversal, symlink/canonical-path safety, HTTP timeouts, caching and secret handling.
- Async task cancellation, bounded work queues, lock ordering, API error paths and restart/shutdown behavior.
- Scheduler: event conflict resolution, clip resumes, recurrence, timezone/DST, changing or removing a queued event; ensure real output matches persisted state.

## B. Media engine (`backend/engine`)

- GStreamer states, owned buffer/sample references, segments/running time and PTS/DTS conversion; thread safety, bounded queues and caps/parser/mux negotiation. GStreamer is the sole engine; no implicit FFmpeg fallback.
- File/folder/external live clients, preloading, seek/flush, decoder errors, reconnect/backoff, timeout, A/V synchronization and continuous output. NoAr must not start RTMP/SRT input listeners. Use `/noar-gstreamer` for native references and require decoded transitions plus wall-clock receiver progress.
- HLS, RTMP, SRT, UDP and SCTE-35 behavior under discontinuity, marker cancellation, ad splice timing and restart.
- Audio loudness/mixing, program-to-commercial transitions, logo blending and missing-media filler behavior.

## C. SQLite and schema (`migrations`, `backend/app/src/db`)

- Released migration immutability and checksums, non-destructive upgrades, backup/restore, optional rollback and transaction integrity.
- Schedule state consistency, crash recovery, concurrency and idempotent background operations.
- SQL parameter binding, foreign keys, deletion lifecycle and logging of database errors.

## D. Testing, build and deployment

- First inspect existing tests; identify code paths missing unit, API integration, migration and synthetic-stream tests.
- Check changed feature sets (and justification): `cargo fmt --all -- --check`, targeted `cargo test`, targeted `cargo clippy` and build. The full CI can be resource-intensive; never infer it ran without command results.
- Inspect GitHub Actions validation/publication ordering, pinned runtime image for a channel, startup health/restarts, updater token, docker-socket exposure and non-disruptive upgrade assumptions.
- Document test evidence as `PASS`, `FAIL`, `NOT_RUN`, or `NOT_AVAILABLE`. Static inspection never equals a verified live stream.

## E. Audit rules

1. Start with `docs/ai/repository-map.md`, refresh if stale; assign separate specialist areas and keep token budgets.
2. Extract findings with `file:line`, impact, severity, static/runtime evidence and concrete next test.
3. Five backend agents submit independent findings, exchange concise challenges and cast independent ballots.
4. Run `python3 .claude/scripts/tally_audit_votes.py --council backend ...` only with five **real** backend ballots. Abstentions are valid but fabricated ballots are not.
5. Evaluate `.claude/audit/QUALITY_GATES.md` independently from votes. Escalate critical security/data-loss/on-air risks even without majority approval.
6. The Documentation Guardian checks `README.md`, `docs/api.md`, `docs/developer.md`, `docs/docker.md`, `docs/releasing.md`, `CHANGELOG.md`, map and relevant subsystem docs for impact.
7. Report confirmed issues, inconclusive questions, unperformed tests, architecture trade-offs, prioritized fixes and minimal recommended regression coverage. Do not edit source during audit.
