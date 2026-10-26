# NoAr full frontend audit — commit adc2061

Council: FRONTEND (5 voters). Lead facilitated; non-voting. Round B relayed by lead (no direct agent messaging).
Tally: `tally_audit_votes.py --council frontend` → readiness **BLOCKED** (deny 5/0/0), confirmed blocker F-003.

## Verification ledger
| Check | Status |
|---|---|
| npm ci | PASS |
| vue-tsc --build | PASS |
| vite build | PASS |
| eslint (no --fix) | PASS (0 issues) |
| Browser clicks / navigation | NOT_AVAILABLE |
| Dynamic a11y (axe, screen reader) | NOT_AVAILABLE |
| Visual screenshots | NOT_AVAILABLE |
| API ↔ DB integration, media output | NOT_RUN |

## Confirmed (≥3 confirm)
- F-001 HIGH — playlist.ts:83 checks `e.status` (undefined on AuthFetchError); failed load → empty playlist marked saved → save can overwrite the real day. 4 confirm.
- F-002 HIGH — auth.ts:235-249 logs out all tabs on any refresh failure incl. 5xx/network. 3 confirm.
- F-003 HIGH (proposer flagged blocker) — modals don't move/trap/restore focus; ScheduleDialog/LiveDialog lack Escape. 4 confirm. Note: 3 of the 4 confirmers stated the blocker flag is too strong; tally uses the finding's is_blocker field as filed.
- F-007 MEDIUM — obtainUuid `e.status` same root cause; SSE status stream can die silently. 3 confirm.
- F-010 MEDIUM — getPlaylist has no stale-response guard. 3 confirm.

## Dismissed (≥3 dismiss)
- F-004 — `<html lang>` IS updated via useHead (App.vue:83-88). 4 dismiss, incl. proposer.
- F-029 — server enforces roles; client gating is UX only. 3 dismiss.

## Unresolved (insufficient votes; mostly out-of-scope abstentions, none dismissed)
F-005, F-006, F-008, F-009, F-011..F-028 (except F-010), F-030, F-031. These remain single-reviewer static observations, not refuted.

## Root-cause note
F-001 and F-007 share one cause: callers read `e.status` while AuthFetchError exposes `e.response.status`.

## Follow-up fixes (after the vote)
The confirmed findings F-001, F-002 and F-007 were fixed in the commit that follows this audit; type-check, lint, prettier and build pass. Runtime behaviour of the fixes is NOT_VERIFIED (no browser/staging). The ballots above apply to commit adc2061 and were not re-run.
