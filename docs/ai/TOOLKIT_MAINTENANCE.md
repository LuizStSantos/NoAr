# AI toolkit maintenance

## Rules

Keep the root `CLAUDE.md` short: critical repository safety, map usage and links to detailed workflows. Add detailed behavior to the applicable skill or path-scoped rule, not to all of them.

The `noar-orchestrator` is a task-scoped coordinator, not a permanent monitor. The FFmpeg Specialist is non-voting. Maintain independent five-member Frontend and Backend Council votes.

Do not install or update the FFmpeg Specialist by rerunning the old one-time installer; integration is already committed and managed as ordinary Markdown files. The live tooling remains `.claude/scripts/ffmpeg_docs.py`.

## Technology reference skills

The `noar-rust`, `noar-vue`, `noar-docker` and `noar-video-processing` skills are scoped reference instructions, not agents or voters. Maintain official links in their skill files. Check dependency lockfiles before following documentation; keep FFmpeg C API guidance in the existing specialist. Do not invoke all skills unnecessarily.

## Before committing a toolkit edit

```sh
python3 -m unittest discover -s .claude/scripts -p 'test_*.py'
python3 .claude/scripts/ffmpeg_docs.py topics 'SCTE-35'
python3 .claude/scripts/update_repository_map.py
python3 .claude/scripts/update_repository_map.py --find scte35
git diff -- docs/ai/repository-map.md docs/ai/symbol-index.json
```

The map parser was upgraded to schema 2. The first local run after the upgrade rebuilds symbols to avoid silently reusing metadata produced by the earlier parser. It does **not** ask an AI model to read files. Only commit generated changes after verifying the diff and that no sensitive or untracked files were included. Changes to the repository after the last index generation require another incremental refresh.

The `ai-toolkit.yml` workflow executes helper tests and warns if the committed map is outdated; it deliberately does not push generated files into the branch. The `docker.yml` workflow ignores documentation-only/AI-toolkit-only pushes to `main`; tagged releases still build.

## Documentation maintenance

Keep `docs/ai/INSTALL.md` as the permanent toolkit guide. FFmpeg setup/reference stays in `ffmpeg/INSTALL.md` and `ffmpeg/README.md`. Do not retain one-time ZIP instructions in the repository root. Root should only contain files needed by development, builds, releases or GitHub navigation. Preserve historical audit files as evidence.

## Search and formatting configuration

The former root `.ignore` hid `frontend/`, `docs/` and `migrations/` from ripgrep-compatible searches. It was removed so code-search helpers can find project files; generated outputs remain covered by `.gitignore`. Keep `.sql-formatter.json` in the root for editor/CLI discovery, using the `sqlite` dialect to match NoAr migrations. The `.dockerignore` excludes Claude tooling and QA test scripts from the Docker build context, without excluding `backend/`, `frontend/`, `migrations/` or required assets.

## Not covered by commits

GitHub branch protection is a repository setting, not a tracked file. Configure a ruleset to require pull-request validation before merging significant changes into `main`, separately from this branch.
