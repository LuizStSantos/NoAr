# Frontend council — integration reviewer — round B

Reviewer: `noar-backend-reviewer`, one independent FRONTEND council voter.
Scope: current worktree on `claude/branch-review-opz967`, based on `f925f608`, compared with the Round A integration report and the parent's deduplicated peer digest. Read-only source inspection; only this report was written. No product edits, builds, server starts, browser actions, database writes or media tests. No Round C ballot cast.

## Original findings and proposed resolutions

- **FI-A1 / F-010, first-image reset — source fix appears complete.** `frontend/src/components/graphics/GraphicsComposer.vue:87` resolves an omitted target to the actual first image ID; the first-image select option at `:826` submits that ID. The save snapshot at `:389` supplies an explicit image ID before the unchanged-role PUT. Thus the old backend `null`-means-retain behavior no longer defeats the UI choice. The backend's old endpoint contract is preserved; explicit null in the new atomic assignment request is separately represented through a nested Option.
- **FI-A2 / F-010, former-layer mappings — source fix appears complete.** Composer `mapField` at `:130` now deletes the field mapping from every layer that no longer contains the placeholder, rather than skipping that layer. `pruneLayerMappings` filters each layer against its own fields after patching and again before saving. Remapping and returning to manual therefore remove an invisible former-layer reference even when another layer retains the same field. The added browser regression tests both persistent outcomes, but the parent's initial full browser run still had a timeout in this test; its runtime result must be resolved rather than inferred from source.
- **FI-A3 / F-011, existing role and content transition — source fix appears complete.** Existing saves send optional assignment together with content (`GraphicsComposer.vue:397`). API validation uses the effective requested role before persistence (`backend/app/src/api/routes/graphics.rs:202`), and `update_graphics_template_with_assignment` (`backend/app/src/db/handles/graphics.rs:208`) persists content, binding and displaced permanent state in one SQLite transaction. Explicit assignment omission preserves the old PUT behavior. Reviewed transition/rejection/late-write rollback test source; no execution claim by this reviewer.

## Residual FI-B1 — MEDIUM — New assigned template creation can partially save and strand retries

**VERIFIED_STATIC; runtime NOT_RUN.** The atomic path only applies when `templateId !== undefined`. A NEW Composer save still performs template POST first, then separate assignment PUT (`GraphicsComposer.vue:397–410`), and sets `savedId` only after assignment succeeds (`:417`).

Concrete bounded reproduction: create a new nonpermanent template with text `{price}` and no data mapping, choose Logo or Ticker in the save form, and save. The template POST permits unmapped fields for an ordinary temporary model, so it stores the template. Assignment subsequently rejects unmapped channel content (`backend/app/src/api/routes/graphics.rs:585–589`). The editor still has no saved ID. Correct the text/mapping or remove the assignment and retry: the next operation is another POST with the same name, producing a name conflict. The already-created ordinary model can be recovered through the library, but the current editor does not explain or recover the partial save.

A new permanent model with literal content but an invalid selected role can also be published as an ordinary permanent model by the first POST before role validation rejects the second call. This is a source interleaving/validation observation, not an observed HLS claim.

Recommended minimized test: reject the assignment step, assert either no model was created or the editor tracks the created ID and retries by update/assignment without duplicates. Preserve the captured channel and generation protections. Prefer atomic creation plus assignment if extending that contract; alternatively implement explicit partial-save recovery without silently discarding the draft.

Relayed directly to the lead. No product changes by the reviewer.

## Peer challenges and cross-discipline observations

- F-011's digest labels the original issue HIGH and “partially persists.” The original **existing-template** failure rejected the initial PUT before content mutation; Round A correctly described a MEDIUM blocked combined edit/two-save workaround. The newly identified creation sequence is the concrete partial-persistence case. Keep these scopes distinct and do not imply original data loss without evidence.
- Capturing `channel`, template ID, role, input and load generation before save, and creating `useGraphicsApi(() => channel)`, prevents a stale continuation from issuing assignment against the newly selected channel. The own-save URL exception addresses the first-save generation invalidation raised by the functional peer. Actual navigation/cancellation browser checks remain required.
- The runtime-control cancellation regression raised by the backend council concerns edit/publication locks, not field mappings or SQL transaction atomicity. An atomic database save does not by itself establish on-air publication correctness; retain the backend lifecycle tests separately.
- No independent objection to the rationale of F-001 through F-009 or F-012 from this API-integration scope. This is not an accessibility/visual/runtime confirmation or a vote.

## Verification ledger

| Check | Status | Scope |
| --- | --- | --- |
| First-image payload and target fallback trace | VERIFIED_STATIC | Current frontend → unchanged-role backend PUT |
| Per-layer mapping cleanup trace | VERIFIED_STATIC | Patch, remap, manual clear, final save pruning |
| Existing content + role transaction trace | VERIFIED_STATIC | Effective role validation; same-transaction DB writes |
| Cross-channel save snapshot trace | VERIFIED_STATIC | Captured API base and generation guards |
| Browser interaction / actual saved data | NOT_RUN by reviewer | Parent owns disposable browser suite; initial partial results are not final PASS |
| Rust compilation / SQLx transaction execution | NOT_RUN by reviewer | Parent owns Rust checks |
| HLS / restart / soak | NOT_RUN by reviewer | Independent media evidence required |
| Round C ballot | PENDING | Await final findings and executed evidence |

Gate: runtime verification is incomplete and FI-B1 remains open at this review snapshot. No release-readiness assertion or authorization.
