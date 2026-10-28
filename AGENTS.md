# NoAr repository instructions for Codex

NoAr is a 24/7 Rust/Vue TV playout system. Protect on-air continuity, stream timing, API compatibility, authentication, SQLite data and predictable release behavior. `backend/AGENTS.md` adds mandatory conventions for changes under `backend/`.

## Default development workflow

For ordinary natural-language requests to implement, fix or refactor code:

1. Locate affected files and symbols using `docs/ai/repository-map.md` and `python3 .claude/scripts/update_repository_map.py --find '<topic>'`. Confirm in current source, reading only needed sections.
2. Consult only the relevant `.agents/skills/` adapter for unfamiliar or version-sensitive technical questions. Each adapter points to shared canonical documentation in `.claude/skills/`; do not load every skill by default.
3. Make small, clear changes consistent with existing code and contracts; add focused regression tests when behavior changes.
4. Plan verification with `python3 .claude/scripts/verify_changes.py --files <changed paths>`; run feasible, permitted local checks with `--run`. Correct failures and state exactly what passed or remains untested.
5. For UI interaction, scheduling, SQLite persistence or FFmpeg/SRT/RTMP/HLS output, require actual evidence from isolated, authorized staging. Never infer runtime PASS from source review or documentation. Use NOT_RUN/NOT_AVAILABLE when appropriate.
6. Review documentation impact for material changes. Summarize changed paths, test evidence, uncertainty and next actions.

Small tasks need no subagent or audit council by default. Keep official documentation research scoped and verify installed dependency versions.

## Repository language, GitHub and release safeguards

- All repository files, code comments, PRs, issues, commit messages, GitHub reviews and documentation must be written in **English**. Chat with the user can be in Portuguese.
- No live channel manipulation, production database changes, destructive operations, production deployment, release publishing, tags, PR merge or pushes without the user's authorization.
- Read `docs/releasing.md` when preparing a release. The existing `.claude/scripts/prepare_release.py` and GitHub Actions can create version PRs; merging a reviewed release PR triggers tagging and Docker publication. Ordinary code PRs do not change `latest`.
- Use throwaway users, databases and synthetic media for tests; never transmit sensitive configuration or real operator credentials.

## Compatibility limits

Claude-specific agent definitions under `.claude/agents/` are **not** Codex subagents. If independent five-reviewer councils cannot run, report NOT_RUN and do not fabricate any ballots or 3/5 approvals. For risky tasks consult `.claude/audit/QUALITY_GATES.md`.

See `docs/ai/CODEX.md` for usage.
