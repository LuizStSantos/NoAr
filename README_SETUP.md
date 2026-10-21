# NoAr Claude Code Web Setup

This directory contains PROJECT-LEVEL helpers. It does not include the account skill uploaded through Customize > Skills.

## Install in the repository

1. Unzip the toolkit at the root of `LuizStSantos/NoAr`, preserving `.claude/` and `docs/ai/` paths.
2. Review the files. Do not overwrite existing agent or map files without merging them.
3. Add the rules from `CLAUDE_RULES_SNIPPET.md` to an existing root `CLAUDE.md`, or create `CLAUDE.md` if it does not exist. Keep `backend/AGENTS.md` unchanged.
4. Run `python3 .claude/scripts/update_repository_map.py` from the root to generate `docs/ai/repository-map.md` and `docs/ai/symbol-index.json`.
5. Commit the toolkit and generated index on a feature branch and open a pull request. Merge only after reviewing. Do not modify production code or release files as part of this setup.
6. Start a fresh Claude Code Web session linked to the updated branch/repository so project agents load.

## Install the account skill

Separately upload `NoAr-Claude-Skill-English.zip` in Claude > Customize > Skills > Create skill > Upload a skill, then enable `noar-token-saver`. This account-level skill also works in cloud project threads, where repository-only skills may not load.

## Use

- Skill: `/noar-token-saver locate scheduler event resumption logic`
- Map: `python3 .claude/scripts/update_repository_map.py`
- Find: `python3 .claude/scripts/update_repository_map.py --find scte35`
- Audit: Ask `noar-release-review` for a read-only assessment of the release workflow.

## Caveats

- Agents are project configuration, not part of the account-skill ZIP.
- The script only indexes file paths and approximate symbols. It does not send source files to any AI service.
- All Claude subagents still count toward the user's Claude plan usage limits.
- No external API keys, paid services, or additional Python dependencies are required.
