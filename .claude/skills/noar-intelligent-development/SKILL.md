---
name: noar-intelligent-development
description: Token-efficient NoAr work with an incremental code map, Orchestrator, Documentation Guardian and separate five-voter frontend and backend audit councils, and evidence-based frontend-to-backend E2E integration QA.
disable-model-invocation: true
---

# NoAr Intelligent Development

User request: $ARGUMENTS

Applies only to the NoAr Rust + Vue 24/7 playout repository. Work carefully; reducing tokens must **never** override correctness, security, authorization, or broadcast continuity.

## Ordinary coding requests

Natural-language change requests automatically use the root `CLAUDE.md` workflow: locate → consult scoped references when necessary → implement cleanly → run relevant deterministic checks via `verify_changes.py` → report evidence. Do not demand a slash command or launch full councils for routine changes.

## 1. Interpret the requested mode

- `map`: update/reuse the indexed file inventory and module map; report changes only.
- `find <topic>`: refresh the map only if stale, query paths/symbols and confirm in current source.
- `docs <topic>`: update only relevant documentation, using `noar-documentation` (Haiku) for substantial writing; prefer a direct small edit for trivial changes.
- `release-review`: inspect SemVer, changelog, migrations, release tags, GitHub Actions and Docker publication directly without mutating anything; consult release-manager instructions only if a complex plan is needed.
- `release [patch|minor|major|rc.N]`: prepare the next release with `noar-release-manager` (Sonnet): notes against commits, proposed SemVer, gates, and — when asked — the `Release X.Y.Z` pull request. Never tags, merges or publishes; merging that pull request does (see `docs/releasing.md`).
- `test integration`: verify frontend ↔ Rust API ↔ disposable SQLite in a **user-approved isolated staging environment** with actual evidence; no five-voter council by default.
- `test e2e`: run focused Playwright browser checks against verified staging only; inspect `qa/e2e/README.md` and never run against production.
- `test full-system`: coordinate staged browser/API/DB/scheduler/media output testing with synthetic fixtures and explicit user approval; report independently verified and missing checks.
- `layout desktop [page]` / `layout mobile [page]`: check how pages look at computer widths (1280–1920 px) with `noar-responsive-layout`, or at phone and tablet widths (320–1024 px) with `noar-responsive-layout`, against a disposable staging instance with `qa/layout/check-layout.cjs`; report only, unless `fix` is added (`layout mobile fix`), then fix in `frontend/src/` and check again. Both run for `layout all`. Non-voting.
- `first-run`: validate tool files, generate the map, run local unit tests of the audit scripts, review readiness, and prepare staging tests without launching production services.
- `docs-impact`: ask the read-only Documentation Guardian what needs updating after completed work.
- `finalize`: gather evidence, run applicable verification gates and the Documentation Guardian; summarize documentation status.
- `review <topic>`: use the smallest relevant number of reviewers for targeted feedback; no five-vote verdict unless `audit` is requested.
- `audit changed`: conduct a five-reviewer frontend council review of changes, unless a scope is specified.
- `audit changed backend`: audit backend-related changes with the five-voter BACKEND council and relevant integration points.
- `audit full frontend`: review the entire frontend using the five-voter FRONTEND council; read-only, cost-intensive.
- `audit full backend`: review all backend/app and backend/engine modules, migrations, tests and Rust build/release integration using the five-voter BACKEND council; read-only, cost-intensive.
- `audit full`: ask for intended area when genuinely ambiguous; default to whole project only when explicitly requested, and run separate backend and frontend councils without pooling votes.
- `implement <task>`: identify minimal edits, implement, run targeted tests, and refresh relevant maps/notes. Do **not** run five auditors unless explicitly asked.
- No recognized mode: start with a brief focused interpretation and use minimal exploration; ask only for truly blocking ambiguity.

## 2. Persistent repository memory comes first

1. Consult `docs/ai/repository-map.md` and `docs/ai/architecture-overview.md` if present (read only the relevant section).
2. Refresh using `python3 .claude/scripts/update_repository_map.py` when the map may be stale. A repeated run reuses unchanged symbol metadata; local hashing **does not consume LLM tokens**.
3. Search using `python3 .claude/scripts/update_repository_map.py --find '<term>'` before reading large files.
4. Inspect only needed file sections. If a map conflicts with code, **the source wins**. Notes about modified files must be revalidated.
5. Keep concise human-verified observations in `docs/ai/file-notes.md`, with file path and revision/hash when practical. Do not pretend machine-generated symbol listings are semantic understanding.
6. Index files and notes remain useful only if committed to the intended branch (with user permission).

## 3. Orchestration, documentation stewardship and cost control

