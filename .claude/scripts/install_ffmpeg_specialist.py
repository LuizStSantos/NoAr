#!/usr/bin/env python3
"""Integrate the NoAr FFmpeg Specialist with existing project instructions.

Run from the NoAr repository root AFTER extracting the add-on ZIP:
  python3 .claude/scripts/install_ffmpeg_specialist.py --check
  python3 .claude/scripts/install_ffmpeg_specialist.py --apply

Only modifies three existing Markdown files. All insertions are additive,
anchored, and idempotent. It does not call git or access the network.
"""
from __future__ import annotations

import argparse
from pathlib import Path
import sys

MARKER = "<!-- NoAr FFmpeg Specialist: integrated -->"

CLAUDE_SECTION = f"""## FFmpeg documentation specialist

{MARKER}

- For media changes involving FFmpeg APIs, codecs, protocols, containers, filtergraphs, timestamps, hardware acceleration or SCTE-35, use the separate `/noar-ffmpeg` skill or ask the non-voting `noar-ffmpeg-docs-specialist` for official, **narrowly scoped** references.
- The `noar-orchestrator` coordinates the expert's handoff to implementation and (where appropriate) `noar-integration-e2e-qa`; the existing `noar-media-engine-reviewer` remains one of the **five** backend council voters.
- Consult `docs/ai/ffmpeg/SOURCES.json` and use `python3 .claude/scripts/ffmpeg_docs.py topics '<query>'` before downloading only needed reference excerpts. Do not read all FFmpeg manuals into the model context.
- Version-check the native FFmpeg libraries/build alongside Rust `ffmpeg-next`; the online documentation may describe APIs unavailable in the installed build. FFmpeg documentation is not proof that a stream was tested.
- Do not start, stop, or reconfigure live playout as part of documentation research.

"""

ORCHESTRATOR_SECTION = f"""## FFmpeg documentation research delegation

{MARKER}

1. When a task requires **nontrivial** FFmpeg knowledge (libavcodec/libavformat/libavfilter/libavutil, demux/mux, SCTE-35, PTS/DTS, A/V sync, SRT, RTMP, HLS, hardware encoding), delegate a narrow, **read-only** reference question to `noar-ffmpeg-docs-specialist` before recommending version-sensitive changes.
2. Give the specialist the exact NoAr files/symbols, expected platform/build, precise question, and short answer budget. The specialist should return official FFmpeg sources, relevant API/option descriptions, version/capability caveats, and safe staging verification proposals; do not ask it to explore the whole repository.
3. Do not invoke the specialist for formatting, rename-only, or unrelated changes. Reuse findings only if their version and source revision remain applicable. The FFmpeg docs cache is a navigation aid, not an authority on local runtime behavior.
4. If nested `Agent` calls are unavailable, return the brief to the **parent Claude session** to invoke `/noar-ffmpeg` or the specialist. Never imply an agent was invoked if it wasn't.
5. After implementation, have `noar-integration-e2e-qa` check required media outputs **only in an approved isolated environment**. Send documentation impact to `noar-documentation-guardian`.
6. `noar-ffmpeg-docs-specialist` is **NON-VOTING** and must never be counted as a sixth backend reviewer. Preserve independent three-of-five council voting.

"""

SKILL_SECTION = f"""## 4a. Consult the independent FFmpeg Specialist when needed

{MARKER}

- For nontrivial FFmpeg media-engine features or FFmpeg C API/version questions, the main coordinator or Orchestrator may ask the read-only `noar-ffmpeg-docs-specialist` (Sonnet) to consult **only relevant** official documentation. It does **not** replace `noar-media-engine-reviewer` and does **not** vote.
- Independent direct command: `/noar-ffmpeg reference <topic>`, `/noar-ffmpeg analyze <path|topic>`, `/noar-ffmpeg verify`.
- Read `docs/ai/ffmpeg/README.md`, and route via `python3 .claude/scripts/ffmpeg_docs.py topics '<topic>'`, followed by bounded searches. For Rust `ffmpeg-next`, check the native FFmpeg build and version-specific Doxygen before applying a suggestion.
- Only spawn the specialist when the task actually needs outside FFmpeg expertise. Pass focused question and file paths, reuse existing map, return concise official sources and compatibility caveats.
- Any needed source modifications belong to the authorized coding agent. Tests belong to Integration QA on disposable staging; Documentation Guardian identifies docs impact.

"""

CHANGES = (
    ("CLAUDE.md", "## Change safety", CLAUDE_SECTION),
    (".claude/agents/noar-orchestrator.md", "## Cost and mutation boundaries", ORCHESTRATOR_SECTION),
    (".claude/skills/noar-intelligent-development/SKILL.md", "## 5. NoAr safety and targeted checks", SKILL_SECTION),
)


def plan(root: Path) -> list[tuple[Path, str | None, str]]:
    """Fully validate the three files before making ANY modification."""
    planned: list[tuple[Path, str | None, str]] = []
    for relative, anchor, section in CHANGES:
        path = root / relative
        if not path.is_file():
            raise ValueError(f"Missing existing NoAr file: {relative}")
        source = path.read_text(encoding="utf-8")
        if MARKER in source:
            planned.append((path, None, "already-integrated"))
            continue
        if source.count(anchor) != 1:
            raise ValueError(f"Expected exactly one anchor {anchor!r} in {relative}; found {source.count(anchor)}. No files changed.")
        updated = source.replace(anchor, section + anchor, 1)
        planned.append((path, updated, "needs-update"))
    return planned


def main(argv: list[str] | None = None) -> int:
    argp = argparse.ArgumentParser(description=__doc__)
    action = argp.add_mutually_exclusive_group(required=True)
    action.add_argument("--check", action="store_true", help="Preview integration status; no edits")
    action.add_argument("--apply", action="store_true", help="Patch three existing files additively")
    argp.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2], help="NoAr repo root (default: computed from script location)")
    args = argp.parse_args(argv)
    root = args.root.resolve()
    try:
        if not (root / ".claude/skills/noar-ffmpeg/SKILL.md").is_file():
            raise ValueError("Install the FFmpeg Specialist add-on files first; .claude/skills/noar-ffmpeg/SKILL.md not found.")
        if not (root / "docs/ai/ffmpeg/SOURCES.json").is_file():
            raise ValueError("docs/ai/ffmpeg/SOURCES.json missing.")
        entries = plan(root)
    except (OSError, UnicodeError, ValueError) as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 2
    for path, update, status in entries:
        print(f"{path.relative_to(root)}: {status}")
    if args.apply:
        for path, update, _ in entries:
            if update is not None:
                path.write_text(update, encoding="utf-8")
        print("Orchestrator + main skill + CLAUDE.md integrated. No git operation performed.")
    else:
        print("Preview only. Run --apply to integrate.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
