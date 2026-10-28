# NoAr project guidance

NoAr is a 24/7 Rust/Vue television playout application. Reliability, API compatibility,
on-air continuity, database safety and predictable Docker releases take priority.

## Repository language policy

- Write **all GitHub pull request titles and descriptions, issues, review comments, commit messages, documentation and agent-authored repository text in English**.
- Users may communicate in Portuguese; respond conversationally in the user's language, but keep text published to GitHub and repository documentation in English.
- Keep existing code identifiers and externally defined user-facing translations unchanged unless the task explicitly requires a localization change.

## Standard development workflow

For ordinary natural-language requests to fix, add or refactor NoAr, **without needing a slash command**:
1. Locate the relevant symbols and verify the source contracts using the indexed map. The indexer's `--find` refreshes it before searching.
2. Consult only relevant official reference skills when a version-dependent API or unfamiliar implementation warrants it.
3. Implement the smallest maintainable fix consistent with `backend/AGENTS.md` and existing UI/API contracts. Add focused regression tests where practical.
4. Plan checks with `python3 .claude/scripts/verify_changes.py --files <changed paths>`; run with `--run` when local tests are available and permitted, then fix reported failures.
5. Test affected behavior in disposable staging if available; never claim UI, persistence or media output PASS based on a static inspection.
6. Report changed paths, observed check results, missing runtime evidence and documentation impact. Do not commit/push/deploy or touch production without authorization.

Use the parent Claude session for simple changes. Full five-agent councils and releases are only for explicit requests.

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
- For material API, operator-facing, migration or release work, `noar-documentation-guardian` reviews a compact change summary and flags any docs, changelog or map updates. Neither guardian nor orchestrator votes.
- For frontend audits, the five frontend council reviewers vote. For backend audits, the five backend council reviewers vote. Never pool votes across domains. No sixth vote.
- Release/production readiness requires independent evidence from `.claude/audit/QUALITY_GATES.md`, not merely a 3/5 vote.
- Never claim UI clicks, media output, tests or release checks were verified unless actually executed.

## Cross-layer integration verification

- `/noar-intelligent-development test integration`: focused Vue ↔ Rust API ↔ disposable SQLite checks (read and write only in user-approved staging).
- `/noar-intelligent-development test e2e`: browser interactions using the `qa/e2e` Playwright smoke harness, **staging only**.
- `/noar-intelligent-development test full-system`: include scheduler and synthetic media output when a safe fixture/environment exists.
- `noar-integration-e2e-qa` is **non-voting**. Its observed results are independent of the five-voter backend and frontend council ballots.
- `/noar-intelligent-development layout desktop|mobile|all [fix]`: `noar-responsive-layout` checks desktop (1280–1920 px) and mobile/tablet (320–1024 px) pages in a browser against disposable staging with `qa/layout/check-layout.cjs`, and fix frontend layout only when asked. Both are **non-voting**.
- Never declare runtime readiness based solely on static review, successful unit tests or advisory votes. Consult `.claude/audit/INTEGRATION_QUALITY_GATES.md`.

## Change safety

- Preserve public API contracts, existing SQL migration checksums and playout behavior unless explicitly requested.
- Do not commit, push, merge, tag, publish, deploy or modify production configuration without permission.
- Never send credentials, media assets or full configuration files to audit reports.
- A 3-of-5 council verdict is advisory, not permission to deploy. Full backend audits require targeted static and, where feasible, staging tests for engine streams, security, migrations and restart resilience.

## Technology references

- For nontrivial version-sensitive work use the relevant repository skill: `/noar-rust`, `/noar-vue`, `/noar-docker`, `/noar-video-processing` or `/noar-ffmpeg`. Confirm lockfile and installed native library versions.
- These are reference instructions, not agents or new council voters. Select only the needed official documentation through `.claude/rules/`. See `docs/ai/tech-references/README.md`.

## FFmpeg reference guidance

- For nontrivial FFmpeg/media API questions, consult the scoped rule `.claude/rules/ffmpeg-reference.md` and ask `noar-ffmpeg-docs-specialist` only when official research is necessary.
- Read relevant references through `/noar-ffmpeg`; check the actual FFmpeg build and Rust bindings before applying advice.
- The specialist is read-only and non-voting; the existing Backend Council remains five voters. See `docs/ai/ffmpeg/README.md`.

