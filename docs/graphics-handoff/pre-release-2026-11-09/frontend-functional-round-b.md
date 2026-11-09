# Frontend functional review — round B

Reviewer: `noar-functional-reviewer`, one of the five frontend council voters. Read-only peer challenge on the current worktree, 2026-11-09. Read the frontend peer digest/finding ledger, my round A report, and the code, integration, accessibility and visual round A reports. Inspected proposed fixes and the new `graphics-editor-safety.spec.ts`. No source edits, servers, builds, browser operations, or tests executed. Runtime remains **NOT_VERIFIED**. No ballot is issued before round C.

## Residual blocker: successful first save can leave a new Composer document permanently busy

**HIGH, VERIFIED_STATIC; runtime NOT_VERIFIED.** This is a regression in the combined F-001/F-002 fix. It must be resolved or disproved before treating their source fixes as complete.

1. `frontend/src/components/graphics/GraphicsComposer.vue:376` captures `loadVersion`; `current()` at line 385 requires that exact generation.
2. Successful save sets `savedId` (line 411), replaces the draft and clean baseline (line 422), then changes the new document route from no template ID to the saved template ID via `router.replace` (line 424).
3. The registered editor navigation guard sees a different `template` query (lines 657–660) and increments `loadVersion` at line 669 even though this is the save's own route update.
4. The route watcher explicitly skips `loadEditor()` when busy, saved ID matches the destination query, and the document is clean (lines 687–693). Consequently there is no reload to clear busy.
5. Successful navigation's after-hook only clears `navigationPending` (line 678); its busy reset applies only to failed navigation (lines 680–681).
6. The save's `finally` cannot clear busy (lines 443–445), because its captured generation was invalidated by its own route replacement. The native disabled/inert editing fieldset therefore remains locked after first save; the success note guarded by `current()` is also skipped.

Focused proof: Open **New template**, add a valid test layer/name, save once, wait for `/composer?...template=<id>`, and then assert Template name, layer controls, and Save template are enabled. Change a field and save a second time. Existing-template-save tests do not cover this first-save route change. Distinguish the save's own canonical-route update from an abandoned editor navigation while preserving stale-response protection.

This blocker was relayed to the lead immediately during review. It is new feedback, not a phase-three vote.

## Challenges to original findings and proposed resolutions

| Finding | Independent source assessment | Required proof / limit |
| --- | --- | --- |
| F-001 | Original defect is credible. `frontend/src/router/index.ts:274` now asks the editor guard before channel initialization at line 315. That addresses the original ordering. | Run actual cancelled/accepted header channel switching with dirty draft. Own-save guard interaction above remains a blocker. |
| F-002 | Snapshot input/channel/generation and `useGraphicsApi(() => channel)` in `GraphicsComposer.vue:376–385` address stale save ownership; continuations check ownership before state publication. | Delayed response after accepted navigation, delayed response after cancelled navigation, new-model first save, and same-document second save. Own-save generation interaction remains a blocker. |
| F-003 | Disabled/inert editor controls address the source-level keyboard mutation mechanism. | Confirm actual focus/keyboard behavior and modal fields during delayed save. Browser test exists but is NOT_RUN. |
| F-004 | `GraphicsChannelEditor.vue` now snapshots route/channel/kind, verifies them before redirect, and invalidates request version on unmount. This directly addresses abandoned lookup continuation. | Delayed lookup followed by leaving editor; assert no later redirect. |
| F-005 / FF-02 | `GraphicsLibrary.vue:173–177` now polls channel models and temporary on-air together, then updates both under the same state version; unmount invalidates it at line 206. Original missing-refresh path is repaired statically. | New E2E changes mocked on-air responses and observes badge add/remove; this proves browser polling only when executed. Also perform one real expiry/externally started synthetic take if claiming actual status correctness. |
| F-006 | Handles moved inside bounds addresses the observed clipping mechanism. | Desktop/mobile bounding boxes and actual drag interaction remain unverified; visual specialist owns final visual assessment. |
| F-007 | Active-filter conditional presentation is a direct source fix. | Test selection then blur; verify visible style in both themes. |
| F-008 | Visible focus-within treatment addresses the clipped native font-input focus indicator. | Actual Tab focus and picker/cancel path; accessibility specialist owns final assessment. |
| F-009 | Arrow resize and `aria-pressed` selection address the missing behavior/state. | Verify key increments and selection AX state; numeric geometry remains the alternate editing route. |
| F-010 | Actual first-image ID and pruning per-layer field mappings address the integration reviewer's two distinct mechanisms. | Exercise explicit-image-B to first-image-A save/reopen, plus shared-placeholder remap and manual-clear. Do not count only removal of the last placeholder, which was already globally pruned. |
| F-011 | Existing-template role changes now send optional assignment with template PUT; this addresses validation against the old role and separate assignment persistence for that path. | Integration voter must verify DB transaction and effective-role validation. New-template creation still uses POST followed by assignment (`GraphicsComposer.vue:403–404`); its assignment failure/retry is not made atomic by this fix. See scope caveat below. |
| F-012 / FF-01 | **Original cancellation defect addressed statically.** `GraphicsOnAir.vue:431` no longer requires active on-air entries for ALL OUT; it is disabled only while the operation is busy. `takeAllOut()` at lines 213–227 sends ID-less `/out` and resets local waiting after success. Backend `take_graphic_out` cancels pending takes for the ID-less payload. Main OUT also allows local waiting with no latest temporary take (line 412). | New E2E mocks waiting and `/out`, so it can verify the formerly disabled button and `{}` payload but cannot establish backend pending cancellation or absence of later output. Real disposable L-shape wait → ALL OUT → end L-shape → no late publication is still required for media behavior. |

## Cross-discipline observations and scope caveats

- F-012's always-available ALL OUT is stronger than relying solely on the new local `waiting` boolean: it remains usable after navigation/reload, and when waiting work originated elsewhere. This closes the original cancellation-access failure even though the local boolean is not an authoritative pending queue. Persistent pending-list visibility would be separate usability work, not evidence that the original all-out access remains broken.
- F-005's mocked badge test usefully isolates browser polling. Its result must be described as that, separately from real timed expiry or scheduler/output evidence. Overlapping delayed polls should be considered if newest-response ordering becomes a runtime concern; I did not promote an unexecuted timing scenario to another finding.
- F-011's new-template path still creates the model before assigning it and records `savedId` only after successful assignment. An injected assignment failure can therefore leave a created model while retry still uses POST, leading to a duplicate-name response. This is a **MEDIUM static scope caveat** in the wider combined-save journey, not proof that the existing-template transactional fix failed. Add a disposable failure/retry test and either retain the created ID for a safe retry or extend atomic handling if the fix is claimed to cover all creation+assignment cases.
- The native busy lock makes the own-route busy-reset defect more consequential: stale-generation protection and accessibility disablement must be validated together, including a new model's first save. The current safety tests primarily use models created by an API fixture and then edited in Composer, leaving this route transition uncovered.

## Disposition

Residual HIGH blocker above is unresolved at the source snapshot inspected and should be tracked explicitly before round C. FF-01/F-012 and FF-02/F-005 source resolutions are credible, pending actual execution evidence. All runtime, persistence and media-output claims remain NOT_VERIFIED/NOT_RUN. No readiness vote yet.
