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
- `/noar-intelligent-development layout desktop|mobile|all [fix]`: `noar-desktop-layout` (1280–1920 px) and `noar-mobile-layout` (320–1024 px) check pages in a browser against disposable staging with `qa/layout/check-layout.cjs`, and fix frontend layout only when asked. Both are **non-voting**.
- Never declare runtime readiness based solely on static review, successful unit tests or advisory votes. Consult `.claude/audit/INTEGRATION_QUALITY_GATES.md`.

## Change safety

- Preserve public API contracts, existing SQL migration checksums and playout behavior unless explicitly requested.
- Do not commit, push, merge, tag, publish, deploy or modify production configuration without permission.
- Never send credentials, media assets or full configuration files to audit reports.
- A 3-of-5 council verdict is advisory, not permission to deploy. Full backend audits require targeted static and, where feasible, staging tests for engine streams, security, migrations and restart resilience.

## FFmpeg Documentation Specialist

The NoAr project includes a dedicated FFmpeg documentation specialist:

- Agent: `noar-ffmpeg-docs-specialist`
- Skill: `/noar-ffmpeg`
- Documentation index: `docs/ai/ffmpeg/`
- Documentation search utility: `.claude/scripts/ffmpeg_docs.py`

### Automatic delegation

For tasks involving FFmpeg, codecs, filters, muxers, demuxers, SRT, RTMP, HLS, MPEG-TS, SCTE-35, PTS/DTS, audio/video synchronization, hardware acceleration, or media processing:

1. The main Claude session or `noar-orchestrator` must determine whether specialized FFmpeg research is necessary.
2. When necessary, delegate a focused research task to `noar-ffmpeg-docs-specialist`.
3. Provide specific questions and relevant NoAr source file paths.
4. Require references to official FFmpeg documentation, relevant C APIs, and version compatibility.
5. Verify recommendations against the actual FFmpeg libraries and `ffmpeg-next` version used by NoAr.
6. Return concise technical findings to the implementation agent or Media Engine Reviewer.
7. Request runtime testing through `noar-integration-e2e-qa` when applicable.
8. Request a documentation impact review through `noar-documentation-guardian`.

### Token efficiency

- Consult the persistent repository map before reading media engine source files.
- Search only relevant FFmpeg documentation sections.
- Reuse cached documentation and verified findings when still current.
- Avoid downloading or loading the entire FFmpeg documentation into the model context.
- Do not invoke the specialist for unrelated or trivial changes.

### Agent boundaries

- The FFmpeg Specialist is read-only and non-voting.
- The specialist does not replace `noar-media-engine-reviewer`.
- The Backend Council retains exactly five voting members.
- The Orchestrator coordinates research and implementation without casting audit votes.
- If nested agent delegation is unavailable, the main Claude session should invoke the specialist directly.
- No FFmpeg-related change is considered runtime-verified without actual tests in an isolated environment.