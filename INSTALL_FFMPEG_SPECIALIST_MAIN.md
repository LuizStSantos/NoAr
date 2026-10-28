# NoAr Main — FFmpeg Specialist integration (additive)

This bundle was prepared for the current NoAr `main` structure, which already contains:
- `CLAUDE.md`
- `.claude/agents/noar-orchestrator.md`
- `.claude/skills/noar-intelligent-development/SKILL.md`
- existing independent five-member Frontend/Backend Audit Councils.

**Nothing in this ZIP removes these files or overwrites them.** It includes an additive, idempotent installer that inserts FFmpeg delegation instructions into those three existing documents.

## Install locally (recommended)

```sh
git clone https://github.com/LuizStSantos/NoAr.git
cd NoAr
git switch -c feat/ffmpeg-specialist
# Extract NoAr-FFmpeg-Specialist-Main-Ready-English.zip here, preserving paths.
python3 .claude/scripts/install_ffmpeg_specialist.py --check
python3 .claude/scripts/install_ffmpeg_specialist.py --apply
python3 -m unittest discover -s .claude/scripts -p 'test_ffmpeg_docs.py' -v
python3 -m unittest discover -s .claude/scripts -p 'test_install_ffmpeg_specialist.py' -v
python3 .claude/scripts/ffmpeg_docs.py topics 'scte35'
git status --short
git diff -- CLAUDE.md .claude/agents/noar-orchestrator.md .claude/skills/noar-intelligent-development/SKILL.md
# Once reviewed: git add the intended files, commit, push and open PR into main.
```

If you already cloned NoAr, run `git switch main && git pull --ff-only` then `git switch -c feat/ffmpeg-specialist`. Keep any existing working changes safe before switching branches.

## Install from GitHub.dev or Claude Code Web (no local terminal)

1. Create a new branch from `main` and open `https://github.dev/LuizStSantos/NoAr/tree/feat/ffmpeg-specialist` (or open GitHub.dev from the repository and switch to your new branch).
2. Unzip this bundle locally. Add all **new** `.claude` and `docs/ai/ffmpeg` files using the file explorer. Dot-folders may be hidden in GitHub's basic upload UI; the editor or Git CLI are more reliable.
3. Ask Claude Code Web on the same branch to read `INSTALL_FFMPEG_SPECIALIST_MAIN.md` and run `python3 .claude/scripts/install_ffmpeg_specialist.py --apply`. It will edit the three existing Markdown files without replacing prior content.
4. Inspect the diff, run tests, commit and create a PR against `main`. Only merge after review.

Do NOT commit `docs/ai/symbol-index.json` merely because of a local timestamp; refresh the Token Saver index when intended and review the generated diff.

## Files that will be modified by the installer

- `CLAUDE.md`: a brief FFmpeg reference rule for the main Claude.
- `.claude/agents/noar-orchestrator.md`: conditional delegation and handoff rules.
- `.claude/skills/noar-intelligent-development/SKILL.md`: reuses the FFmpeg Specialist in the unified development flow.

Everything else in this bundle is a **new file**. `.gitignore` is already configured to ignore `.cache/` in current `main`, so it is NOT changed.

## First run after merge or on branch

```text
/noar-ffmpeg verify
/noar-ffmpeg reference SCTE-35 MPEG-TS signaling and PTS timing
/noar-intelligent-development implement review SCTE-35 timing in NoAr using official FFmpeg docs, report only before any code edit
```

A CLI or library feature may not be supported by the native FFmpeg build installed in Claude Code's environment. Distinguish FFmpeg docs, C ABI, `ffmpeg-next` binding and observed runtime tests. The specialist does not change production, run broadcasts or vote in the five-member backend council.

## Check the merged result

```sh
python3 .claude/scripts/install_ffmpeg_specialist.py --check
python3 .claude/scripts/ffmpeg_docs.py topics 'SCTE-35'
```

Both scripts run without online access for their basic checks. `topics` queries return document suggestions, not runtime verification. Use `search ... --manual` only when network access to the official FFmpeg website is available.
