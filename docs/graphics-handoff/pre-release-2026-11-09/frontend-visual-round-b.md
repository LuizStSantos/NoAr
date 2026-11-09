# Frontend visual council — Round B peer challenge

Reviewer: `noar-visual-design-reviewer`, independent frontend voter. Worktree: `f925f608` plus review fixes. Read-only product review; no browser, server, build, API request, migration, Git mutation, or source edit. No Round C ballot cast.

## Original visual findings

- **F-006 / V1:** The source fix moves the resize targets into the canvas and uses explicit 24px dimensions. The supplied dark pt-BR `browser/checkerboard-composer-{1440,390}-pt-BR.jpg` captures visually show all three full-frame targets inside the frame. These are screenshots captured by the parent-owned browser fixture, not independently captured here. The initial suite's 21px target failure was a real mismatch caused by rem sizing; the current explicit `w-[24px] h-[24px]` corrects the source dimensions. A successful browser rerun remains required.
- **F-007 / V2:** The current library binds accent border, tinted background, and accent text to exactly the predicate used by `aria-pressed` (`GraphicsLibrary.vue:234`, `:245`). The supplied desktop/phone templates captures visibly show the active Todos pill in orange, unlike inactive pills. This follows the approved library prototype's selected treatment. Other groups, blur persistence, and light theme were not interacted with by this reviewer.

## V-B1 — MEDIUM — Small-layer targets overlap after clamping

- **Verification:** VERIFIED_STATIC, with a bounded numerical execution of the current positioning formula. Pointer hit testing is NOT_RUN.
- **Location:** `frontend/src/components/graphics/composer/ComposerCanvas.vue:54` (`handleStyle`), `:195` (width, height, both DOM order), `:198` (equal z-index and 24px targets).
- **Construction:** A stock 42×42 logo, positioned 12px from the top/right of a 1280×720 output, has percentage geometry `x=95.78125, y=1.6666667, width=3.28125, height=5.8333333`. On a 346×194.625px phone canvas, the formula produces these target rectangles:

  | Target | Left | Top | Width | Height |
  | --- | ---: | ---: | ---: | ---: |
  | Width | 318.756 | 0 | 24 | 24 |
  | Height | 322 | 0 | 24 | 24 |
  | Both | 318.756 | 0 | 24 | 24 |

- **Consequence:** The last rendered Both button completely covers Width and leaves only about a 3.2px strip of Height available to the pointer. The controls are inside the frame, but independent one-axis dragging is effectively unavailable for a normal small corner logo on a phone. Numeric inspector inputs and keyboard focus are alternatives, so this is not classified HIGH.
- **Suggested correction/test:** Separate authoring target positions when their rectangles collide, preserving artwork geometry; alternatively expose a single unambiguous pointer target with explicit resize-mode selection. Add a phone small-logo hit-test proving that a point inside each target resolves to that target, not merely checking bounds. Current new E2E covers full-frame artwork only.
- **Reachability:** Ordinary Composer image layers, including the stock logo, can be smaller than the targets. No unusual API-authored geometry is necessary.

The finding was relayed to the parent for a source fix or explicit disposition. It is not added to the final issue list by this reviewer.

## Peer challenge and cross-discipline observations

- Native disabled/inert save controls, first-save route-generation handling, asynchronous operation ownership, pending-take cancellation and atomic assignment need their functional/API evidence. Visual screenshots do not establish those behaviors. Parent reports the initial full suite as 42/45, with the remaining failures being corrected; therefore no interface readiness approval is justified yet.
- Enlarging controls for accessibility must also account for target independence. Bounds and minimum dimensions alone do not catch V-B1; keyboard access does not prove pointer usability.
- The available screenshots contain custom fixture names `Channel L-shape`, `Channel logo`, and `Ticker`. The display helper deliberately preserves custom names, so these captures do not demonstrate the requested raw converted-default localization. Obtain captures using raw defaults `lshape/logo/ticker` in `Channel graphics`, with generated `image/Image` and `picture/Picture` layers; verify localized display and unchanged stored bytes.
- The templates captures show an unavailable logo image and leftover Editor safety fixture. They are useful diagnostic artifacts but should not replace the principal clean review gallery until the image issue and fixture cleanup are resolved.
- A Vite development overlay bubble overlaps the timeline and a template thumbnail in supplied captures. This is a development capture artifact, not asserted as a production defect; hide it in final gallery screenshots.
- The L-shape video placeholder's blue-gray rectangle is opaque authored video content; the surrounding transparent preview uses the checkerboard. Do not mistake that video-window placeholder for a failure of transparent canvas backing.

## Evidence examined

- Current Composer canvas/selection/handle code, current library selected classes, display-name helper, full-frame browser regression, and approved archived library design filter styles.
- Actual supplied image contents: `browser/checkerboard-composer-1440-pt-BR.jpg`, `browser/checkerboard-composer-390-pt-BR.jpg`, `browser/checkerboard-templates-1440-pt-BR.jpg`, `browser/checkerboard-templates-390-pt-BR.jpg`.
- Own browser/touch/keyboard/layout-runtime execution: NOT_RUN (parent owns runtime).
- Builds, lint, Rust/media tests, output parity, soak: NOT_RUN by this reviewer.
- Round C ballot: PENDING final issue list and executed runtime evidence.
