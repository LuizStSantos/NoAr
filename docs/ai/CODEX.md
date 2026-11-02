# Codex compatibility for the NoAr AI toolkit

Codex and Claude Code share **one** set of NoAr reference documents, deterministic checks and release workflows. The Codex files are intentionally lightweight adapters, not a parallel AI system.

## Start using Codex

1. Open `LuizStSantos/NoAr` in Codex on the intended branch.
2. Codex loads the root `AGENTS.md` for routine work. `backend/AGENTS.md` adds backend-specific requirements.
3. Ask naturally: "Fix the playlist-save issue, add a focused regression test, run the relevant checks, and explain what remains unverified. Do not deploy."
4. To use a specialized reference, request its repository skill (for example, "Use noar-video-processing to analyze timestamp drift").
5. Check actual test results, diff and PR reviews before merging.

No account-level skill installation or additional ZIP is required when the repository-local files are discoverable.

## Skills and canonical sources

| Codex adapter | Shared reference |
| --- | --- |
| `noar-intelligent-development` | `.claude/skills/noar-intelligent-development/SKILL.md` |
| `noar-rust` | `.claude/skills/noar-rust/SKILL.md` |
| `noar-vue` | `.claude/skills/noar-vue/SKILL.md` |
| `noar-docker` | `.claude/skills/noar-docker/SKILL.md` |
| `noar-gstreamer` | `.claude/skills/noar-gstreamer/SKILL.md` (primary media runtime reference) |
| `noar-ffmpeg` | `.claude/skills/noar-ffmpeg/SKILL.md` |
| `noar-video-processing` | `.claude/skills/noar-video-processing/SKILL.md` |

Each `.agents/skills/<name>/SKILL.md` contains a short description and a link to its canonical reference. Codex should consult only the relevant source, not preload every manual.

## Shared tools

```sh
python3 .claude/scripts/update_repository_map.py --find scheduler
python3 .claude/scripts/verify_changes.py --files frontend/src/views/PlayerView.vue
python3 -m unittest discover -s .claude/scripts -p 'test_*.py'
```

Use `python3 .claude/scripts/verify_changes.py --files <paths> --run` to execute selected local checks where environment dependencies and authorization permit. Never equate static review or a successful build with verified browser actions, database persistence or video output. Browser, scheduler and stream tests need disposable staging.

## Differences

- Claude Code reads root `CLAUDE.md` plus `.claude/skills/`; Codex reads root `AGENTS.md` plus `.agents/skills/`.
- Claude-specific `.claude/agents/` agents and the two five-voter councils are **not** automatically transferred. Do not fabricate ballots if no independent reviewers ran.
- Existing CI, version preparation, release tagging and Docker image publication are shared. A stable release PR merged into `main` triggers the existing workflow and updates `latest` only after successful release publication.
- All GitHub documentation, commits, PRs and review notes must be written in **English**; chats may be in Portuguese.

## Maintenance

Update technical instructions in their original `.claude/skills/` source. Keep Codex adapters short; run `.claude/scripts/test_codex_adapters.py` or the AI toolkit workflow to verify links and metadata. This integration does not change application logic or production configuration.
