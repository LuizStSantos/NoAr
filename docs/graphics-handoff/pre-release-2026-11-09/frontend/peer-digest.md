# Peer challenge and resolution digest

Round A reports are in the parent directory. Review every finding and challenge both original scope and proposed fixes. Runtime checks are pending; no approval from source alone. B-010 public reachability was not established and zero-bindings state remains ambiguous. Reviewers must flag any unresolved high defect; parent will restore blocker=true if warranted.

- **F-001 HIGH: Dirty channel navigation guard occurs after global config mutation.** `frontend/src/composables/editorNavigation.ts`. Proposed resolution: Global router beforeEach guard precedes configInit; cancelled header restores selection.
- **F-002 HIGH: Stale save continuation crosses document/channel generations.** `frontend/src/components/graphics/GraphicsComposer.vue`. Proposed resolution: Snapshotchannel/document/generation; obsolete responses cannot publish/reassign/navigate.
- **F-003 MEDIUM: Keyboard edits remain possible during save.** `frontend/src/components/graphics/GraphicsComposer.vue`. Proposed resolution: Native disabled+inert fieldset and disabled dialog metadata.
- **F-004 MEDIUM: Abandoned legacy editor lookup redirects new route.** `frontend/src/components/graphics/GraphicsChannelEditor.vue`. Proposed resolution: Disposed/generation guard.
- **F-005 MEDIUM: Template temporary on-air badges never refresh.** `frontend/src/components/graphics/GraphicsLibrary.vue`. Proposed resolution: Poll temporary list with channel bindings.
- **F-006 MEDIUM: Fullframe resize handles clipped by canvas overflow.** `frontend/src/components/graphics/composer/ComposerCanvas.vue`. Proposed resolution: Move handles inside bounds; focused desktop/mobile regression.
- **F-007 LOW: Active library filter has no visual feedback.** `frontend/src/components/graphics/GraphicsLibrary.vue`. Proposed resolution: Pressed visible style.
- **F-008 MEDIUM: Font upload input focus invisible.** `frontend/src/components/graphics/GraphicsComposer.vue`. Proposed resolution: focus-within style and keyboard check.
- **F-009 MEDIUM: Resize handles lack keyboard action and layer selection semantic state.** `frontend/src/components/graphics/composer/ComposerCanvas.vue`. Proposed resolution: Arrow resizing on handle; aria-pressed selection.
- **F-010 MEDIUM: First image cannot reset old target and stale data mapping remains on former layer.** `frontend/src/components/graphics/composer/ComposerLayerInspector.vue`. Proposed resolution: ActualfirstimageID; prune former-layer data mapping evenifsharedplaceholder remains elsewhere.
- **F-011 HIGH: Combined role+content save validates old role and partially persists changes.** `backend/app/src/api/routes/graphics.rs`. Proposed resolution: Optional atomicassignment payload validated against effective new role, single DBtransaction content+bindings+displaced permanent; old omittedpayload unchanged.
- **F-012 HIGH: Waiting takes cannot be cancelled when active list empty.** `frontend/src/components/graphics/GraphicsOnAir.vue`. Proposed resolution: Alwaysavailableall-out clears active and pending takes; noactive requirement.
