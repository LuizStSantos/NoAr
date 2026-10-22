# Frontend-to-Backend Integration Quality Gates

These checks apply **after** the frontend/backend councils report. The separate `noar-integration-e2e-qa` agent owns observable test evidence and never votes. Keep statuses `PASS`, `FAIL`, `NOT_RUN`, `NOT_AVAILABLE`.

| Gate | Meaning | Required for a full integration sign-off? |
| --- | --- | --- |
| `public_api` | `GET /api/setup` returns the documented JSON shape in staging | Yes |
| `browser_navigation` | Login route, unauthenticated redirect, controls and basic accessibility interaction | Yes |
| `authentication` | Staging-only credential/login, permissions and protected API request | Yes |
| `api_database` | Safe create/read/update/restart persistence tests using disposable SQLite | Yes |
| `scheduler` | Program/schedule submitted in UI appears in API/DB, then executes on staging timeline | Yes |
| `media_output` | Synthetic clip/live output seen and probed on HLS or other selected output, with A/V continuity | Yes |
| `recovery` | Controlled staging restart/recovery with verified expected behavior | Yes |
| `security` | Non-admin and unauthenticated requests correctly rejected; no secrets leaked | Yes |

Not every change requires every gate: focused tests can cover only impacted gates. **Full-system sign-off requires all eight gates PASS with actual evidence.** Static analysis may suggest a test but cannot change a NOT_RUN to PASS.

## Required manifest

Use the JSON schema in `.claude/audit/templates/integration-evidence.example.json`. Each gate has `status`, `reason`, and `evidence` references. Scripts are deterministic; they validate evidence fields and aggregate status but cannot authenticate whether a human/browser truly ran a test. Do not invent evidence.

Command:

```bash
python3 .claude/scripts/check_integration_readiness.py --evidence path/to/integration-evidence.json
```

- Any `FAIL` -> `NOT_READY`.
- Missing/unknown gates or `NOT_RUN`/`NOT_AVAILABLE` -> `NEEDS_TESTING`.
- All eight `PASS` with nonempty evidence -> `READY_FOR_REVIEW` (advisory only).

## Never run on air

Production playout, live SRT/RTMP endpoints, real accounts, media archives, and persistent production databases are strictly out of scope. Staging media is synthetic, URLs are local by default, and updates/restarts require explicit staging authorization. A `3/5` vote does not override failures or risk.
