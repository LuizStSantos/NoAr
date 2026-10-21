# NoAr Context-Efficient Development Rules

> Append these rules to the existing root `CLAUDE.md` if present. Do not replace existing instructions.

- Start navigation with `docs/ai/repository-map.md` and `docs/ai/architecture-overview.md` if present. For specific symbols use `python3 .claude/scripts/update_repository_map.py --find '<term>'`; do not print `symbol-index.json` into the conversation.
- Refresh the map with `python3 .claude/scripts/update_repository_map.py` at the start of a new worktree/session or after code changes. The script reuses unchanged file entries and runs without AI calls.
- Search narrowly and verify relevant source files; the index is not authoritative evidence of current behavior.
- Use Haiku subagents for substantial read-only exploration, documentation and release analysis when useful, and Sonnet for targeted architecture reviews. Avoid subagents for trivial lookups.
- Use small diffs and bounded command output. Run focused checks before comprehensive checks.
- Preserve `backend/AGENTS.md` guidance for Rust work.
- Never publish releases, push tags, merge PRs, change production updater policy or edit migrations/CI as an incidental action.
- Keep final reports concise, including changes, checks, and remaining uncertainty.
