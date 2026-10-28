# NoAr E2E Smoke Harness

This is an **optional, isolated** Playwright smoke harness. It does not modify NoAr's root `package.json`, Docker configuration, production data or GitHub workflows. It deliberately refuses to run without staging opt-in.

## Prerequisites

- Node/npm and a local Chromium browser installed by Playwright.
- A running disposable NoAr server, default URL `http://127.0.0.1:8787`, with its **own temporary** database. Never aim at your on-air NoAr instance.
- Explicit shell variable `NOAR_E2E_STAGING=1`. Only loopback hosts are allowed by default.

## Install inside `qa/e2e`

```sh
npm install
npx playwright install chromium
```

## Smoke run

```sh
NOAR_E2E_STAGING=1 NOAR_E2E_BASE_URL=http://127.0.0.1:8787 npx playwright test
```

The smoke suite reads `GET /api/setup`, verifies frontend loading and the login page's password visibility toggle, and verifies unauthenticated redirects for player, playlist, schedule, graphics, media, report and configuration routes. It performs no authenticated writes. No test passwords, account creation, database writes, scheduling or media output are performed.

## Other staging hosts

Only if you reviewed the URL and know it is disposable, you can set `NOAR_E2E_ALLOW_REMOTE=1`. This is an explicit override, **not a recommendation**. The script cannot prove an arbitrary hostname is not production. Never point it at a publicly accessible live service.

## Limitations

The `qa/e2e` smoke suite is a starting point, not complete coverage of buttons, API persistence, scheduler, or media engine. For full-system testing use the plan in `docs/ai/INTEGRATION_TEST_PLAN.md`, the non-voting `noar-integration-e2e-qa` agent, and safe synthetic fixtures.

Generated artifacts live under ignored `qa/e2e/test-results` and `qa/e2e/playwright-report`; do not commit traces, cookies, session storage, credentials or screenshots containing private data. If the environment lacks browsers, mark `NOT_AVAILABLE` and provide reproducible commands instead of claiming test success.
