# NoAr Intelligent Development v4 — Installation & First Run

**English-only package** for the `LuizStSantos/NoAr` repository. This is an additive upgrade from v3. It contains one project skill, 16 agents (10 voting roles across 2 separate councils, 1 reviewer shared, 4 helpers, Orchestrator, Documentation Guardian, and the new independent QA agent), three deterministic toolkit scripts, and an isolated Playwright smoke harness. Do not count the integration agent as a voter.

## 1. Install from a ZIP into GitHub

1. Download and extract `NoAr-Intelligent-Development-v4-Repository-English.zip` on your computer. The `.claude/` directory must keep its leading dot.
2. Open `https://github.dev/LuizStSantos/NoAr` or clone the repository locally. Create a new branch e.g. `chore/noar-ai-workflow-v4`.
3. Copy `.claude/`, `docs/ai/`, `qa/e2e/`, `INSTALL.md` and `CLAUDE.md` into the repository root (not into a nested `NoAr-Intelligent-Development-v4/` folder). Allow replacement of **our previous v3 files only**. Merge an existing `CLAUDE.md` rather than overwriting unrelated project instructions. Keep `backend/AGENTS.md` unchanged.
4. Review changes and open a Pull Request. Do not enable workflows, update the application, deploy, or change releases as part of this setup.
5. Open `https://claude.ai/code` with the branch that contains the files. Start a new Code session.

If GitHub's browser Upload files UI hides `.claude`, use `github.dev` (VS Code web editor), create `.claude/agents/` and `.claude/skills/` paths there, and copy the files. The directory is valid in Git; it is not a forbidden filename.

## 2. Project skill vs. account skill

The repository contains `.claude/skills/noar-intelligent-development/SKILL.md`, which provides `/noar-intelligent-development`. **Normally use the repository skill for Claude Code Web.** If the cloud session does not load project skills, use the separate account ZIP in `https://claude.ai/customize/skills` (Upload a skill). The account skill still expects repository agents/scripts. Avoid enabling duplicate personal and project copies simultaneously unless your environment requires it.

## 3. First-run commands (without touching production)

**First:**

```text
/noar-intelligent-development first-run
```

**Then**, if a shell is available in Claude Code's session:

```sh
python3 .claude/scripts/update_repository_map.py
python3 -m unittest discover -s .claude/scripts -p 'test_*.py'
python3 .claude/scripts/check_integration_readiness.py --evidence .claude/audit/templates/integration-evidence.example.json
```

The last command is **expected to report `NEEDS_TESTING` and exit with code 2** because it is a placeholder with unperformed runtime gates. This proves the validator is enforcing verification, not that the app is broken.

## 4. Audit both sides (expensive, intentional)

```text
/noar-intelligent-development audit full frontend
/noar-intelligent-development audit full backend
```

Five independent council voters per domain, with 3-of-5 advisory verdict. The Orchestrator coordinates and the Documentation Guardian receives concise handoff. This is a static audit unless separate staging runtime tests are expressly authorized. To conserve Claude Pro usage, run the councils sequentially.

## 5. Integration testing (independent of 3/5 votes)

**Prepare only:**

```text
/noar-intelligent-development test integration
```

**Run browser smoke:** after explicitly approving a **disposable local staging NoAr** and ensuring Node/browser are available, use:

```sh
cd qa/e2e
npm install
npx playwright install chromium
NOAR_E2E_STAGING=1 NOAR_E2E_BASE_URL=http://127.0.0.1:8787 npx playwright test
```

This exercises a public API and unauthenticated browser controls only, not media streaming. The config refuses to launch without staging opt-in and blocks non-loopback URLs by default. The packages are isolated to `qa/e2e`; no root dependency edits are necessary.

**Full verified integration:**

```text
/noar-intelligent-development test full-system
```

This is a **planning/review request until you provide a safe isolated environment and permission for active tests**. Authenticated UI/API/SQLite, scheduling, media output and recovery require disposable accounts, fixtures and staging. Never run tests on the actual 24/7 playout.

## 6. Completion

```text
/noar-intelligent-development docs-impact
/noar-intelligent-development finalize
```

The QA agent sends evidence statuses to the Orchestrator and Documentation Guardian; final results separate council recommendations from actual verification gates.

## Modes (quick reference)

| Command | Intent |
| --- | --- |
| `map` / `find scheduler` | Persistent incremental navigation index |
| `implement <task>` | Minimal source changes with focused tests |
| `docs <topic>` / `docs-impact` | Documentation with Haiku, Guardian check |
| `release-review` | Read-only SemVer/CI/Docker analysis |
| `audit changed frontend` | Targeted five-reviewer frontend council |
| `audit changed backend` | Targeted five-reviewer backend council |
| `audit full frontend` / `audit full backend` | Complete, expensive static councils |
| `test integration` | QA-focused API/UI/DB integration checks |
| `test e2e` | Playwright staging-only browser checks |
| `test full-system` | Staging scheduler, media and recovery plan/tests |
| `first-run` | Setup validation and index/unit tests; zero runtime changes |
| `finalize` | Independent gates + Guardian handoff |

More: `docs/ai/FIRST_RUN.md`, `docs/ai/ROLES_AND_VOTING.md`, `docs/ai/INTEGRATION_TEST_PLAN.md`, `qa/e2e/README.md`, `.claude/audit/QUALITY_GATES.md`.

## Explicit limits

- Scripts and sample smoke tests are supplied but **not yet run against your live NoAr**.
- A voting majority is never sufficient proof of functioning playback, actual button clicks, API persistence or release readiness.
- No credentials, media, traces or real user data should be committed.
- Do not merge, tag, release, restart, publish or deploy without the user's permission.
- Five reviewers plus extra QA increase quota; use narrow tests before full audits, not every time.
