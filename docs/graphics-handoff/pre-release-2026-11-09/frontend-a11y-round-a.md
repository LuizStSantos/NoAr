# Frontend council round A — noar-accessibility-reviewer

Scope: read-only independent source review of `/workspace/NoAr`, HEAD `f925f608`, against `origin/main`, including current uncommitted display localization changes. Instructions consulted: root AGENTS/CLAUDE, reviewer definition, repository map, QUALITY_GATES and INTEGRATION_QUALITY_GATES. No vote is issued in round A.

Evidence boundary: source and existing test inspection performed; no browser, screen reader, dynamic keyboard check, build, or test execution performed by this reviewer. Runtime behavior remains NOT_VERIFIED. Lead is preparing isolated staging and received focused follow-up requests. Existing screenshots `docs/graphics-handoff/composer-review-text-fields-pt-BR.jpg` and `composer-layers-phone-lshape-pt-BR.jpg` were inspected as historical layout evidence only; their older background/captions differ from current source, and they cannot establish current keyboard behavior.

## Findings

### A11Y-A1 — MEDIUM — Busy Composer remains editable by keyboard; save response overwrites intervening edits

- Exact locations: `frontend/src/components/graphics/GraphicsComposer.vue:730` (grid disables pointer events only), `:158` (update handler has no busy guard), `:347` (save starts), `:354` (server response replaces draft).
- Evidence: busy changes CSS to `opacity-60 pointer-events-none` and sets aria-busy; it does not disable or make descendants inert. Layer, geometry and text controls remain enabled, and their handlers mutate the draft while a save request is pending. After the request returns, saveTemplate unconditionally replaces the draft with the earlier saved server result and records that replacement as the baseline. The Save form's ordinary fields are also enabled during this interval.
- Impact: keyboard users can enter edits through controls that appear visually unavailable, then lose those edits when the successful response arrives; the success message does not identify that loss. This is also a functional race for fields outside the pointer-blocked grid.
- Verifiedness: VERIFIED_STATIC for the enabled controls and overwrite path; end-to-end data loss NOT_VERIFIED.
- Next test: delay a disposable template PUT response, activate Save, Tab to an inspector input, edit and blur, release response; compare rendered draft and persisted model. Either make editing consistently unavailable during save or preserve/reconcile later edits.

### A11Y-A2 — MEDIUM — Font upload has an invisible keyboard focus target

- Exact location: `frontend/src/components/graphics/composer/ComposerLayerInspector.vue:191` (visible label), `:196` (focusable input uses `sr-only`). Related shared styles: `frontend/src/assets/css/main.css:223`.
- Evidence: the upload UI presents a styled label containing a visually clipped native file input. Keyboard focus goes to that input. The visible label has no focus-within treatment; shared `.btn-quiet` styles provide hover/disabled appearance but no focus-within rule. A focus outline on the clipped input does not visibly identify the full upload control.
- Impact: a sighted keyboard user can reach Upload font without seeing which control has focus (WCAG 2.4.7). Mouse activation and Playwright setInputFiles do not exercise this path.
- Verifiedness: VERIFIED_STATIC for markup/style omission; rendered focus appearance NOT_VERIFIED.
- Next test: select a text layer, Tab forward from Font into Upload font and capture focus appearance in light/dark themes; use Enter/Space to open the picker and cancel. Add visible focus to the label when its input is focused, or use a visible native control/button.

### A11Y-A3 — MEDIUM — Resize handles are focusable buttons with no keyboard action

- Exact locations: `frontend/src/components/graphics/composer/ComposerCanvas.vue:158`, `:170`.
- Evidence: width/height/both are native buttons with accessible names and only an `@pointerdown` handler. There is no click action or key handler; Enter/Space therefore emits the native click that has no resize behavior. Their ancestor `role="button"` handles Enter only to reselect the layer at `:154`. Space does not select a canvas layer either.
- Impact: the canvas exposes three apparent keyboard actions that do nothing. Geometry fields at `GraphicsComposer.vue:799` provide a keyboard alternative, so this is not a claim that resizing the model is entirely keyboard-inaccessible. The nested interactive role/button structure additionally warrants an accessibility-tree check.
- Verifiedness: VERIFIED_STATIC for event coverage; actual keyboard/AX behavior NOT_VERIFIED.
- Next test: focus each handle, press Enter/Space/arrows, compare dimensions; also inspect whether handles appear correctly in Chromium's AX tree. Provide keyboard resizing with documented increments, or remove inert handles from keyboard navigation while clearly providing the numeric alternative.

### A11Y-A4 — MEDIUM — Selected layer state is conveyed visually without programmatic selection state

- Exact locations: `frontend/src/components/graphics/composer/ComposerLayerList.vue:57`, `:61`; corresponding canvas `ComposerCanvas.vue:147` and timeline `ComposerTimeline.vue:39`.
- Evidence: layer selection is expressed by border/background classes. Selection buttons do not expose aria-pressed/current/selected or a relationship to the updated inspector. Canvas selection likewise only changes CSS. The polite dimension text contains coordinates, not the identity of the selected layer, and equally positioned layers can produce no text change.
- Impact: a screen-reader user cannot query which layer is currently selected and therefore targeted by geometry/property editing and deletion. This is particularly ambiguous with repeated kinds/names or returning to the layer list after editing.
- Verifiedness: VERIFIED_STATIC for missing selection state/relationship; screen-reader experience NOT_VERIFIED.
- Next test: create two same-kind layers with identical dimensions, keyboard-select each from the list and canvas, inspect AX state and announcements, then return focus to the list. Expose the selected layer using an appropriate stateful selection pattern or explicit named status.

## Coverage notes and next checks

- GenericModal has dialog semantics, a name, Escape handling and the shared stacked focus helper. Storage rename/create inputs are labeled; file grids use named native buttons and focus-within action visibility. No new concrete modal-trap defect is asserted from source.
- Scheduler timeline events are native buttons and have list/edit alternatives; drag operations alone are not treated as proof of missing keyboard functionality.
- Playlist import's unnamed file input and the More actions menu keyboard pattern already exist on origin/main; not presented as newly introduced findings.
- Existing `qa/e2e/tests/operator.spec.ts:159` tests basic schedule focus move/trap/restore/Escape. Composer tests at `:186`, font test at `:862`, and sources test at `:923` use clicks, fill/selectOption and setInputFiles; they do not verify the four cases above. `graphics-localization.spec.ts` checks translated accessible names, not keyboard focus or control activation. No existing test was executed during this review.
- Additional useful staging probe: keyboard-add a layer and delete the focused layer-list item, then inspect document.activeElement and next Tab destination; both actions remove the originating button without explicit focus repair. This remains a test request, not an additional finding.

Verification ledger: source review PASS; historical screenshot inspection PASS; dynamic keyboard/control-name/error checks NOT_RUN; screen-reader check NOT_RUN. No deployment/readiness assertion and no round-A ballot.
