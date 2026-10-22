# Roles, Voting and Escalation

## Coordinators (never vote)

- Main Claude session: user-facing coordinator and ultimate task dispatcher.
- `noar-orchestrator` (Sonnet): multi-stage coordination if the Agent tool can delegate; otherwise main Claude orchestrates directly.
- `noar-documentation-guardian` (Haiku): receives explicit compact handoffs, checks docs/changelog/map impact.
- `noar-integration-e2e-qa` (Sonnet): verifies cross-layer behavior using actual staging tests, independently of votes.

## Frontend Council (five voters, 3-of-5)

`noar-backend-reviewer`, `noar-frontend-reviewer`, `noar-visual-design-reviewer`, `noar-accessibility-reviewer`, `noar-functional-reviewer`.

## Backend Council (five voters, 3-of-5)

`noar-backend-reviewer`, `noar-media-engine-reviewer`, `noar-security-reviewer`, `noar-database-reviewer`, `noar-reliability-reviewer`.

The reused backend reviewer belongs to each council's separate audit; never count the same ballot twice within a council. The councils may run sequentially to save quota. No sixth vote. Collect independent findings, relay concise peer challenges, then collect final ballots. Deterministic tally: `.claude/scripts/tally_audit_votes.py`. A 3-of-5 verdict is a **recommendation only**.

## Runtime gate vs. votes

Council approvals are static/advisory and never mark browser clicks, API writes, SQLite persistence or media output as executed. The QA agent records independent PASS/FAIL/NOT_RUN evidence in `.claude/audit/templates/integration-evidence.example.json` format. Unverified critical flows block claims of production readiness regardless of any majority.

## Cost strategy

For trivial changes use the main session only. For documentation and map maintenance prefer Haiku. Use the specialist QA agent only when real integration testing was requested/needed. Reserve simultaneous five-agent councils for full audits or high-risk changes. Send only paths, hypotheses and short evidence summaries between agents.
