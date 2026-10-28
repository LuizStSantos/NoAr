# AI toolkit maintenance

## Rules

Keep the root `CLAUDE.md` short: critical repository safety, map usage and links to detailed workflows. Add detailed behavior to the applicable skill or path-scoped rule, not to all of them.

The `noar-orchestrator` is a task-scoped coordinator, not a permanent monitor. The FFmpeg Specialist is non-voting. Maintain independent five-member Frontend and Backend Council votes.

Do not install or update the FFmpeg Specialist by rerunning the old one-time installer; integration is already committed and managed as ordinary Markdown files. The live tooling remains `.claude/scripts/ffmpeg_docs.py`.

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

Keep `INSTALL.md` as the permanent project-facing guide. FFmpeg setup/reference stays in `ffmpeg/INSTALL.md` and `ffmpeg/README.md`. Do not retain one-time ZIP instructions in the repository root. Preserve historical audit files as evidence.

## Not covered by commits

GitHub branch protection is a repository setting, not a tracked file. Configure a ruleset to require pull-request validation before merging significant changes into `main`, separately from this branch.
