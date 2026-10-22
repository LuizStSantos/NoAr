#!/usr/bin/env python3
"""Tally NoAr council ballots: five independent voters, threshold three."""
from __future__ import annotations
import argparse
import json
from collections import Counter
from pathlib import Path

# Frontend review roster is kept as the default for backward compatibility.
FRONTEND_VOTERS = (
    'noar-backend-reviewer',
    'noar-frontend-reviewer',
    'noar-visual-design-reviewer',
    'noar-accessibility-reviewer',
    'noar-functional-reviewer',
)
BACKEND_VOTERS = (
    'noar-backend-reviewer',
    'noar-media-engine-reviewer',
    'noar-security-reviewer',
    'noar-database-reviewer',
    'noar-reliability-reviewer',
)
COUNCILS = {'frontend': FRONTEND_VOTERS, 'backend': BACKEND_VOTERS}
VOTERS = FRONTEND_VOTERS
ISSUE_VOTES = {'confirm', 'dismiss', 'abstain'}
READINESS_VOTES = {'approve', 'deny', 'abstain'}
SEVERITIES = {'CRITICAL', 'HIGH', 'MEDIUM', 'LOW'}
VERIFICATIONS = {'VERIFIED_RUNTIME', 'VERIFIED_STATIC', 'NOT_VERIFIED'}


def validate_vote(node, allowed, location):
    if not isinstance(node, dict):
        raise ValueError(f'{location}: expected object containing vote and reason')
    vote = node.get('vote')
    reason = node.get('reason')
    if vote not in allowed:
        raise ValueError(f'{location}: unexpected vote {vote!r}')
    if not isinstance(reason, str) or not reason.strip():
        raise ValueError(f'{location}: non-empty reason required')
    return vote


def tally(findings_data, ballots, council='frontend'):
    if council not in COUNCILS:
        raise ValueError(f'Unknown council: {council}')
    voters = COUNCILS[council]
    if findings_data.get('council', council) != council:
        raise ValueError(f'Findings council and selected council do not match')
    items = findings_data.get('findings')
    if not isinstance(items, list):
        raise ValueError('findings: expected findings[]')
    if len(ballots) != 5 or set(ballots) != set(voters):
        raise ValueError(f'Exactly five distinct voter files are required for {council}: {voters}')
    ids = []
    for item in items:
        if not isinstance(item, dict):
            raise ValueError('Every finding must be an object')
        fid = item.get('id')
        if not isinstance(fid, str) or not fid:
            raise ValueError('Each finding needs a non-empty id')
        if fid in ids:
            raise ValueError(f'Duplicate finding ID: {fid}')
        ids.append(fid)
        if item.get('severity') not in SEVERITIES:
            raise ValueError(f'{fid}: severity must be CRITICAL/HIGH/MEDIUM/LOW')
        if item.get('verification') not in VERIFICATIONS:
            raise ValueError(f'{fid}: invalid verification status')
        if not isinstance(item.get('is_blocker'), bool):
            raise ValueError(f'{fid}: is_blocker must be true or false')
        if not isinstance(item.get('evidence'), list) or not item['evidence']:
            raise ValueError(f'{fid}: at least one evidence reference is required')
    per_finding = {fid: Counter() for fid in ids}
    overall = Counter()
    for voter in voters:
        ballot = ballots[voter]
        if ballot.get('agent') != voter:
            raise ValueError(f'{voter}: mismatched ballot agent identity')
        overall[validate_vote(ballot.get('readiness'), READINESS_VOTES, f'{voter} readiness')] += 1
        votes = ballot.get('findings')
        if not isinstance(votes, dict) or set(votes) != set(ids):
            raise ValueError(f'{voter}: expected a vote for each finding (abstain allowed)')
        for fid in ids:
            per_finding[fid][validate_vote(votes[fid], ISSUE_VOTES, f'{voter} {fid}')] += 1
    results = []
    for item in items:
        fid = item['id']
        c = per_finding[fid]
        decision = ('CONFIRMED' if c['confirm'] >= 3 else
                    'DISMISSED' if c['dismiss'] >= 3 else 'UNRESOLVED')
        results.append({'id': fid, 'title': item.get('title'),
                        'severity': item['severity'], 'is_blocker': item['is_blocker'],
                        'verification': item['verification'], 'decision': decision,
                        'votes': {v: c[v] for v in ('confirm', 'dismiss', 'abstain')}})
    confirmed_blockers = [x['id'] for x in results if x['decision'] == 'CONFIRMED'
                          and x['is_blocker'] and x['severity'] in ('HIGH', 'CRITICAL')]
    if confirmed_blockers:
        readiness = 'BLOCKED'
    elif overall['deny'] >= 3:
        readiness = 'DENIED'
    elif overall['approve'] >= 3:
        readiness = 'APPROVED_ADVISORY_ONLY'
    else:
        readiness = 'INCONCLUSIVE'
    return {'council': council, 'readiness': readiness, 'readiness_votes': {v: overall[v] for v in ('approve', 'deny', 'abstain')},
            'confirmed_blockers': confirmed_blockers, 'findings': results,
            'note': 'This audit result does not authorize merge, deploy or release. Runtime behavior requires actual tests.'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--council', choices=tuple(COUNCILS), default='frontend')
    parser.add_argument('--findings', type=Path, required=True)
    parser.add_argument('--votes-dir', type=Path, required=True)
    parser.add_argument('--output', type=Path, default=None)
    args = parser.parse_args()
    try:
        findings = json.loads(args.findings.read_text(encoding='utf-8'))
        files = list(args.votes_dir.glob('*.json'))
        if sorted(p.stem for p in files) != sorted(COUNCILS[args.council]):
            raise ValueError('Vote directory must contain exactly five .json ballots: ' + ', '.join(COUNCILS[args.council]))
        ballots = {p.stem: json.loads(p.read_text(encoding='utf-8')) for p in files}
        result = tally(findings, ballots, council=args.council)
    except (ValueError, OSError, json.JSONDecodeError) as error:
        parser.error(str(error))
    encoded = json.dumps(result, indent=2, ensure_ascii=False) + '\n'
    if args.output:
        args.output.write_text(encoded, encoding='utf-8')
    print(encoded, end='')


if __name__ == '__main__':
    main()
