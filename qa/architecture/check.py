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
            print(f"{finding['path']}:{finding['line']}: {finding['rule']}")
        print(f"{result['status']}: architecture boundaries ({scanned} Rust files)")
    return int(bool(findings))


if __name__ == '__main__':
    raise SystemExit(main())
