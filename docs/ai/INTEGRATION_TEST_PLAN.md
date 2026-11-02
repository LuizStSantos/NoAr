# NoAr Cross-Layer Integration & End-to-End Test Plan

**Goal:** provide evidence that Vue frontend, Rust backend, SQLite database, scheduler and GStreamer media output actually work together. This document is a plan, not an assertion that tests were executed. Use `/noar-gstreamer` for native references; require actual NoAr source transitions and receiver progress in addition to packet timing and connection counts.

## A. Environment setup and access control

- Use a disposable Docker/local staging instance; no production mount, live ingest source, on-air destination, production credentials, or commercial media.
- Confirm exact git commit and configuration. Keep outputs and logs private. Never place credentials in Git or screenshots.
- Create test-only administrator and restricted user when a safe test instance is ready. Document how the database will be destroyed after the run.
- Verify Playwright dependencies and browser locally before tests. In Claude Code Web, the hosted container might not have browser binaries; do not invent execution.

## B. Integration matrix

| ID | Layer / flow | Observed outcome | Evidence |
| --- | --- | --- | --- |
| INT-01 | Public `GET /api/setup` | Status 200; JSON contains boolean `required` | Test report / HTTP status |
| INT-02 | Login page and navigation | Login form present; password toggle responds; all core protected routes redirect anonymously | Playwright trace or report |
| INT-03 | Authentication | Test account can login, invalid requests rejected; role boundaries enforced | Scrubbed HTTP assertions |
| INT-04 | Vue ↔ Rust ↔ SQLite | Create or update safe test object, reload page, verify persisted result | Test ID and API/DB check |
| INT-05 | Scheduling | UI-created test event is reflected in API/database and executed on staging clock | Scheduler logs and observed state |
| INT-06 | Output | Synthetic video/audio emit playable HLS/RTMP/SRT (whichever is applicable), with probed timestamps | ffprobe or media diagnostics |
| INT-07 | Failure recovery | Simulated safe staging restart/connection interruption preserves intended state | Staging logs and timestamps |
| INT-08 | Authorization/security | Non-admin cannot update global controls, credentials absent from logs and reports | Negative authorization tests |

## C. Test tiers

**Smoke:** `INT-01` and `INT-02` are included as runnable `qa/e2e` Playwright tests. They do not need test credentials. These are **not sufficient** to certify full-system integration.

**Authenticated integration:** `INT-03`, `INT-04`, `INT-08` require a disposable test account and database. The QA agent should design, review and implement narrowly scoped tests based on the current API; do not guess selectors or credentials.

**Staging broadcast E2E:** `INT-05`, `INT-06`, `INT-07` need synthetic fixtures, observability and nonproduction output. Approve scope explicitly. Tests must check real emitted output, not just the frontend status.

## D. Evidence policy

Record per gate: command/tool, UTC timestamp, commit, status, exit code, sanitized report path, and observed result. `FAIL` takes precedence over a 3/5 council vote. If browser is absent, record `NOT_AVAILABLE`, not PASS. Commit no secrets, generated traces or media.

## E. Regression scheduling

Run a focused smoke on small frontend/API changes, affected authenticated integration flows for DB/auth changes, and a controlled full-system pass before releases affecting scheduler/engine, migrations, HLS, SCTE-35 or restart behavior. Avoid a full five-agent audit for small fixes unless requested.
