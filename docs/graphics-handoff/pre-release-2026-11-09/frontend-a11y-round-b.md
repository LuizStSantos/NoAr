# Frontend council — accessibility reviewer — Round B

Reviewer: `noar-accessibility-reviewer`, independent frontend voter. Scope: current working tree based on `f925f608`, original Round A findings, peer digest and targeted Composer fixes. Read-only product review; no source changes, browser launch, server, build, migration, Git mutation or ballot.

## Original findings and proposed resolutions

| Final ID | Original issue | Round B challenge / resolution assessment |
| --- | --- | --- |
| F-003 | Enabled keyboard editing during saves | **Source fix complete in inspected path.** `GraphicsComposer.vue:742` uses a native disabled fieldset plus inert, both driven by `editorLocked`; this includes busy/navigation/font upload. Native disabling prevents the previously enabled form controls from accepting keyboard edits. The inspected saved Playwright report records `save disables native editing and a late response cannot navigate back after leaving Composer` as passed; its assertions cover layer name, X, assignment and dialog template-name disabled state. This is observed evidence from the shared staging runner, not a test personally executed by this reviewer. |
| F-008 | Font picker has invisible keyboard focus | **Source fix complete; rendered focus verification still pending.** `ComposerLayerInspector.vue:192` adds a 2px accent outline and offset on label focus-within. The real focusable native file input remains named and clipped inside that visible label. A new test tabs from Font to Upload font and checks the label outline. The only supplied whole-test result for that combined mapping/font case is timedOut, so it cannot establish a current complete PASS for this interaction. |
| F-009 | Focusable resize handles have no keyboard action | **Source fix complete; final runtime check pending.** `ComposerCanvas.vue:64` handles arrows, Shift+arrows, Enter and Space; it prevents default and stops propagation, then emits one resize transaction. A screen-reader-readable help paragraph is referenced by each handle, and the localized instructions match the 1% / Shift 10% increment. Handles are native buttons; the old nested role-button structure was replaced by a separate native move/selection button. The initial full-frame test failed at the 24px target-size assertion before exercising its resize assertions, so that test is not current keyboard proof. |
| F-009 | Layer selection has no programmatic state | **Source fix complete.** Canvas selection button (`ComposerCanvas.vue:187`), layer-list button (`ComposerLayerList.vue:63`) and timeline button (`ComposerTimeline.vue:41`) each expose aria-pressed from the actual selected ID. Hide/show buttons expose their own independent pressed state. Runtime AX-tree and assistive-technology verification remain NOT_RUN. |

No additional HIGH accessibility issue is confirmed in this bounded source review.

## Peer challenge and cross-discipline observation

- The current full-frame target fix uses explicit `w-[24px] h-[24px]`; this avoids treating Tailwind's 1.5rem as 24px in the application's 14px root font. The initial saved test failure (21px target) corroborates why this matters. Do not replace the numeric assertion with a smaller threshold merely to make it pass.
- The visual review's small-logo handle overlap is a valid related concern. A focusable button may remain keyboard reachable despite another handle covering its entire pointer target; those are distinct interaction paths. Numeric geometry inputs and newly implemented keyboard handles provide alternatives, but the pointer/touch authoring path still deserves the separate small-stock-logo test requested by the visual reviewer. Full-frame bounds assertions alone cannot resolve that issue. The implementation helper is already addressing it; no duplicate finding is added here.
- First-save locking, stale asynchronous authoring continuations and atomic new assignment are functional/integration concerns with accessibility impact: a disabled state must reflect the current operation, not a stale document. Their other reviewers' reproductions should be resolved before final keyboard evidence is collected. This review does not change their severity solely on accessibility grounds.
- Source-only aria-pressed is not proof of useful screen-reader announcements, focus restoration after deleting the focused layer or successful platform file-picker activation. Those remain explicitly unverified rather than additional confirmed defects.

## Evidence and limits

Inspected actual artifact `/workspace/noar-cloud/review-20261109/browser/results.json`: 42 expected, 3 unexpected, no skips. Relevant delayed-save/native-lock test passed. Full-frame size/keyboard test failed at target size; combined font-focus/mapping test timed out; operator natural-size restore failed. The helper is correcting these and preparing a fresh complete run. No failed relevant test is waived by this source review.

Source review: **PASS within scope**. Dynamic keyboard checks personally executed: **NOT_RUN**. Current font focus and resize test completion: **NOT_VERIFIED**. Screen reader / assistive technology: **NOT_RUN**. Round C: **PENDING final fixes and runtime evidence**. No readiness ballot or release authorization issued.
