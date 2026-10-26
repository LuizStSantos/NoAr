#!/usr/bin/env python3
"""Prepare a NoAr release: choose the next SemVer, date the changelog, set every version.

Follows docs/releasing.md:

- MAJOR when the Unreleased notes have a `### Removed` or `### Breaking` section,
  or an entry marked `**Breaking**`;
- MINOR when they have an `### Added` section or a new migration was added since
  the last release;
- PATCH otherwise (fixes only).

`--check` only prints the plan as JSON. Without it the script rewrites
CHANGELOG.md, Cargo.toml, Cargo.lock, package.json and package-lock.json.
It never commits, tags or pushes.
"""
from __future__ import annotations

import argparse
import datetime as dt
import json
import re
import subprocess
import sys
from pathlib import Path

VERSION_RE = re.compile(r'^(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?$')
UNRELEASED_RE = re.compile(r'^## \[Unreleased\][ \t]*$', re.MULTILINE)
SECTION_RE = re.compile(r'^## \[', re.MULTILINE)
WORKSPACE_MEMBERS = ('noar', 'noar-engine')


class ReleaseError(Exception):
    pass


def parse_version(text: str) -> tuple[int, int, int, str | None]:
    match = VERSION_RE.match(text.strip())
    if not match:
        raise ReleaseError(f'not a MAJOR.MINOR.PATCH version: {text!r}')
    major, minor, patch, pre = match.groups()
    return int(major), int(minor), int(patch), pre


def current_version(root: Path) -> str:
    cargo = (root / 'Cargo.toml').read_text(encoding='utf-8')
    section = cargo.split('[workspace.package]', 1)
    if len(section) != 2:
        raise ReleaseError('Cargo.toml has no [workspace.package] section')
    match = re.search(r'^version = "([^"]+)"', section[1], re.MULTILINE)
    if not match:
        raise ReleaseError('Cargo.toml [workspace.package] has no version')
    return match.group(1)


def unreleased_notes(changelog: str) -> str:
    start = UNRELEASED_RE.search(changelog)
    if not start:
        raise ReleaseError('CHANGELOG.md has no "## [Unreleased]" section')
    rest = changelog[start.end():]
    end = SECTION_RE.search(rest)
    link = re.search(r'^\[[^\]]+\]: ', rest, re.MULTILINE)
    stops = [m.start() for m in (end, link) if m]
    return rest[:min(stops)] if stops else rest


def has_entries(notes: str) -> bool:
    return any(line.lstrip().startswith(('- ', '* ')) for line in notes.splitlines())


def bump_kind(notes: str, new_migrations: int) -> tuple[str, list[str]]:
    reasons: list[str] = []
    headings = {h.strip().lower() for h in re.findall(r'^### (.+)$', notes, re.MULTILINE)}

    if 'removed' in headings or 'breaking' in headings or '**breaking**' in notes.lower():
        reasons.append('the notes list a removal or a breaking change')
        return 'major', reasons
    if 'added' in headings:
        reasons.append('the notes have an "Added" section')
    if new_migrations:
        reasons.append(f'{new_migrations} new database migration(s) since the last release')
    if reasons:
        return 'minor', reasons
    reasons.append('only fixes and changes')
    return 'patch', reasons


def next_version(version: str, kind: str, pre: str | None = None) -> str:
    major, minor, patch, current_pre = parse_version(version)
    if current_pre and kind != 'set':
        # A release candidate is finished by releasing its own number.
        base = f'{major}.{minor}.{patch}'
    elif kind == 'major':
        base = f'{major + 1}.0.0'
    elif kind == 'minor':
        base = f'{major}.{minor + 1}.0'
    elif kind == 'patch':
        base = f'{major}.{minor}.{patch + 1}'
    else:
        raise ReleaseError(f'unknown bump {kind!r}')
    return f'{base}-{pre}' if pre else base


def count_new_migrations(root: Path, version: str) -> int:
    """New files in migrations/ since the tag of `version`; 0 when git cannot tell."""
    try:
        out = subprocess.run(
            ['git', 'diff', '--name-only', '--diff-filter=A', f'v{version}', 'HEAD', '--', 'migrations'],
            cwd=root, capture_output=True, text=True, check=True,
        ).stdout
    except (OSError, subprocess.CalledProcessError):
        return 0
    return sum(1 for line in out.splitlines() if line.endswith('.sql'))


