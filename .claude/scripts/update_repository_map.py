#!/usr/bin/env python3
"""Maintain a deterministic, incremental NoAr file and symbol index.

Examples (run from the repository root):
    python3 .claude/scripts/update_repository_map.py
    python3 .claude/scripts/update_repository_map.py --find schedule
    python3 .claude/scripts/update_repository_map.py --full

Only writes docs/ai/repository-map.md and docs/ai/symbol-index.json.
No external packages, network access or AI model calls are required.
"""
from __future__ import annotations

import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[2]
OUTPUT_DIR = ROOT / "docs" / "ai"
INDEX_PATH = OUTPUT_DIR / "symbol-index.json"
MAP_PATH = OUTPUT_DIR / "repository-map.md"
SCHEMA_VERSION = 2
MAX_SYMBOLS = 1000
MAX_PARSE_BYTES = 400_000

SKIP_DIRS = {
    ".git", ".venv", ".next", ".nuxt", ".output", ".cache",
    "target", "node_modules", "dist", "build", "coverage", "vendor",
    "__pycache__", "bin", "obj", "data", "tmp", "logs",
}
SKIP_EXTENSIONS = {
    ".mp4", ".mp3", ".mkv", ".mov", ".wav", ".png", ".jpg",
    ".jpeg", ".webp", ".gif", ".ico", ".pdf", ".woff", ".woff2",
    ".ttf", ".otf", ".zip", ".gz", ".tar", ".exe", ".dll",
    ".so", ".dylib", ".sqlite", ".db", ".pem", ".key", ".crt",
    ".p12", ".svg",
}
SECRET_FILE_NAMES = {
    ".env", ".env.local", ".env.production", ".env.development",
    "id_rsa", "id_ed25519", "credentials.json", "secrets.json",
}
LANGUAGES = {
    ".rs": "Rust", ".vue": "Vue", ".ts": "TypeScript", ".tsx": "TypeScript",
    ".js": "JavaScript", ".jsx": "JavaScript", ".py": "Python",
    ".go": "Go", ".java": "Java", ".sh": "Shell", ".sql": "SQL",
    ".json": "JSON", ".toml": "TOML", ".md": "Markdown",
    ".yml": "YAML", ".yaml": "YAML", ".html": "HTML", ".css": "CSS",
}
SYMBOL_PATTERNS = {
    "Rust": [
        re.compile(r"^\s*(?:(?:pub(?:\([^)]*\))?|async|unsafe|const|extern(?:\s+\"[^\"]+\")?)\s+)*(?:fn|struct|enum|trait|mod|type)\s+([A-Za-z_]\w*)\b"),
        re.compile(r"^\s*impl(?:<[^>]*>)?\s+([A-Za-z_]\w*)\b"),
    ],
    "TypeScript": [
        re.compile(r"^\s*(?:export\s+)?(?:default\s+)?(?:declare\s+)?(?:async\s+)?(?:function|class|interface|type|enum)\s+([\w$]+)\b"),
        re.compile(r"^\s*(?:export\s+)?(?:const|let)\s+([\w$]+)\s*(?::[^=]+)?=\s*(?:(?:async\s+)?(?:\([^)]*\)|[\w$]+)\s*=>|(?:defineStore|computed|ref|reactive|createRouter|createI18n|createApp|defineComponent|defineConfig)\s*\()"),
    ],
    "JavaScript": [
        re.compile(r"^\s*(?:export\s+)?(?:default\s+)?(?:async\s+)?(?:function|class)\s+([\w$]+)\b"),
        re.compile(r"^\s*(?:export\s+)?(?:const|let)\s+([\w$]+)\s*=\s*(?:(?:async\s+)?(?:\([^)]*\)|[\w$]+)\s*=>)"),
    ],
    "Vue": [
        re.compile(r"^\s*(?:export\s+)?(?:const|let)\s+([\w$]+)\s*[:=]"),
        re.compile(r"^\s*(?:async\s+)?function\s+([\w$]+)\s*\("),
    ],
    "Python": [
        re.compile(r"^\s*(?:async\s+)?(?:def|class)\s+([A-Za-z_]\w*)\b"),
    ],
    "Go": [
        re.compile(r"^\s*func\s+(?:\([^)]*\)\s*)?([A-Za-z_]\w*)\b"),
        re.compile(r"^\s*type\s+([A-Za-z_]\w*)\b"),
    ],
    "Java": [re.compile(r"\b(?:class|interface|enum|record)\s+([A-Za-z_]\w*)\b")],
}


def eligible(path: str) -> bool:
    rel = Path(path)
    parts = rel.parts
    if not parts or ".." in parts or rel.is_absolute():
        return False
    if any(part in SKIP_DIRS for part in parts[:-1]):
        return False
    if parts[:2] == ("docs", "ai"):
        return False
    if any(part.lower() in SECRET_FILE_NAMES for part in parts):
        return False
    if rel.suffix.lower() in SKIP_EXTENSIONS or rel.name.endswith(".lock"):
        return False
    if re.search(r"(^|[/._-])(secret|password|credential|private[-_]?key)([/._-]|$)", path.lower()):
        return False
    return True