- The main Claude session directly coordinates tiny tasks. For complex multi-stage work, use `noar-orchestrator` (Sonnet) as the non-voting coordinator **only if doing so provides value**; it can delegate to smaller subagents if the `Agent` tool is supported. Otherwise the main session coordinates directly. Never spawn both layers redundantly.
- Define scope, git base/changed paths, who owns each task, how the result will be verified, and token/time budget before delegating. Assign specialists to *different* evidence; avoid five agents re-reading the same file.
- For material API/operator-facing, migration, release and full-audit work, request a read-only Documentation Guardian handoff; minor edits use direct impact checks. Provide a compact change summary, relevant paths, and verified findings. It inspects whether documentation, examples, changelog or repository map must change, but never monitors agents continuously.
- A scoped local candidate list is available through `python3 .claude/scripts/analyze_documentation_impact.py --base <ref>` or `--files path1 path2`; these are suggestions only. Use the writing agent `noar-documentation` (Haiku) **only when documentation edits are authorized**.
- At completion, consult `.claude/audit/QUALITY_GATES.md` for risky tasks and keep `PASS / FAIL / NOT_RUN / NOT_AVAILABLE` evidence. No auditor vote substitutes for browser, stream or migration tests.

## 3a. Independent cross-layer verification (new in v4)

- `noar-integration-e2e-qa` (Sonnet) checks *observed* frontend/backend behavior. It is non-voting and is engaged only for `test ...` modes or targeted verification, not on every documentation update.
- Inspect `docs/ai/INTEGRATION_TEST_PLAN.md` and `.claude/audit/INTEGRATION_QUALITY_GATES.md` before running tests.
- Distinguish static contract review from real browser clicks, API responses, SQLite writes, and emitted media. **Do not mark PASS without tool output**.
- Run the isolated Playwright smoke harness in `qa/e2e` only against a reviewed disposable staging service. It intentionally refuses to start without `NOAR_E2E_STAGING=1`, and defaults to loopback-only URLs.
- Use `python3 .claude/scripts/check_integration_readiness.py --evidence path/to/evidence.json` to derive `READY_FOR_REVIEW`, `NEEDS_TESTING` or `NOT_READY`; even `READY_FOR_REVIEW` does not authorize a release.
- If browser runtime, app server or synthetic streaming fixtures are unavailable, label tests `NOT_AVAILABLE`/`NOT_RUN`; provide next commands for a staged environment.

## 4. Efficient model delegation

- `noar-documentation` (Haiku) — user/developer docs, README, API comments.
- `noar-release-manager` (Sonnet) — prepares releases with `.claude/scripts/prepare_release.py`; opens the release pull request only when asked; never tags or publishes.
- FRONTEND council for frontend audits only: backend/API integration, frontend, visual design, accessibility and functional reviewers.
- BACKEND council for backend audits only: backend Rust/API, media engine, security/authorization, persistence/migrations and reliability/testing reviewers.
- Each council has five independent ballots; do not pool, reuse or fabricate votes across councils. Full audits are expensive and intentional.
- `noar-documentation-guardian` (Haiku) — review documentation impact of completed, material changes; NON-VOTING.
- `noar-orchestrator` (Sonnet) — coordinate multi-stage reviews; NON-VOTING.
- `noar-integration-e2e-qa` (Sonnet) — frontend/API/database/browser/full-system runtime evidence; NON-VOTING.
- `noar-responsive-layout` (Sonnet) — one non-voting agent for desktop, tablet and mobile browser layout verification.
- Do not invoke a subagent for a two-line change or duplicate searches in multiple agents.
- Agent model aliases are requests, not guarantees of plan availability; report when unavailable.
- The main model is set by the Claude Code selector. This skill cannot force an Opus-to-Sonnet switch.

### Official technology reference skills

For version-sensitive questions, consult only the relevant playbook: `/noar-rust`, `/noar-vue`, `/noar-docker`, or `/noar-video-processing`. They complement `/noar-ffmpeg` and are **not additional agents or voting council members**. Confirm versions from lockfiles/native build and query only 1–3 relevant official documentation sections. See `docs/ai/tech-references/README.md`.

### FFmpeg specialist (conditional, non-voting)

For complex FFmpeg, SCTE-35, media timestamp or protocol questions, the lead/Orchestrator may request a focused read-only handoff from `noar-ffmpeg-docs-specialist`. Follow `.claude/rules/ffmpeg-reference.md`; detailed retrieval is in `/noar-ffmpeg`. Verify native FFmpeg and `ffmpeg-next` compatibility before implementing. The adviser does **not** join Backend Council votes; staging behavior requires Integration QA.

## 5. NoAr safety and targeted checks

- API/auth/DB: `backend/app/src/api/`, `backend/app/src/db/handles/`, `migrations/`.
- Playout: `backend/app/src/player/`, `backend/engine/src/`; protect timing, SCTE-35 markers, FFmpeg output, clock and SRT/RTMP/HLS behavior.
- Frontend: `frontend/src/` (Vue/TypeScript/router/stores).
- Releases: `Cargo.toml`, root `package.json`, `package-lock.json`, `Cargo.lock`, `CHANGELOG.md`, `.github/workflows/`, `docker-compose.yml`.
- Test focused changes first. For Rust changes use `cargo fmt --all -- --check`, and select feature flags consciously when using clippy/tests. For frontend changes use type-check/build/lint as relevant.
- Existing `backend/AGENTS.md` applies within backend; report conflicts rather than ignoring it.
- Do not claim an interface button works without actually exercising it in a **non-production** browser environment. Report `NOT_VERIFIED` where preview/runtime unavailable.

