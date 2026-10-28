# NoAr AI Development Toolkit — Setup and Usage

The NoAr repository includes two project skills, specialized Claude Code agents, a persistent code map, audit councils and optional FFmpeg documentation research. **The toolkit is already in the repository:** no ZIP or account-skill import is needed when Claude Code loads project files correctly.

## Start a fresh Claude Code session

Open the NoAr repository and the intended branch at https://claude.ai/code. The root `CLAUDE.md` gives global guidance. The available skills are:

- `/noar-intelligent-development` — mapping, implementation, audits, documentation, releases, and integration QA.
- `/noar-ffmpeg` — bounded research in official FFmpeg manuals and C APIs.

Use `/noar-intelligent-development first-run` for an initial **non-production** toolkit check. It should not start on-air tests or full council audits unless separately requested.

## Common commands

```text
/noar-intelligent-development map
/noar-intelligent-development find scheduler
/noar-intelligent-development implement <task>
/noar-intelligent-development audit changed backend
/noar-intelligent-development audit full frontend
/noar-intelligent-development audit full backend
/noar-intelligent-development test integration
/noar-intelligent-development docs-impact
/noar-intelligent-development finalize
/noar-ffmpeg verify
/noar-ffmpeg reference SCTE-35 timing in MPEG-TS
```

Five reviewers vote in each independent Frontend or Backend Council; the Orchestrator, Documentation Guardian, Integration QA, and FFmpeg Specialist do **not** vote. Full audits are expensive; use incremental/scoped checks for routine work.

## Local deterministic checks

These run without Claude tokens (though some commands depend on local developer tools):

```sh
python3 -m unittest discover -s .claude/scripts -p 'test_*.py'
python3 .claude/scripts/update_repository_map.py
python3 .claude/scripts/update_repository_map.py --find scheduler
python3 .claude/scripts/ffmpeg_docs.py topics 'SCTE-35'
```

The map caches symbols by file SHA-256. The schema 2 parser forces a rebuild once after its upgrade. Do not paste the full JSON into model context. See `docs/ai/TOOLKIT_MAINTENANCE.md` before committing map changes.

## Verification rules

- Actual browser clicks, API persistence and media output require an authorized disposable staging instance. The Playwright smoke tests live in `qa/e2e/`, layout tests in `qa/layout/`.
- Separate `PASS`, `FAIL`, `NOT_RUN` and `NOT_AVAILABLE`. A 3-of-5 vote never authorizes a release or proves runtime behavior.
- No production actions, unreviewed database changes, merges, tags or deployments without explicit user authorization.
- For full setup walkthrough, see `docs/ai/FIRST_RUN.md`; for FFmpeg use `docs/ai/ffmpeg/INSTALL.md`; for quality gates see `.claude/audit/QUALITY_GATES.md`.

The GitHub Actions `ai-toolkit.yml` workflow runs Python helper tests and warns about a stale map. Code/build validation still runs through `validate.yml`. Documentation-only pushes do not need to publish a new Docker image.
