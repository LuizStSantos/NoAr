# Frontend code council — Round B peer challenge

Reviewer: `noar-frontend-reviewer`, independent read-only FRONTEND council voter.
Revision: `f925f608` plus the reviewed worktree fixes. No product edits, builds,
servers, browser sessions, Git mutations or ballot in this task.

Read AGENTS.md, CLAUDE.md, the reviewer definition, both quality-gate documents,
the supplied Round A report, peer digest and deduplicated F-001..F-012 list.
Inspected the current routing/Composer state changes and their supporting API,
library, channel redirect and canvas changes. The runtime skill was read and
selected environment status was connected; no network or installation work.

## Original findings and proposed fixes

- **F-001 / FC-01:** The global editor guard runs before `configInit`, and the
  header selector restores the confirmed selected channel after navigation.
  This addresses the original ordering failure in source. The dirty decision
  no longer skips a save merely because it is busy.
- **F-002 / FC-02:** The save captures channel, document ID, role, input and
  generation. Its API is bound to that channel, and stale responses are ignored
  before publishing state or assignment/navigation continuations. This addresses
  the original save ownership failure. This guarantee is not yet applied to all
  other asynchronous authoring operations; see FC-B2.
- **F-003 / FC-03:** Native disabled/inert fieldset covers name, group, assignment,
  field mapping and canvas controls. This closes the former keyboard gap during
  save. Controlled internal operations must not call a handler that rejects their
  own busy state; see FC-B1.
- **F-004 / FC-04:** Channel-editor lookup captures route/channel/kind and is
  invalidated on unmount. Obsolete route replacement is guarded in source.
- **F-005 / FC-05:** The polling request includes temporary on-air state and uses
  the same state generation as bindings. Unmount invalidates it. Original stale
  temporary badges are addressed in source; independent response-ordering of
  multiple overlapping polls remains a general eventual-consistency limitation,
  not a newly confirmed data-loss finding.
- **First-save peer objection:** The explicit save-owned URL replacement now
  preserves the editor generation. The matching watcher skips reload only for
  the just-saved clean document, allowing `finally` to clear busy. This fixes the
  identified permanent-lock path in source; the first browser suite also passed
  its new first-save → edit → second-save regression.
- **F-010 cross-discipline observation:** Explicit first-image IDs remove the
  ambiguous null reset. Mapping pruning examines each layer independently, so a
  shared placeholder elsewhere no longer protects an obsolete former-layer
  mapping. Persistence must still be exercised in the browser; that focused
  regression timed out in the first suite and is under investigation.
- **F-011 cross-layer observation:** Atomic optional assignment on existing PUT
  validates the effective role and updates saved content/binding in one database
  transaction. The new-template POST-then-assign path remains non-atomic; see
  FC-B3 below. Do not describe all Composer role saves as atomic yet.
- **F-006..F-009 / F-012:** Canvas handles, pressed selection/filter semantics,
  font focus and all-out with no active temporary model have corresponding
  changes. The first suite reports the badge/pending all-out regression passing,
  but its handle-size assertion failing; visual/accessibility reviewers and the
  implementation helper own the focused correction. No static claim substitutes
  for that failed geometry check.

## Residual findings relayed to lead

### FC-B1 — MEDIUM — Restore logo placement calls a handler that rejects its own busy state

**VERIFIED_STATIC**, corroborated by the existing operator test failing in the
first parent-owned browser suite. References at review time:
`frontend/src/components/graphics/GraphicsComposer.vue:570`, `:588`, `:173`.

`restoreLogoPlacement` sets `busy = true`, decodes the image and calls
`updateSelectedLayer` to apply the natural size/corner. The new handler returns
immediately while `busy` is true. The restore button therefore completes without
changing geometry. The first suite's operator restore test observed width
remaining `100`, matching this trace.

Fix direction: apply the operation's authorized internal geometry change under
its captured document/channel/generation/layer identity, preserving undo and
scale base; keep the external editable handler locked. Re-run the actual
restore/default-corner/undo operator regression. Sent to lead and implementation
helper before report completion.

### FC-B2 — MEDIUM — Font/image authoring continuations remain outside the document generation boundary

**VERIFIED_STATIC**, runtime reproduction **NOT_RUN**. References:
`GraphicsComposer.vue:460..493`, `:496..559`, `:562..599`, `:633..655`.

Font upload captures version/channel/layer, but after its awaits it unconditionally
pushes the old asset into `uploadedFonts`, and unconditionally writes the error
and `fontUploading` flag. Switching to another channel/document while the upload
is delayed can populate the new channel's font selector with an asset belonging
to the old channel. Server ownership checks should reject its use; they do not
prevent this incorrect authoring state. Image upload/pick and placement repair
also clear `busy` without checking their generation; an older completion can
unlock a newer load. Placement repair checks only the selected layer ID, which
is not unique across documents for converted `image` layers.

Fix direction: bind all asynchronous authoring operations to captured channel,
document generation and exact layer identity, including error/status/finally
publication; initialize operation flags explicitly when loading a different
document. Add delayed upload → navigation → completion coverage. No runtime
claim or cross-channel authorization bypass is asserted.

### FC-B3 — MEDIUM — Failed assignment after new-model creation leaves a partially saved draft

**VERIFIED_STATIC**, runtime reproduction **NOT_RUN**. References:
`GraphicsComposer.vue:392..412`; `backend/app/src/api/routes/graphics.rs:228..239`
and the existing assignment route's visible-video validation.

A new save still POSTs content before performing optional assignment, and sets
`savedId` only after assignment succeeds. For example, a new image-only model
assigned to channel L-shape can pass ordinary template creation and then fail
assignment because there is no visible video window. The saved template remains
in the library, but Composer still believes it is new. Retrying after adding the
missing video sends another POST with the same name and encounters a conflict.

Fix direction: validate/create/assign atomically in a compatible optional POST
contract, or explicitly recover the created ID and keep a retryable editing
state when assignment fails. Add a new-create invalid-role-content rejection
and corrected retry regression. Existing-model atomic PUT does not cover this
flow. Relayed to lead as a scope challenge for F-011.

## Observed test artifact and limits

Read-only inspection of parent-owned
`/workspace/noar-cloud/review-20261109/browser/results.json` at review time:
45 tests, 42 expected/passing, 3 unexpected failures, no skipped/flaky tests.
The first-save, dirty-channel cancellation, delayed-save navigation, temporary
badge/pending-out and display-name persistence tests passed in that artifact.
The three failures were handle hit size, shared mapping timeout and logo restore.
The parent/helper are correcting and repeating them. These are supplied actual
browser results, not tests executed by this reviewer; later evidence must
supersede this provisional result.

No new residual HIGH was established in this source-focused Round B. The listed
MEDIUM regressions and failed browser checks still require resolution. Advisory
readiness and all per-finding votes remain **PENDING ROUND C**; this report does
not assert release readiness or authorize any deployment.
