#!/usr/bin/env python3
"""Identify candidate NoAr docs impacted by changed paths, without reading source.

The suggestions are conservative pointers, not evidence of a behavior change.
"""
from __future__ import annotations

import argparse
import json
import subprocess
from pathlib import Path

DOCS = {
    'root': ('README.md', 'docs/README.md', 'docs/ai/architecture-overview.md'),
    'backend': ('docs/developer.md', 'docs/ai/backend.md'),
    'engine': ('docs/developer.md', 'docs/output.md', 'docs/ai/engine.md'),
    'frontend': ('frontend/README.md', 'docs/ai/frontend.md'),
    'api': ('docs/api.md',),
    'streaming': ('docs/output.md', 'docs/live_ingest.md', 'docs/scheduler.md'),
    'docs': ('docs/README.md',),
    'release': ('CHANGELOG.md', 'docs/releasing.md', 'docs/docker.md'),
    'deployment': ('docs/docker.md', 'README.md'),
    'database': ('docs/developer.md', 'CHANGELOG.md', 'docs/ai/backend.md'),
    'design': ('docs/ai/frontend.md',),
    'tests': ('docs/developer.md',),
    'guide': ('docs/xmltv.md',),
    'map': ('docs/ai/repository-map.md', 'docs/ai/symbol-index.json'),
}


def candidate_categories(path: str) -> set[str]:
    categories = set()
    if path.startswith('backend/app/'):
        categories.add('backend')
    if path.startswith('backend/engine/'):
        categories.add('engine')
    if path.startswith('frontend/'):
        categories.add('frontend')
    if path.startswith('backend/app/src/api/'):
        categories.add('api')
    if any(s in path for s in ('/player/', '/output/', '/input/', '/schedule/', '/scte35', 'compositor')):
        categories.add('streaming')
    if any(s in path for s in ('xmltv', '/nfo', 'epg')):
        categories.add('guide')
    if path.startswith('migrations/') or '/db/' in path:
        categories.add('database')
    if path.startswith('.github/workflows/') or path in ('Cargo.toml', 'Cargo.lock', 'CHANGELOG.md', 'package.json', 'package-lock.json'):
        categories.add('release')
    if path in ('Dockerfile', 'docker-compose.yml', '.dockerignore'):
        categories.add('deployment')
    if path.startswith('frontend/src/components/') or path.startswith('frontend/src/views/'):
        categories.add('design')
    if '/tests/' in path or path.endswith(('.test.ts', '.spec.ts')):
        categories.add('tests')
    if path.startswith('docs/') or path == 'README.md':
        categories.add('docs')
    if path.endswith(('.rs', '.ts', '.vue', '.sql', '.json', '.toml', '.yml', '.yaml')):
        categories.add('map')
    return categories


def impact(paths: list[str]) -> dict:
    normalized = sorted({p.strip().replace('\\', '/') for p in paths if p.strip()})
    # Don't trigger a cycle by recommending an updated map from map-only changes.
    meaningful = [p for p in normalized if not p.startswith('docs/ai/') and not p.startswith('.claude/')]
    category_sources: dict[str, list[str]] = {}
    for path in meaningful:
        for category in candidate_categories(path):
            category_sources.setdefault(category, []).append(path)
    doc_sources: dict[str, set[str]] = {}
    for category, files in category_sources.items():
        for doc in DOCS[category]:
            doc_sources.setdefault(doc, set()).update(files)
    return {
        'changed_files': normalized,
        'documentation_candidates': [
            {'path': doc, 'triggered_by': sorted(files), 'status': 'REQUIRES_SEMANTIC_REVIEW'}
            for doc, files in sorted(doc_sources.items())
        ],
        'note': 'Path-based candidates only. Verify behavior and existing docs before editing. No automatic documentation changes.'
    }


def git_files(base: str | None) -> list[str]:
    commands = []
    if base:
        commands.append(['git', 'diff', '--name-only', '--diff-filter=ACMR', f'{base}...HEAD'])
    commands += [
        ['git', 'diff', '--name-only', '--diff-filter=ACMR'],
        ['git', 'diff', '--cached', '--name-only', '--diff-filter=ACMR'],
        ['git', 'ls-files', '--others', '--exclude-standard'],
    ]
    paths = []
    for cmd in commands:
        run = subprocess.run(cmd, text=True, capture_output=True, check=False)
        if run.returncode:
            raise RuntimeError(f'{" ".join(cmd)} failed: {run.stderr.strip()}')
        paths += run.stdout.splitlines()
    return paths


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    source = parser.add_mutually_exclusive_group()
    source.add_argument('--base', help='Commit or branch used as the audit base')
    source.add_argument('--files', nargs='+', help='Explicit paths, suitable for cloud agent handoffs')
    parser.add_argument('--output', type=Path, help='Write the report to a JSON file')
    args = parser.parse_args()
    try:
        result = impact(args.files if args.files is not None else git_files(args.base))
    except RuntimeError as error:
        parser.error(str(error))
    payload = json.dumps(result, ensure_ascii=False, indent=2) + '\n'
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(payload, encoding='utf-8')
    print(payload, end='')


if __name__ == '__main__':
    main()
