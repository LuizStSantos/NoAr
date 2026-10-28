# NoAr Audit Council

Exactly five FRONTEND voting reviewer agents:

- `noar-backend-reviewer`
- `noar-frontend-reviewer`
- `noar-visual-design-reviewer`
- `noar-accessibility-reviewer`
- `noar-functional-reviewer`

Exactly five BACKEND voting reviewer agents:

- `noar-backend-reviewer`
- `noar-media-engine-reviewer`
- `noar-security-reviewer`
- `noar-database-reviewer`
- `noar-reliability-reviewer`

Select `--council backend` when tallying the backend ballot files; frontend is the default. Do not mix the councils. Each council is five members, and only the selected council votes on an audit.

Simple exploration and map updates run via deterministic scripts. Documentation and responsive-layout helpers NEVER vote.

The lead relays peer criticisms between voting reviewers. Direct teammate messaging requires an actual supported Claude Code Agent Teams environment; never assume it works in Code Web. A ballot without a real reviewer result is invalid.

Use the templates in `.claude/audit/templates/` and the deterministic script in `.claude/scripts/` to tally; 3-of-5 determines confirmations and release-readiness advisory verdicts. A confirmed high/critical blocker prevents an approval.

**No release is authorized by a vote.** Runtime UI behavior must be tested in a safe browser/preview; never report click results from static code review alone.

## Cross-layer integration QA (v4)

After the two councils, the non-voting `noar-integration-e2e-qa` may run real browser/API/database/media checks **only on authorized isolated staging**. Read `.claude/audit/INTEGRATION_QUALITY_GATES.md`, `docs/ai/INTEGRATION_TEST_PLAN.md`, and `qa/e2e/README.md`. Its evidence is validated by `.claude/scripts/check_integration_readiness.py` and is independent of the council's 3-of-5 advisory votes. Absent staging or browser capabilities, record `NOT_RUN` or `NOT_AVAILABLE`.
