#!/usr/bin/env python3
"""Validate an independent NoAr integration evidence manifest and emit gate readiness.

Statuses reflect *provided* evidence. The script cannot prove that tests really ran.
Exit 0: all PASS (advisory); 1: FAIL present or invalid evidence; 2: missing/not-run gates.
"""
from __future__ import annotations
import argparse
import json
from pathlib import Path

GATES = ('public_api', 'browser_navigation', 'authentication', 'api_database', 'scheduler', 'media_output', 'recovery', 'security')
STATUSES = {'PASS', 'FAIL', 'NOT_RUN', 'NOT_AVAILABLE'}


def check(manifest):
    if not isinstance(manifest, dict):
        raise ValueError('Expected a JSON object')
    if manifest.get('schema') != 1:
        raise ValueError('Expected schema 1')
    for key in ('repository', 'commit', 'environment'):
        if not isinstance(manifest.get(key), str) or not manifest[key].strip():
            raise ValueError(f'Nonempty {key} is required')
    if manifest['environment'].lower() in ('production', 'prod'):
        raise ValueError('Production integration execution is out of scope')
    gates = manifest.get('gates')
    if not isinstance(gates, dict):
        raise ValueError('Expected gates object')
    if set(gates) != set(GATES):
        raise ValueError('Expected exactly these gates: ' + ', '.join(GATES))
    counts = {k:0 for k in sorted(STATUSES)}
    for name in GATES:
        g = gates[name]
        if not isinstance(g, dict) or g.get('status') not in STATUSES:
            raise ValueError(f'{name}: invalid gate status')
        if not isinstance(g.get('reason'), str) or not g['reason'].strip():
            raise ValueError(f'{name}: nonempty reason required')
        if not isinstance(g.get('evidence'), list) or not all(isinstance(e, str) and e.strip() for e in g['evidence']):
            raise ValueError(f'{name}: evidence must be a list of nonempty references')
        if g['status'] in ('PASS', 'FAIL') and not g['evidence']:
            raise ValueError(f'{name}: PASS/FAIL requires at least one evidence reference')
        counts[g['status']] += 1
    decision = 'NOT_READY' if counts['FAIL'] else 'NEEDS_TESTING' if counts['NOT_RUN'] or counts['NOT_AVAILABLE'] else 'READY_FOR_REVIEW'
    return {'decision': decision, 'counts': counts, 'missing': [name for name in GATES if gates[name]['status'] != 'PASS'],
            'note': 'Advisory only. Evidence is supplied by tests/operators; this script cannot verify execution authenticity or authorize a deployment.'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--evidence', type=Path, required=True)
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    try:
        data = json.loads(args.evidence.read_text(encoding='utf-8'))
        result = check(data)
    except (OSError, ValueError, json.JSONDecodeError) as exc:
        parser.error(str(exc))
    message = json.dumps(result, indent=2) + '\n'
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(message, encoding='utf-8')
    print(message, end='')
    return {'READY_FOR_REVIEW':0, 'NOT_READY':1, 'NEEDS_TESTING':2}[result['decision']]


if __name__ == '__main__':
    raise SystemExit(main())
