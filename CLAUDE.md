# NoAr project guidance

NoAr is a 24/7 Rust/Vue television playout application. Reliability, API compatibility,
on-air continuity, database safety and predictable Docker releases take priority.

## Efficient navigation

- Read `docs/ai/repository-map.md` first **if it exists**. It is a navigation aid, not verified implementation behavior.
- Run `python3 .claude/scripts/update_repository_map.py` to refresh the local map when needed.
- Query symbols with `python3 .claude/scripts/update_repository_map.py --find '<term>'`.
- Read only relevant source ranges. Do not paste the complete symbol index or large logs into chat.
- For existing Rust coding rules, follow `backend/AGENTS.md`; do not override them here.

## Delegation, documentation and verification

- Use the `/noar-intelligent-development` skill for explicit map, docs, release, implementation and audit workflows.
- Prefer one Haiku helper for substantial narrow exploration/documentation; a helper is not always needed.
- Sonnet handles routine development. Reserve Opus for difficult media, concurrency and architecture problems.
- For complex, user-requested multi-stage work, `noar-orchestrator` coordinates; the main Claude session handles simple work directly.
- At the end of material work, `noar-documentation-guardian` reviews a compact change summary and flags any docs, changelog or map updates. Neither guardian nor orchestrator votes.
- For frontend audits, the five frontend council reviewers vote. For backend audits, the five backend council reviewers vote. Never pool votes across domains. No sixth vote.
- Release/production readiness requires independent evidence from `.claude/audit/QUALITY_GATES.md`, not merely a 3/5 vote.
- Never claim UI clicks, media output, tests or release checks were verified unless actually executed.

## Cross-layer integration verification

- `/noar-intelligent-development test integration`: focused Vue ↔ Rust API ↔ disposable SQLite checks (read and write only in user-approved staging).
- `/noar-intelligent-development test e2e`: browser interactions using the `qa/e2e` Playwright smoke harness, **staging only**.
- `/noar-intelligent-development test full-system`: include scheduler and synthetic media output when a safe fixture/environment exists.
- `noar-integration-e2e-qa` is **non-voting**. Its observed results are independent of the five-voter backend and frontend council ballots.
- Never declare runtime readiness based solely on static review, successful unit tests or advisory votes. Consult `.claude/audit/INTEGRATION_QUALITY_GATES.md`.

## Change safety

- Preserve public API contracts, existing SQL migration checksums and playout behavior unless explicitly requested.
- Do not commit, push, merge, tag, publish, deploy or modify production configuration without permission.
- Never send credentials, media assets or full configuration files to audit reports.
- A 3-of-5 council verdict is advisory, not permission to deploy. Full backend audits require targeted static and, where feasible, staging tests for engine streams, security, migrations and restart resilience.
