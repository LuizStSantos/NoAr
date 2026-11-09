# Backend API/application council — Round B

Independent reviewer: `noar-backend-reviewer` for the **backend** council. This is a new independent continuation of the supplied Round-A report, not a claim to have performed its earlier tool calls. Revision inspected: `f925f608` plus the current worktree fixes. No ballot is cast in this round.

Read root/backend instructions, `CLAUDE.md`, the reviewer definition, repository map, quality gates, final candidate list, peer digest, and relevant API/service/database/scheduler source and tests. Source review only: builds, tests, browser interaction and output playback **NOT_RUN** by this reviewer. Parent owns execution.

## Peer challenges and source resolution assessment

| Finding | Round-B assessment |
| --- | --- |
| B-001 | Original allocation trace is credible. Manual field/record validation now precedes specification construction; the expansion budget counts replacement slices before constructing the expanded text. The source fix addresses the allocation route; runtime rejection tests remain required. |
| B-002 | The publication mutex, bounded pending registry, cancellation token and version checks address the original stale/unbounded takes. **The cancellation side effect is too broad for runtime controls; see R-B1 below.** Source resolution is partial until this regression is corrected. |
| B-003 | Per-template preparation failures now warn and allow the loop to continue, while the preceding database queries still propagate errors. Failed IDs are retained so prior prepared pixels can survive an asset failure. This matches the proposed isolation policy. A real missing-artwork startup/output check is still required. |
| B-004 | Initial and dynamic L-shape loads now consume the `(spec, primary)` returned by the shared filtered conversion helper. The previous unfiltered-index derivation is gone from those publication paths. Logo uses the corresponding mapping. Native renderer omission behavior is the media reviewer's deeper scope. |
| B-005 | `draw_and_take` resolves running output dimensions before falling back to saved settings. This addresses the temporary-take path described in Round A. A pending-output-restart coordinate check remains the direct runtime evidence. |
| B-006–B-009 | The source changes target the stated cached movement, native work under mutexes and animation-epoch problems. I defer detailed raster/native timing conclusions to the media reviewer; source inspection alone does not establish frame continuity or performance. |
| B-010 | Surviving role/scene bindings can now establish adoption without reimporting frozen values. The historical production reachability qualification is necessary: the pre-31 deletion handle was not established as an operator-reachable route. Do not describe the fully empty ambiguous state as a proven operator upgrade failure. SQLx upgrade/repeated-start tests remain necessary. |
| B-011 | Independent position/size comparisons and the previous projection dimensions address the fractional-coordinate round trip at the API/configuration boundary. Actual database persistence tests, rather than just formula arithmetic, are needed for completion. |

## R-B1 — MEDIUM — ordinary runtime controls cancel queued Wait takes

Status when discovered: **VERIFIED_STATIC**; actual HTTP execution **NOT_RUN**. This is a regression in the proposed B-002 fix, reported promptly to the lead and implementation helper. It is not another copy of the original unbounded-queue finding.

- `GraphicsTakes::begin_edit` in `backend/app/src/graphics.rs` now calls `cancel_pending`.
- `control_channel_graphics` in `backend/app/src/api/routes/graphics.rs` acquires that edit guard before invoking `control_channel_graphic` for every role and either visibility value.
- Consequently the normal bound-model API/UI sequence `L-shape on → take Wait GC → bound L-shape off` cancels the GC before turning the L-shape off. The promised Wait behavior is lost through the public model control, although direct `manager.lshape_control.off()` still releases it.
- `TemplateRuns::start` in `backend/app/src/player/graphics_schedule/graphics.rs` likewise calls `begin_edit` before checking whether a scheduled Wait model must keep waiting. A scheduler no-op poll can therefore cancel an unrelated manual pending take.
- Existing wait-release tests call the engine control directly. New cancellation tests cover explicit out/edit/delete/lifecycle actions but do not exercise the public L-shape-off release path, so they would miss this regression.

Recommended distinction: use the shared noninvalidating publication lock to serialize runtime visibility commands and scheduled takes; reserve cancellation/generation invalidation for authored changes, explicit all-out, and lifecycle resets. Also trace the `binding.enabled=false` logo/ticker enable path through `manager.update_config`, which otherwise invalidates the queue indirectly.

Required focused regressions:

1. Through actual HTTP routes: bound L-shape on, queue a Wait GC, turn the bound L-shape off, and observe that the queued GC enters.
2. Toggle an unrelated bound logo/ticker while a Wait GC is pending; the queue must remain intact.
3. Poll a scheduled Wait event while an unrelated manual take waits; ending the L-shape must still release the manual take.
4. Keep the explicit all-out/edit/delete/output-stop cancellation tests passing.

The implementation helper confirmed the call-chain issue and is preparing the guard distinction plus HTTP/scheduler regressions. This report does **not** assert those later edits or tests have passed; re-review is needed before Round C.

## Atomic assignment and cross-discipline observations

The optional assignment object on existing template PUT now determines the effective role before role validation. Content, displacement of a previous permanent role, binding updates and control target changes are written by a single database transaction under the edit lock. An omitted assignment preserves the former route behavior. The added late-trigger rollback test is a useful check of real transaction rollback, rather than just an early-validation assertion. I found no additional confirmed high-severity API defect in this scoped pass.

One useful cross-discipline lesson is R-B1: serializing content mutation and serializing runtime state changes need the same exclusion primitive but different invalidation semantics. UI testing of the public control is essential because direct engine-control tests can bypass application side effects.

Current advisory gate remains **NOT_READY**: R-B1 is unresolved at this report's snapshot, and runtime verification/90-minute soak/performance comparison are pending. No five-voter verdict, release permission, or tested-media assertion is implied.