def candidate_files() -> list[str]:
    try:
        completed = subprocess.run(
            ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"],
            cwd=ROOT, capture_output=True, check=True, timeout=20,
        )
        candidates = [p.decode("utf-8", "replace") for p in completed.stdout.split(b"\0") if p]
    except (OSError, subprocess.SubprocessError):
        candidates = [p.relative_to(ROOT).as_posix() for p in ROOT.rglob("*") if p.is_file()]
    return sorted({p for p in candidates if eligible(p) and (ROOT / p).is_file() and not (ROOT / p).is_symlink()})


def read_index() -> dict[str, dict]:
    try:
        data = json.loads(INDEX_PATH.read_text(encoding="utf-8"))
        if data.get("schema") == SCHEMA_VERSION and isinstance(data.get("files"), dict):
            return data["files"]
    except (OSError, ValueError, TypeError):
        pass
    return {}


def parse_symbols(text: str, language: str) -> list[dict]:
    patterns = SYMBOL_PATTERNS.get(language, [])
    found: list[dict] = []
    for line_number, line in enumerate(text.splitlines(), start=1):
        for pattern in patterns:
            match = pattern.search(line)
            if match:
                found.append({"name": match.group(1), "line": line_number})
                break
        if len(found) == MAX_SYMBOLS:
            break
    return found


def write_if_different(path: Path, text: str) -> bool:
    if path.exists() and path.read_text(encoding="utf-8") == text:
        return False
    path.write_text(text, encoding="utf-8")
    return True


def refresh(force: bool = False) -> None:
    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    previous = {} if force else read_index()
    entries: dict[str, dict] = {}
    changed = reused = 0
    for relative in candidate_files():
        file_path = ROOT / relative
        try:
            raw = file_path.read_bytes()
        except OSError:
            continue
        checksum = hashlib.sha256(raw).hexdigest()
        cached = previous.get(relative)
        if cached and cached.get("sha256") == checksum:
            entries[relative] = cached
            reused += 1
            continue
        language = LANGUAGES.get(file_path.suffix.lower(), "Other")
        if file_path.name in {"Dockerfile", "Makefile", "Justfile"}:
            language = "Build"
        symbols = []
        if len(raw) <= MAX_PARSE_BYTES and language in SYMBOL_PATTERNS:
            symbols = parse_symbols(raw.decode("utf-8", "replace"), language)
        entries[relative] = {
            "sha256": checksum, "language": language,
            "bytes": len(raw), "symbols": symbols,
        }
        changed += 1

    index_content = json.dumps({"schema": SCHEMA_VERSION, "files": entries}, indent=2, ensure_ascii=False) + "\n"
    index_written = write_if_different(INDEX_PATH, index_content)

    top_level = Counter(p.split("/")[0] if "/" in p else "(root)" for p in entries)
    languages = Counter(v["language"] for v in entries.values())
    overview = [
        "# NoAr Repository Map", "",
        "Machine-generated navigation index; verify behavior against current source code.",
        "", "## File counts by top-level directory", "",
        *(f"- `{directory}/`: {count} indexed files" for directory, count in sorted(top_level.items())),
        "", "## Languages and formats", "",
        *(f"- {language}: {count}" for language, count in languages.most_common()),
        "", "## Find files or symbols", "",
        "```sh", "python3 .claude/scripts/update_repository_map.py --find scheduler",
        "python3 .claude/scripts/update_repository_map.py --find scte35", "```", "",
        "The complete index is `docs/ai/symbol-index.json`. Never paste the complete JSON into the AI context.",
        "For architectural responsibilities, consult `docs/ai/architecture-overview.md`.",
        "For human-verified observations, consult `docs/ai/file-notes.md`.", "",
    ]
    map_written = write_if_different(MAP_PATH, "\n".join(overview))
    print(f"Indexed {len(entries)} files: {changed} new/changed, {reused} reused. "
          f"Files written: index={index_written}, map={map_written}.")


def find(term: str) -> None:
    if not INDEX_PATH.exists():
        raise SystemExit("Index not found. Run the script without --find first.")
    entries = read_index()
    needle = term.casefold()
    matches: list[str] = []
    for path, meta in sorted(entries.items()):
        if needle in path.casefold():
            matches.append(path)
        for symbol in meta.get("symbols", []):
            if needle in symbol["name"].casefold():
                matches.append(f"{path}:{symbol['line']}  {symbol['name']}")
        if len(matches) >= 25:
            break
    for match in matches[:25]:
        print(match)
    if not matches:
        print("No index matches. Try a different term or a narrowly scoped source search.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Build or query the NoAr repository map")
    options = parser.add_mutually_exclusive_group()
    options.add_argument("--find", metavar="TERM", help="Show up to 25 matching indexed paths/symbols")
    options.add_argument("--full", action="store_true", help="Discard the previous cache and rebuild")
    args = parser.parse_args()
    if args.find:
        # Refresh hashes and schema before querying. Reads source files locally and
        # reuses unchanged metadata; no network or model tokens are required.
        refresh()
        find(args.find)
    else:
        refresh(force=args.full)
