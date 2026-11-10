# NoAr isolated browser tests

This is an **optional, isolated** Playwright harness. It does not modify NoAr's root `package.json`, Docker configuration, production data or GitHub workflows. It deliberately refuses to run without staging opt-in. The default smoke suite is read-only; authenticated writes require the disposable runner below.

## Prerequisites

- Node/npm and a local Chromium browser installed by Playwright.
- A running disposable NoAr server, default URL `http://127.0.0.1:8787`, with its **own temporary** database. Never aim at your on-air NoAr instance.
- Explicit shell variable `NOAR_E2E_STAGING=1`. Only loopback hosts are allowed by default.

## Install inside `qa/e2e`

```sh
npm ci
npx playwright install chromium
```

## Smoke run

```sh
NOAR_E2E_STAGING=1 NOAR_E2E_BASE_URL=http://127.0.0.1:8787 npx playwright test
```

The smoke suite reads `GET /api/setup`, verifies frontend loading and the login page's password visibility toggle, and verifies unauthenticated redirects for player, playlist, schedule, graphics, media, report and configuration routes. It performs no authenticated writes. No test passwords, account creation, database writes, scheduling or media output are performed.

## Authenticated editing and persistence

From the repository root, install the frontend dependencies with `npm ci` and
the harness dependencies with `npm ci --prefix qa/e2e`. Use a current Linux
backend binary with its native runtime libraries, or build a local NoAr image.
The frontend always comes from the current checkout.

```sh
# Native backend: build with the project's required GStreamer development libraries.
cargo build --locked -p noar --no-default-features
python3 qa/e2e/run_staging.py --native --binary target/debug/noar

# Alternatively, build and run the current source in Docker.
docker build -t noar-e2e:local .
python3 qa/e2e/run_staging.py --image noar-e2e:local

# An existing Chromium can be selected without downloading a browser.
python3 qa/e2e/run_staging.py --native --binary target/debug/noar --chromium /usr/bin/chromium
```

The runner requires free loopback ports 8787 and 5757. It creates its own empty
SQLite database, media and playlist folders under `work/e2e-*`, copies a synthetic
clip, starts the backend and Vite, and completes first-run setup with random
operator credentials. It signs in once through the browser and shares only that
disposable session across authenticated tests, respecting the login rate limit.
Anonymous tests retain separate, empty sessions.

The full suite includes:

- Add a clip, edit title/in/out, undo/redo, save and reload; verify saved fields
  through the authenticated API.
- Inject an HTTP 503 on save, retain the draft and retry successfully.
- Inject an HTTP 503 on playlist loading, attempt a new draft edit and verify
  saving remains disabled and the real saved day stays unchanged.
- Create, edit and delete an event, change it to daily recurrence and verify
  next-day occurrences through the real API and after full page reloads.
- Move, trap and restore keyboard focus in ScheduleDialog; close with Escape.
- Create/cancel/rename/move/delete Storage folders, including bookmarked-folder
  reload, and download uploaded MP4/PNG/WebVTT bytes from desktop and phone.
- Fail a report read, clear stale exportable data and recover on day navigation.
- Exercise Graphics model/Composer editing, fonts, sources and compatibility
  navigation through the real backend.
- Create another event, then let the runner restart the backend and compare
  both saved playlists and the scheduled events through the authenticated API.

Successful persistence requests use the real backend and SQLite/filesystem;
only the two explicitly described 503 failure scenarios are intercepted.
The channel is never started. The runner stops only its own processes/container
and removes its database, media and session file on exit. Logs and a successful
`restart-result.json` remain in the printed `work/e2e-*` folder.

The write suite rejects initialized servers, remote hosts and storage paths
that differ from the runner-owned directory. Do not enable `NOAR_E2E_WRITES`
manually against an existing instance. The runner is the supported entry point.

Run `npm run type-check --prefix qa/e2e` to verify the harness independently.

## Other staging hosts

Only if you reviewed the URL and know it is disposable, you can set `NOAR_E2E_ALLOW_REMOTE=1`. This is an explicit override, **not a recommendation**. The script cannot prove an arbitrary hostname is not production. Never point it at a publicly accessible live service.

## Limitations

These tests cover operator CRUD and restart persistence, not automatic execution
of scheduled events, decoded media output, stream continuity, GPU encoders,
multi-channel permissions, every modal, or all browsers. Only Chromium is
configured. Full-system sign-off still requires the separate gates in
`docs/ai/INTEGRATION_TEST_PLAN.md` and actual synthetic-media output evidence.
The Docker runner path must be verified in an environment with enough space;
the recorded 2.2.1 run used the native runner.

Generated artifacts live under ignored `qa/e2e/test-results` and `qa/e2e/playwright-report`; do not commit traces, cookies, session storage, credentials or screenshots containing private data. If the environment lacks browsers, mark `NOT_AVAILABLE` and provide reproducible commands instead of claiming test success.

Failure traces can contain disposable session tokens; keep them local. The
random credentials and live session state are not part of committed evidence.
