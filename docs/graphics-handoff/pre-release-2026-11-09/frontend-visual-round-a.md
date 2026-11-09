# Frontend visual council — round A

Reviewer: `noar-visual-design-reviewer` (one of five frontend voters). Scope: `/workspace/NoAr`, branch `claude/branch-review-opz967`, base implementation `f925f608` versus `origin/main`, plus the current uncommitted converted-name localization patch. Product files were not modified. No vote is issued before peer challenge.

## Evidence and limits

Read `CLAUDE.md`, `.claude/agents/noar-visual-design-reviewer.md`, the repository map, and both audit quality gate documents. Inspected the actual image contents using the image viewer: `docs/images/{composer,onair,graphics}.png`; `docs/graphics-handoff/composer-review-four-layers-phone-pt-BR.jpg`, `library-channel-phone-controls-pt-BR.jpg`, `ui-on-air-phone.jpg`, `composer-natural-full-output-phone-pt-BR.jpg`, `maintenance-composer-checkerboard-phone-pt-BR.jpg`, `open-templates-final-library-pt-BR.jpg`, and `preview-checkerboard-onair-390-pt-BR.jpg`. Compared relevant component code and the approved Composer/On air/library design HTML.

These are **archived screenshot inspections**, not screenshots captured by this reviewer against the present worktree. Archived images may show earlier labels or artwork. The localization patch was inspected statically; it has no newly captured visual evidence here. No browser interaction, keyboard operation, hit testing, live channel, media output, or persistence operation was performed. All interaction claims remain **NOT_VERIFIED**; no runtime PASS is implied.

## Findings

### V1 — MEDIUM — Canvas-edge resize handles are clipped

- **Source:** `frontend/src/components/graphics/composer/ComposerCanvas.vue:161` (16 px handle), `:164`/`:167` (handles positioned half outside the layer), and `frontend/src/components/graphics/TemplatePreview.vue:138` (`overflow-hidden` on the preview containing the overlay).
- **Observed reference:** `docs/graphics-handoff/composer-natural-full-output-phone-pt-BR.jpg`, preview around y=397–591. Its geometry readout is `x 0 · y 0 · w 100 · h 100 %`. The right and bottom handles are visibly cut in half; the bottom-right handle is reduced to the inner corner. Compare the fully visible handles in `maintenance-composer-checkerboard-phone-pt-BR.jpg`, where the selected logo is inside the canvas.
- **Impact:** Full-output artwork and layers aligned to the frame edge present smaller, less legible resize targets precisely when operators need to pull them back in. This is especially consequential on phones. It affects the authoring overlay, not the broadcast artwork.
- **Fix direction:** Keep artwork clipped, but render the editor selection/handles in a separate overlay that can remain visible, or position edge handles inside the frame. Preserve the saved layer geometry.
- **Verification:** **VERIFIED_STATIC + archived image inspection**. Pointer/touch drag behavior is **NOT_VERIFIED**. Recheck a layer at 0/0/100/100 and layers touching each frame edge at 390 px and desktop width; confirm all handles remain fully visible and draggable without introducing page overflow.

### V2 — LOW — Selected library group has no visible selected state

- **Source:** `frontend/src/components/graphics/GraphicsLibrary.vue:225` and `:232`. Both All and group pills always receive `btn-quiet rounded-full`; only `aria-pressed` changes. `frontend/src/assets/css/main.css:223` defines no selected-state presentation, and a targeted source search found no shared `[aria-pressed]` selector.
- **Observed reference:** `docs/images/graphics.png`, filter row near y=149, and `docs/graphics-handoff/open-templates-final-library-pt-BR.jpg`, filter row near y=149. All groups are displayed, so All is active, yet its pill has the same border/background/text treatment as its peers. Approved `docs/graphics-handoff/library-design.html:129` explicitly gives the active filter an accent border, tinted background, and lighter text.
- **Impact:** After filtering, sighted operators cannot identify the current scope from the filter row, weakening navigation feedback and making an apparently missing model harder to explain.
- **Fix direction:** Bind a selected class to the same predicate used by `aria-pressed`, with an accent border/background or another clear persistent treatment.
- **Verification:** **VERIFIED_STATIC + archived image inspection**. Filter clicking is **NOT_VERIFIED**. In staging, select All and each group, then blur the control; exactly the active pill should retain a visible selected treatment in light and dark themes and at phone width.

## Cross-discipline note for phase B

The 16 px resize controls also merit accessibility review: edge clipping compounds target acquisition, and the canvas controls expose selection via Enter but no keyboard resize behavior. This reviewer reports the visible clipping only; keyboard usability and target-size compliance belong with the accessibility reviewer and require their evidence.

## Checks and disposition

- Screenshot visual inspection: performed on the ten archived views listed above.
- Current source/style comparison: performed for Composer canvas/layers/timeline/inspector, On air, library, shared styles, and converted-name display helper.
- Current localization patch runtime/rendered verification: **NOT_RUN**.
- Browser interaction, keyboard/focus, mobile touch dragging, media output: **NOT_RUN**.
- Overall advisory ballot: **pending phase B peer challenge and final deduplicated findings**.
