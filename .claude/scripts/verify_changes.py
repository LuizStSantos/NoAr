#!/usr/bin/env python3
"""Select deterministic, non-production checks for a NoAr change.

Plan only by default:
    python3 .claude/scripts/verify_changes.py --base origin/main
    python3 .claude/scripts/verify_changes.py --files frontend/src/App.vue

Execute ONLY selected local checks when explicitly requested:
    python3 .claude/scripts/verify_changes.py --files backend/app/src/main.rs --run

No Docker containers are started, no tests target a live NoAr instance and
no production data or deployments are changed.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
PYTEST_CMD = [sys.executable, "-m", "unittest", "discover", "-s", ".claude/scripts", "-p", "test_*.py"]


def classify(paths: list[str]) -> dict[str, bool]:
    normalized = {p.strip().replace("\\", "/") for p in paths if p.strip()}
    rust = any(p.endswith(".rs") or p in {"Cargo.toml", "Cargo.lock"} or
               (p.startswith("backend/") and p.endswith("Cargo.toml")) for p in normalized)
    frontend = any(p.startswith("frontend/") or p in {"package.json", "package-lock.json",
                    "vite.config.ts", "tsconfig.json"} for p in normalized)
    docker = any(p in {"Dockerfile", "docker-compose.yml", ".dockerignore"} or
                 p.startswith(".github/workflows/docker") for p in normalized)
    migrations = any(p.startswith("migrations/") and p.endswith(".sql") for p in normalized)
    media = any(p.startswith("backend/engine/") or
                p.startswith("backend/app/src/player/") for p in normalized)
    toolkit = any(p.startswith(".claude/") or p.startswith("docs/ai/") or
                  p == "CLAUDE.md" for p in normalized)
    return dict(rust=rust, frontend=frontend, docker=docker, migrations=migrations,
                media=media, toolkit=toolkit)


def commands_for(paths: list[str]) -> tuple[list[dict], list[str]]:
    groups = classify(paths)
    checks: list[dict] = []
    risks: list[str] = []
    def add(name: str, command: list[str]) -> None:
        checks.append({"name": name, "command": command})
    if groups["rust"] or groups["migrations"]:
        add("rust-format", ["cargo", "fmt", "--all", "--", "--check"])
        add("rust-test", ["cargo", "test", "--no-default-features"])
        add("rust-clippy", ["cargo", "clippy", "--no-default-features", "--all-targets", "--", "--deny", "warnings"])
    if groups["frontend"]:
        add("frontend-build-typecheck", ["npm", "run", "build"])
        add("frontend-eslint", ["npx", "eslint", "--config", "frontend/eslint.config.ts", "frontend/src"])
    if groups["docker"]:
        add("docker-compose-config", ["docker", "compose", "config", "--quiet"])
        add("dockerfile-build-check", ["docker", "build", "--check", "."])
    if groups["toolkit"]:
        add("toolkit-unit-tests", PYTEST_CMD)
    if groups["migrations"]:
        risks.append("SQLite migration: only disposable DB upgrade/rollback and API persistence can verify runtime behavior.")
    if groups["media"]:
        risks.append("Media engine: synthetic-stream/FFprobe continuity, timing and recovery checks require authorized isolated staging.")
    if groups["frontend"]:
        risks.append("Interactive UI changes require browser evidence in authorized disposable staging.")
    if groups["docker"]:
        risks.append("Docker validation is not permission to build/publish/deploy; production updater and volume changes require review.")
    return checks, risks


def changed_paths(base: str) -> list[str]:
    command = ["git", "diff", "--name-only", "--diff-filter=ACMR", f"{base}...HEAD"]
    result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, check=False, timeout=20)
    if result.returncode:
        raise RuntimeError(f"Could not determine changes from {base}: {result.stderr.strip()}")
    return result.stdout.splitlines()


def execute(check: dict, timeout: int) -> dict:
    cmd = check["command"]
    if shutil.which(cmd[0]) is None:
        return {"name": check["name"], "status": "NOT_AVAILABLE", "reason": f"{cmd[0]} unavailable"}
    try:
        result = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True,
                                timeout=timeout, check=False)
    except subprocess.TimeoutExpired:
        return {"name": check["name"], "status": "FAIL", "reason": f"timeout after {timeout}s"}
    except OSError as exc:
        return {"name": check["name"], "status": "NOT_AVAILABLE", "reason": str(exc)}
    # Avoid storing logs or secrets in machine-readable output.
    return {"name": check["name"], "status": "PASS" if result.returncode == 0 else "FAIL",
            "returncode": result.returncode, "command": cmd,
            "summary": (result.stderr or result.stdout)[-1200:] if result.returncode else ""}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    origin = parser.add_mutually_exclusive_group(required=True)
    origin.add_argument("--base", help="Compare HEAD against a known base branch/commit")
    origin.add_argument("--files", nargs="+", help="Explicit changed repository paths")
    parser.add_argument("--run", action="store_true", help="Run selected local checks; default is plan only")
    parser.add_argument("--timeout", type=int, default=900, help="Maximum seconds per check (default 900)")
    args = parser.parse_args(argv)
    if not 1 <= args.timeout <= 3600:
        parser.error("--timeout must be in 1..3600")
    try:
        paths = args.files if args.files is not None else changed_paths(args.base)
    except RuntimeError as exc:
        parser.error(str(exc))
    checks, risks = commands_for(paths)
    results = [execute(c, args.timeout) if args.run else
               {"name": c["name"], "status": "NOT_RUN", "command": c["command"]}
               for c in checks]
    report = {"changed_paths": sorted(set(paths)), "checks": results,
              "manual_staging_gates": risks,
              "note": "PASS means the specific command exited successfully, never production readiness."}
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return 1 if any(c["status"] == "FAIL" for c in results) else 0


if __name__ == "__main__":
    raise SystemExit(main())
