# FFmpeg Specialist — already installed in NoAr

The specialist is maintained directly in this repository. **Do not reinstall the original ZIP or run a one-time patch installer.** This guide is for new Claude Code sessions and maintainers.

## Files

- `.claude/agents/noar-ffmpeg-docs-specialist.md` — read-only adviser
- `.claude/skills/noar-ffmpeg/SKILL.md` — `/noar-ffmpeg`
- `.claude/rules/ffmpeg-reference.md` — path-scoped activation guidance
- `.claude/scripts/ffmpeg_docs.py` — bounded official-document lookup and local cache
- `docs/ai/ffmpeg/SOURCES.json` — official reference URLs
- `CLAUDE.md`, `.claude/agents/noar-orchestrator.md`, and the main skill — brief integration rules

## Use in Claude Code Web

Open a session using the branch with these files. No account-level import is normally required for repository skills.

```text
/noar-ffmpeg verify
/noar-ffmpeg reference SCTE-35 MPEG-TS timing
/noar-intelligent-development review SCTE-35 implementation
```

## Run local checks when a terminal is available

```sh
python3 .claude/scripts/ffmpeg_docs.py topics 'SCTE-35'
python3 -m unittest discover -s .claude/scripts -p 'test_ffmpeg_docs.py'
```

Documentation search needs network access unless cached; the `topics` command works offline. The online FFmpeg documentation does not guarantee the installed native FFmpeg C API or `ffmpeg-next` behavior. All actual media output checks require isolated staging and evidence.

The specialist remains read-only and non-voting; the Backend Council still has exactly five reviewers.
