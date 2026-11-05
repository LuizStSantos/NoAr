# Revalidation evidence — 2026-11-05

## Tested scope and provenance

Baseline: `d4a1a0a8a41f39b90de23844910998431fd42510`, NoAr 2.2.1.
Local changes are limited to the E2E harness, its dependency lock/type config,
ignore rules and audit documentation. No production Rust/Vue behavior changed.

The server was built from that baseline with `cargo build --locked -p noar
--no-default-features`, using Rust 1.99.1, `CARGO_INCREMENTAL=0` and
`CARGO_PROFILE_DEV_DEBUG=0`. Node: 24.19.0; Playwright: 1.64.1; Chromium: 154.0.8037.57. Frontend: current
checkout served by Vite. Locale: English, browser timezone America/Sao_Paulo.
The native GStreamer libraries are Debian 1.26.2, **not** the 1.26.11 core/base/libav
shipped in the published Docker image. No stream or media-runtime equivalence
is claimed. The initial Docker attempt lacked native development libraries;
a larger development image exceeded disk capacity and was discarded. The
successful run used local libraries and extracted development metadata under
a temporary workspace folder. The Docker runner branch is NOT_RUN.

## Commands and results

| Check | Result | Evidence |
|---|---|---|
| Current backend build, no default features | PASS | Native executable compiled from the baseline; used by the browser run |
| `python3 qa/e2e/run_staging.py --native --binary target/debug/noar --chromium /usr/bin/chromium` | PASS | [15 Chromium tests](browser-run.txt), [restart comparison](restart-result.json) |
| `npm run type-check --prefix qa/e2e` | PASS | Strict independent TypeScript check |
| `python3 -m py_compile qa/e2e/run_staging.py` | PASS | Runner syntax check |
| `python3 .claude/scripts/verify_changes.py --files ... --run` | PASS | [Verification receipt](verification.json), toolkit unit-test command exited 0 |
| `git diff --check` | PASS | No whitespace errors |
| New frontend build / Rust unit tests / Clippy | NOT_RUN | Production frontend/Rust source did not change; prior CI is not a new local test run |
| Independent five-reviewer councils | NOT_RUN | No renewed ballots or votes were created |
| Full integration / broadcast readiness | NEEDS_TESTING | [Integration gate manifest](integration-evidence.json) |

The first development run failed because the new harness used an ambiguous
In/Out locator, assumed the edit submit button was named Save rather than
Schedule, assumed additions were disabled after a load error, and performed
more logins than the real limiter allows. The selectors/assertions were corrected,
and setup now signs in once and reuses the disposable session. The final run
passed all 15 tests without retries, plus the backend restart comparison.

## Actual behavioral evidence

- Browser UI creates a playlist, edits title/in/out, undoes/redoes the edit,
  saves and reloads it. An authenticated API read verifies the saved fields.
- A simulated HTTP 503 on saving retains the draft; removing the intercept
  permits a real successful save.
- A simulated HTTP 503 on loading leaves Save disabled even after adding a
  new draft item. The actual stored playlist remains unchanged. This covers
  frontend F-001 within that specific failure scenario.
- Scheduler UI creates, edits and deletes an event, changes daily recurrence,
  and verifies the next-day occurrence and saved data through the real API.
- ScheduleDialog takes focus, wraps Tab/Shift+Tab, closes on Escape and returns
  focus to its opener. This covers frontend F-003 for ScheduleDialog only.
- Another event and both saved playlists are read through the authenticated
  API, the runner stops and restarts its own backend, and all API responses
  match the pre-restart snapshots. This verifies actual retained SQLite and
  playlist storage, beyond browser memory or a page reload.

All successful CRUD responses came from the real server, database and filesystem.
Only the two named 503 failures were intercepted. No channel was started.
The runner removed its database, media and session file after stopping its
processes. The checked-in receipts contain no operator password or bearer token.

## Remaining limits

There was no scheduled on-air execution, decoded output continuity check,
GPU/hardware test, NDI/SDI/HLS output test, complete role matrix, legacy-schema
upgrade fixture, multi-tab refresh failure simulation or Firefox/WebKit run.
A source correction is recorded as FIX_PRESENT, not automatically as a runtime
closure. Twenty source corrections do not mean twenty findings were dynamically
verified. Original severities and council verdicts remain historical.

Documentation impact: **UPDATED** by the primary reviewer (`qa/e2e/README.md`,
`docs/developer.md`, the audit index and historical-report notices). No independent
Documentation Guardian was run. Generated source maps were reviewed as navigation
artifacts and left unchanged; no production symbols changed.