def date_changelog(changelog: str, version: str, previous: str, date: str, repo_url: str) -> str:
    start = UNRELEASED_RE.search(changelog)
    if not start:
        raise ReleaseError('CHANGELOG.md has no "## [Unreleased]" section')
    if re.search(rf'^## \[{re.escape(version)}\]', changelog, re.MULTILINE):
        raise ReleaseError(f'CHANGELOG.md already has a section for {version}')

    dated = changelog[:start.end()] + f'\n\n## [{version}] - {date}' + changelog[start.end():]
    unreleased_link = f'[Unreleased]: {repo_url}/compare/v{version}...HEAD'
    version_link = f'[{version}]: {repo_url}/compare/v{previous}...v{version}'

    if re.search(r'^\[Unreleased\]: .*$', dated, re.MULTILINE):
        dated = re.sub(r'^\[Unreleased\]: .*$', f'{unreleased_link}\n{version_link}', dated, count=1, flags=re.MULTILINE)
    else:
        dated = dated.rstrip('\n') + f'\n\n{unreleased_link}\n{version_link}\n'
    return re.sub(r'\n{3,}(## \[)', r'\n\n\1', dated)


def set_cargo_version(text: str, version: str) -> str:
    head, sep, tail = text.partition('[workspace.package]')
    if not sep:
        raise ReleaseError('Cargo.toml has no [workspace.package] section')
    tail, count = re.subn(r'^version = "[^"]+"', f'version = "{version}"', tail, count=1, flags=re.MULTILINE)
    if count != 1:
        raise ReleaseError('Cargo.toml [workspace.package] has no version')
    return head + sep + tail


def set_lock_versions(text: str, version: str) -> str:
    for name in WORKSPACE_MEMBERS:
        pattern = rf'(\[\[package\]\]\nname = "{re.escape(name)}"\nversion = ")[^"]+(")'
        text, count = re.subn(pattern, rf'\g<1>{version}\g<2>', text)
        if count != 1:
            raise ReleaseError(f'Cargo.lock has no single entry for {name}')
    return text


def set_package_json(text: str, version: str) -> str:
    data = json.loads(text)
    data['version'] = version
    return json.dumps(data, indent=2, ensure_ascii=False) + '\n'


def set_package_lock(text: str, version: str) -> str:
    data = json.loads(text)
    data['version'] = version
    if '' in data.get('packages', {}):
        data['packages']['']['version'] = version
    return json.dumps(data, indent=2, ensure_ascii=False) + '\n'


def repo_url(cargo: str) -> str:
    match = re.search(r'^repository = "([^"]+)"', cargo, re.MULTILINE)
    return match.group(1).rstrip('/') if match else 'https://github.com/LuizStSantos/NoAr'


def plan(root: Path, bump: str, pre: str | None, new_migrations: int | None) -> dict:
    changelog = (root / 'CHANGELOG.md').read_text(encoding='utf-8')
    notes = unreleased_notes(changelog)
    if not has_entries(notes):
        raise ReleaseError('nothing to release: the Unreleased section of CHANGELOG.md is empty')

    version = current_version(root)
    migrations = count_new_migrations(root, version) if new_migrations is None else new_migrations
    kind, reasons = bump_kind(notes, migrations)
    if bump != 'auto':
        reasons = [f'{bump} asked for (the notes suggest {kind})']
        kind = bump

    return {
        'current': version,
        'next': next_version(version, kind, pre),
        'bump': kind,
        'reasons': reasons,
        'new_migrations': migrations,
    }


def apply(root: Path, current: str, version: str, date: str) -> list[str]:
    parse_version(version)
    cargo_path = root / 'Cargo.toml'
    cargo = cargo_path.read_text(encoding='utf-8')
    url = repo_url(cargo)
    changes = {
        'CHANGELOG.md': date_changelog((root / 'CHANGELOG.md').read_text(encoding='utf-8'), version, current, date, url),
        'Cargo.toml': set_cargo_version(cargo, version),
        'Cargo.lock': set_lock_versions((root / 'Cargo.lock').read_text(encoding='utf-8'), version),
        'package.json': set_package_json((root / 'package.json').read_text(encoding='utf-8'), version),
        'package-lock.json': set_package_lock((root / 'package-lock.json').read_text(encoding='utf-8'), version),
    }
    # Everything is worked out before anything is written.
    for name, text in changes.items():
        (root / name).write_text(text, encoding='utf-8')
    return list(changes)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument('--bump', choices=('auto', 'patch', 'minor', 'major'), default='auto')
    parser.add_argument('--pre', help='release candidate suffix, such as rc.1')
    parser.add_argument('--version', help='use this exact version instead of computing one')
    parser.add_argument('--date', default=dt.date.today().isoformat())
    parser.add_argument('--new-migrations', type=int, help='override the git count of new migrations')
    parser.add_argument('--check', action='store_true', help='print the plan only')
    args = parser.parse_args(argv)

    try:
        result = plan(args.root, args.bump, args.pre, args.new_migrations)
        if args.version:
            parse_version(args.version)
            result['next'] = args.version
            result['reasons'] = [f'{args.version} asked for']
        if not args.check:
            result['changed'] = apply(args.root, result['current'], result['next'], args.date)
    except ReleaseError as error:
        print(json.dumps({'error': str(error)}), file=sys.stderr)
        return 2

    print(json.dumps(result, indent=2))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
