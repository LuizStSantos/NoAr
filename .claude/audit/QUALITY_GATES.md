# NoAr verification and evidence gates

These are independent of the advisory 3/5 council vote. A majority of reviewers **does not establish runtime correctness**.

## Mandatory completion ledger

Report `PASS`, `FAIL`, `NOT_RUN`, or `NOT_AVAILABLE` for applicable checks, using actual command or browser evidence, never invented results.

| Change area | Minimum relevant evidence |
| --- | --- |
| Rust source | Targeted Rust tests, formatting, Clippy as applicable; identify checked Cargo features |
| Vue/TypeScript | Type check, linter, affected build/tests; identify untested browsers |
| UI interactions | Runtime clicks and navigation with nonproduction data; otherwise `NOT_VERIFIED` |
| Accessibility | Keyboard focus, control names and errors; dynamic validation only with actual browser/accessibility tooling |
| Playback, HLS, SRT, RTMP, SCTE-35 | Real media/stream tests in isolated environment before claiming on-air readiness |
| DB migrations | Upgrade/backup and reversible migration impacts; never modify released migrations in place |
| Release, image, updater | Version-tag-changelog consistency; protected main, workflows and pinned production images review |
| Documentation | Documentation Guardian has reviewed changed behavior; output is `UPDATED`, `NO_UPDATE_REQUIRED` or `FOLLOW_UP` |

## Stop conditions

- Confirmed critical or high blocking finding.
- Failed relevant tests not explained or resolved.
- Suspected security or authorization regression.
- Unverified change that may interrupt 24/7 broadcast output.
- Potential destructive migration or unrecoverable update without explicit safeguards.

If any stop condition is met, label the handoff `NOT_READY`. The council may still finish and tally advisory votes, but **no ready-to-ship assertion is allowed**. If no runtime was available, label relevant behavioral claims `NOT_VERIFIED`; request staging verification instead of pretending clicks worked.

## Council voting

Exactly five reviewers vote independently; the Orchestrator and Documentation Guardian do not vote. `tally_audit_votes.py` validates the five ballot files and counts three approvals/denials. Reports distinguish:

- `readiness_vote`: what the reviewers concluded.
- `verification_gate`: what was actually tested and remains blocking.
- `overall`: `NOT_READY`, `NEEDS_TESTING`, or `ADVISORY_APPROVAL`.

Never convert an advisory verdict into permission to push, merge, tag, deploy or control live output.

## Cross-layer E2E gate (v4)

Use `.claude/audit/INTEGRATION_QUALITY_GATES.md` and the independent `noar-integration-e2e-qa` agent to record real results. The five-agent council vote never replaces these gate checks. `READY_FOR_REVIEW` is conditional and requires human approval; it is not automatic deployment permission.