## 6. Five-reviewer councils only for `audit`

The lead or orchestrator is a facilitator, NOT a sixth vote. The Documentation Guardian is also NON-VOTING. EXACTLY five agents each contribute one independent ballot **per selected council**. Frontend council:

1. `noar-backend-reviewer` (Sonnet): Rust/server/engine, security, streaming, persistence.
2. `noar-frontend-reviewer` (Sonnet): Vue/TS, state, API integration, errors.
3. `noar-visual-design-reviewer` (Haiku): UI consistency, responsive visual appearance; screenshots if available.
4. `noar-accessibility-reviewer` (Haiku): semantics, keyboard, contrast and WCAG, verified where possible.
5. `noar-functional-reviewer` (Sonnet): routes, buttons, forms, navigation and user flows; actual clicking ONLY if browser/runtime exists.



For `audit full backend` and `audit changed backend`, use the **BACKEND council instead**:

1. `noar-backend-reviewer` (Sonnet) — Rust API, application, scheduler, playout integration.
2. `noar-media-engine-reviewer` (Sonnet) — FFmpeg, encoding, clock, SRT/RTMP/HLS, SCTE-35.
3. `noar-security-reviewer` (Sonnet) — API auth, file access, upload, trust boundaries.
4. `noar-database-reviewer` (Sonnet) — SQLite, migrations, transactions, persistence, recovery.
5. `noar-reliability-reviewer` (Haiku) — test coverage, CI, observability, restart/update readiness.

Backend audit coverage: `backend/app/`, `backend/engine/`, `migrations/`, Rust workspace, related CI/Docker, and their frontend API contracts. Use `.claude/audit/BACKEND_AUDIT_CHECKLIST.md`; limit repeated reads with separate specialist scopes, and never claim a complete runtime audit from static inspection. A 3/5 majority is advisory; security, data-loss, FFmpeg output and broadcast continuity blockers override approval.

**Round A: Independent discovery.** Assign bounded scopes; each voter returns up to 8 evidenced findings with `path:line`, severity, consequence, and whether tested. Don't fabricate unknown checks.

**Round B: Peer challenge.** Merge/deduplicate into IDs `F-001` etc.; each reviewer sees focused peer objections and may revise arguments. Use direct Agent Teams communication ONLY when truly supported and enabled. Otherwise the lead relays summaries. Never claim direct agent-to-agent messaging if it did not occur.

**Round C: Independent ballots.** Each agent votes separately on EVERY final finding: `confirm`, `dismiss`, `abstain`, and overall readiness: `approve`, `deny`, `abstain`. Preserve dissenting opinions and reason texts. Don't fill missing agents' votes yourself.

**Deterministic gate:**
- >=3 `confirm` -> finding CONFIRMED; >=3 `dismiss` -> DISMISSED; else UNRESOLVED.
- >=3 `deny` -> readiness DENIED; >=3 `approve` -> APPROVED_ADVISORY_ONLY, unless an independently verified high/critical blocking issue is confirmed (then BLOCKED). Else INCONCLUSIVE.
- Verification grades and evidence matter more than numeric majority. Disclose missing browser/runtime tests; advisory votes do not authorize a release.

Persist a small `docs/ai/audits/<run-id>/findings.json`, five separate ballots in `votes/`, and `audit-report.md` when the user requests saved results. Follow `.claude/audit/templates/` and validate with:

```bash
python3 .claude/scripts/tally_audit_votes.py --council backend --findings docs/ai/audits/<run-id>/findings.json --votes-dir docs/ai/audits/<run-id>/votes
# Use --council frontend for a frontend audit (the default).
```

No valid five-ballot tally from the selected council? Report INCOMPLETE instead of forging consensus. Even with an advisory 3/5 approval, independently evaluate `.claude/audit/QUALITY_GATES.md`; do not mark untested interface controls or broadcast paths as verified.

## 6a. First-run setup (explicitly requested)

Run the following from repository root, on the setup branch:

```sh
python3 .claude/scripts/update_repository_map.py
python3 -m unittest discover -s .claude/scripts -p 'test_*.py'
python3 .claude/scripts/check_integration_readiness.py --evidence .claude/audit/templates/integration-evidence.example.json
```

The third command is intentionally expected to exit nonzero with `NEEDS_TESTING` for placeholder evidence. Never mark first-run as an E2E PASS. Do not launch Docker or install Playwright without the user's staging approval. See `docs/ai/FIRST_RUN.md` for all steps.

## 7. Output discipline

Summarize changed/reviewed paths, key evidence, tests performed and skipped, documentation impact (`UPDATED`, `NO_UPDATE_REQUIRED`, `FOLLOW_UP`), risk, and next action. Do not dump full source or unbounded logs. Never silently modify source, docs or production configuration during a read-only audit or release review. Return paths/line numbers for actionable issues.
