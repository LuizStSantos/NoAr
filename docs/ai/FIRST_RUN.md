# First Run: NoAr Intelligent Development v4

> Read this after `INSTALL.md`. All commands below are examples **for an isolated setup branch**. No action should touch live channels, real credentials or production data.

## Phase 1 — Verify the project skill is available

1. Open `https://claude.ai/code`, select `LuizStSantos/NoAr` and the branch containing this package, and start a fresh session.
2. Invoke `/noar-intelligent-development first-run` (or paste the fallback prompt in `INSTALL.md` if the slash command is not listed).
3. Verify the expected files in `.claude/skills/`, `.claude/agents/` and `.claude/scripts/`. Read the short `CLAUDE.md` and preserve existing repository instructions.

## Phase 2 — Build the persistent map and test toolkit

From repository root, if Bash/Python are available:

```sh
python3 .claude/scripts/update_repository_map.py
python3 .claude/scripts/update_repository_map.py --find scheduler
python3 -m unittest discover -s .claude/scripts -p 'test_*.py'
```

Expected: `docs/ai/repository-map.md` and `docs/ai/symbol-index.json` created; unit tests report success. These tests validate the helper scripts, **not NoAr runtime behavior**. Keep generated map under version control on your setup branch only if appropriate.

## Phase 3 — Static review of both sides

In separate requests (budget friendly):

```text
/noar-intelligent-development audit full frontend
/noar-intelligent-development audit full backend
```

Each council has five voting reviewers. Capture findings and ballot evidence. No PASS claim for clicked buttons or streams from static analysis. The orchestrator produces two independent review summaries and the Documentation Guardian flags affected docs.

## Phase 4 — Readiness preflight (zero runtime side effects)

```sh
python3 .claude/scripts/check_integration_readiness.py --evidence .claude/audit/templates/integration-evidence.example.json
```

The sample contains NOT_RUN gates and **returns exit code 2 deliberately**. Expected output is `NEEDS_TESTING`; it is not an installation error. Do not change sample statuses to PASS without observed evidence.

## Phase 5 — Browser smoke on disposable staging only

Only with your approval and an already running disposable NoAr instance on `127.0.0.1:8787`. This step may require local/SSH environment if your Claude Code Web cloud environment lacks browser/tooling:

```sh
cd qa/e2e
npm install
npx playwright install chromium
NOAR_E2E_STAGING=1 NOAR_E2E_BASE_URL=http://127.0.0.1:8787 npx playwright test
```

This smoke verifies a public backend endpoint, routing and the login form, **not** authenticated workflow persistence or media playout.

## Phase 6 — Authenticated and media integration (with explicit staging authorization)

After reviewing `docs/ai/INTEGRATION_TEST_PLAN.md`, invoke:

```text
/noar-intelligent-development test integration
/noar-intelligent-development test full-system
```

Provide a controlled staging instance, disposable credentials via safe mechanism (not chat/commits), synthetic media and permission to run the specified checks. The QA agent records real results and unresolved gates. Stop on failures or unavailable resources.

## Phase 7 — Complete the documentation handoff

```text
/noar-intelligent-development docs-impact
/noar-intelligent-development finalize
```

Orchestrator documents checks and blockers; Guardian flags README, API, Docker, release notes and maps to update. **No automatic merge, tag, deploy or modification of production.**

## Minimal first-run request to paste

> Run `/noar-intelligent-development first-run` for `LuizStSantos/NoAr` on this setup branch. Validate the toolkit, generate the map, run only the toolkit's Python unit tests and report prerequisites for staging integration. Do not start Docker, install browser dependencies, run on-air tests, change product code, push or merge anything. Label every unperformed runtime check NOT_RUN or NOT_AVAILABLE.
