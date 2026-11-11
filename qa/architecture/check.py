#!/usr/bin/env python3
"""Check source-level layer boundaries without requiring native media libraries.

This is a deliberately small architectural tripwire, not a Rust parser or a
security audit. Cargo and runtime tests remain mandatory. Test modules may build
SQL fixtures; production database operations belong in db/handles.
"""
import argparse
import json
from pathlib import Path
import re


# Mask comments and quoted text so documentation, SQL strings and test fixtures
# do not accidentally count as dependency edges. Preserve line numbers.
NON_CODE = re.compile(r'''r(\#*)".*?"\1|"(?:\\.|[^"\\])*"|'(?:\\(?:u\{[0-9a-fA-F]+\}|.)|[^'\\])'|//[^\n]*|/\*.*?\*/''', re.DOTALL)
TEST_ITEM = re.compile(r'#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]')
SQL_QUERY = re.compile(r'\bsqlx\s*::\s*(?:query(?:_as|_scalar)?|raw_sql)\s*(?:[!(]|::)|\buse\s+sqlx\s*::[^;]*\b(?:query(?:_as|_scalar)?|raw_sql)\b', re.DOTALL)


def production_code(source: str) -> str:
    def mask(text):
        return re.sub(r'[^\n]', ' ', text)

    source = NON_CODE.sub(lambda match: mask(match.group()), source)
    offset = 0
    while match := TEST_ITEM.search(source, offset):
        end = match.end()
        depth = 0
        # Exclude just this cfg-gated item. Production definitions may follow
        # test imports/statics or another test module in the same source file.
        while end < len(source):
            character = source[end]
            end += 1
            if character == '{':
                depth += 1
            elif character == '}':
                depth -= 1
                if depth == 0:
                    break
            elif character == ';' and depth == 0:
                break
        source = source[:match.start()] + mask(source[match.start():end]) + source[end:]
        offset = end

    return source


# These panels receive data and emit commands. Loading and mutations belong to
# their page/composable owners; importing a store here silently creates another
# state owner. This narrow check complements type checking and browser tests.
PRESENTATION_PANELS = (
    'components/player/playlist/*.vue',
    'components/schedule/ScheduleTimeline.vue',
    'components/schedule/ScheduleEventList.vue',
    'components/schedule/ScheduleGraphicsList.vue',
    'components/storage/StorageImageGrid.vue',
    'components/storage/StorageFileDialogs.vue',
)
PANEL_DEPENDENCY = re.compile(r"(?:from\s*|import\s*)['\"](@/stores/[^'\"]+|@/composables/authFetch)['\"]")
JS_STRINGS = re.compile(r"'(?:\\.|[^'\\])*'|`(?:\\.|[^`\\])*`", re.DOTALL)
PANEL_NETWORK = re.compile(r'\b(?:fetch|XMLHttpRequest|WebSocket)\s*\(')


def panel_violations(root: Path) -> list[dict]:
    findings = []
    frontend = root / 'frontend/src'
    for pattern in PRESENTATION_PANELS:
        for path in sorted(frontend.glob(pattern)):
            source = path.read_text().split('</script>', 1)[0]
            # Preserve import strings while excluding comments from edge checks.
            imports = NON_CODE.sub(lambda match: re.sub(r'[^\n]', ' ', match.group())
                                   if match.group().startswith(('//', '/*')) else match.group(), source)
            network_code = JS_STRINGS.sub(
                lambda match: re.sub(r'[^\n]', ' ', match.group()), production_code(source))
            for rule, code, expression in (
                ('panel-depends-on-data-owner', imports, PANEL_DEPENDENCY),
                ('panel-opens-network', network_code, PANEL_NETWORK),
            ):
                for match in expression.finditer(code):
                    findings.append({'rule': rule, 'path': path.relative_to(root).as_posix(),
                                     'line': code.count('\n', 0, match.start()) + 1})
    return findings


# Where the Rust version is set: the version is the first group.
TOOLCHAIN_SOURCES = (
    ('rust-toolchain.toml', re.compile(r'^channel\s*=\s*"([^"]+)"', re.MULTILINE)),
    ('Dockerfile', re.compile(r'^ARG RUST_IMAGE=rust:([0-9.]+)-', re.MULTILINE)),
    ('.github/workflows/validate.yml', re.compile(r'^\s*container:\s*rust:([0-9.]+)-', re.MULTILINE)),
)


def toolchain_violations(root: Path) -> list[dict]:
    """Local builds, CI and the image must use one Rust: Clippy differs between versions."""
    if not (root / 'rust-toolchain.toml').exists():
        return []
    found = []
    for name, pattern in TOOLCHAIN_SOURCES:
        path = root / name
        text = path.read_text() if path.exists() else ''
        match = pattern.search(text)
        if not match:
            return [{'rule': 'rust-toolchain-not-pinned', 'path': name, 'line': 1}]
        found.append((name, match.group(1), text.count('\n', 0, match.start()) + 1))
    pinned = found[0][1]
    return [{'rule': 'rust-toolchain-mismatch', 'path': name, 'line': line,
             'detail': f'Rust {version}, rust-toolchain.toml pins {pinned}'}
            for name, version, line in found[1:] if version != pinned]


def violations(root: Path) -> tuple[list[dict], int]:
    findings, scanned = [], 0
    for crate in ('app', 'engine'):
        for path in sorted((root / 'backend' / crate / 'src').rglob('*.rs')):
            scanned += 1
            relative = path.relative_to(root).as_posix()
            code = production_code(path.read_text())
            rules = []
            if crate == 'app':
                if path.relative_to(root / 'backend/app/src').parts[0] in ('utils', 'player', 'graphics', 'graphics.rs'):
                    rules.append(('service-depends-on-http', re.compile(r'\bapi\s*::')))
                if not relative.startswith('backend/app/src/db/') and path.name != 'test_support.rs':
                    rules.append(('sql-outside-data-layer', SQL_QUERY))
            else:
                rules.append(('engine-depends-on-application', re.compile(r'\b(?:noar_app|axum|sqlx)\s*::')))
            for rule, pattern in rules:
                for match in pattern.finditer(code):
                    findings.append({'rule': rule, 'path': relative, 'line': code.count('\n', 0, match.start()) + 1})
    findings.extend(panel_violations(root))
    findings.extend(toolchain_violations(root))

    return findings, scanned


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument('--json', action='store_true')
    args = parser.parse_args()
    findings, scanned = violations(args.root)
    result = {'status': 'FAIL' if findings else 'PASS', 'rust_files_scanned': scanned, 'findings': findings}
    if args.json:
        print(json.dumps(result, indent=2))
    else:
        for finding in findings:
            detail = f" ({finding['detail']})" if finding.get('detail') else ''
            print(f"{finding['path']}:{finding['line']}: {finding['rule']}{detail}")
        print(f"{result['status']}: architecture boundaries ({scanned} Rust files)")
    return int(bool(findings))


if __name__ == '__main__':
    raise SystemExit(main())
