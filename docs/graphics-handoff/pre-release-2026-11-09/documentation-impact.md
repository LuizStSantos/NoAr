# Documentation impact review

Reviewer: `noar-documentation-guardian` (non-voting, read-only product review).
Date: 2026-11-09. Baseline: `f925f608`; current cleanup commit: `a3c008fe`, with implementation fixes still in the worktree.

Status: **FOLLOW_UP**. This report checks documentation impact and links. It does not establish runtime behavior or release readiness. Final browser, media comparison, performance and 90-minute soak evidence remain parent-owned and pending at this handoff.

## Required follow-up

| Priority | Target | Classification | Proposed change and trigger | Confirmation needed |
| --- | --- | --- | --- | --- |
| P1 | `docs/graphics-handoff/README.md` | NEEDS_VERIFICATION | Replace pending verification statements with the final executed ledger, tested source/binary identities, command features, unchanged migration hashes, exact soak duration and graphic actions, main/candidate fault outcomes, benchmark profile/scopes, browser result totals and remaining limits. Link detailed reports on `evidence/graphics-2.3`; keep at most 20 principal screenshots. | Parent runtime evidence; do not infer PASS from historical captures or council votes. |
| P1 | `CHANGELOG.md`, Unreleased | VERIFIED_STALE for this review's new changes | Add concise fixed entries for converted default display localization without database renames; safe dirty/stale Composer navigation and saves; atomic content/assignment create/update; bounded cancellable waits and text expansion; isolated optional artwork failures; geometry precision preservation; cached logo movement, lock-safe preparation and retained motion epochs. Existing notes cover the earlier graphics feature/refactor but not these fixes. | Final implemented scope and passed checks. Do not change the release number or date. |
| P1 | `docs/ai/repository-map.md`, `docs/ai/symbol-index.json` | NEEDS_VERIFICATION | Refresh after the final source and evidence cleanup. New display-name/navigation composables, E2E tests, native soak helper, request type/DB helpers and guarded publication methods changed after the last supplied map. | Parent runs the standard incremental indexer once edits settle. Navigation entries are not behavioral evidence. |
| P2 | `docs/developer.md`, Processing Benchmark | NEEDS_VERIFICATION | Mention or link `graphics_blend` boundaries documented in `graphics-maintenance.md`: several disjoint calls per frame, preparation and native encoder work excluded, whole-process CPU/RSS required. Mention native graphics soak as an optional development-container workload through the existing QA link. | Match final implementation and report; no hardware/capacity claim from debug timings. |
| P2 | `docs/logo.md`, Composer/On air operation | NEEDS_VERIFICATION | Add a short operator note if needed: converted generated names are localized for display only; custom names/groups remain unchanged. Out-all also cancels graphics waiting for an L-shape, while leaving L-shape releases waiting graphics. Detailed limits already belong in API docs. | Final browser/API tests and actual control behavior. |
| P2 | `docs/releasing.md`, SemVer examples | VERIFIED_STALE / existing policy ambiguity | The MAJOR example says a database migration that cannot be undone merely by returning to the previous image, while the MINOR example permits additive migrations. `docs/docker.md` correctly explains that SQLx rejects migrations unknown to an old binary and rollback restores backup plus old image. Clarify that normal SQLx restore procedure versus an incompatible/destructive data contract does not imply contradictory version guidance. | Parent policy wording; this review chooses no version, tag or release. |

## Documentation already aligned in the supplied worktree

- **UPDATED, source reviewed:** `docs/api.md` describes optional combined `assignment` on template POST/PUT, its transaction boundary, omitted versus explicit-null target semantics, preservation of the separate assignment endpoint, 16 waiting takes, cancellation triggers, manual field/record limits and aggregate expanded-text bound. These statements still require the parent's final executable tests.
- **UPDATED, source reviewed:** `docs/graphics-maintenance.md` explains edit invalidation versus runtime control serialization, generation checks across asynchronous publication, avoiding native work under frame-critical locks, atomic persistence, optional asset failure isolation and `graphics_blend` measurement limits.
- **UPDATED, source reviewed:** `qa/soak/README.md` documents native opt-in binaries/paths, fresh ownership checks, process cleanup, independent video-only ROI, real monotonic duration, graphics receipts versus artwork-pixel proof, and bounded SRT/UDP outage grace. This is a workload description, not proof that 90 minutes passed.
- **NO_CHANGE_NEEDED:** `docs/docker.md` already documents restoring the pre-migration backup and previous image together, SQLx version rejection, WAL/SHM handling and best-effort backup limits. No new schema migration is introduced by the supplied fixes; migrations 00029–00034 must remain immutable.
- **NO_CHANGE_NEEDED for feature descriptions:** root `README.md` already covers Composer layers, fonts, data sources, scrolling, optional role assignments and channel controls. New release-review screenshots should not be described as verified there unless refreshed and inspected.
- **NO_CHANGE_NEEDED:** `frontend/README.md` has no newly contradicted API/installation contract in this bounded review. The path-based suggestion of `docs/ai/{frontend,backend,engine}.md` points to nonexistent documents; creating those files is unnecessary.
- **NEEDS_VERIFICATION:** `docs/graphics-roadmap.md` is an approved design/planning record. Its “What exists today” and ordering text should be labelled as the original baseline or kept consistent with current model-before-L-shape composition if edited; do not silently rewrite approved prototypes. The archive README now supplies their location.

## Link and image checks actually executed

A local Python check inspected inline Markdown links in root `README.md`, every `docs/**/*.md` file and `qa/soak/README.md`, resolving file targets after removing anchors/query strings and excluding external URLs.

- **PASS:** zero missing local Markdown file targets.
- **PASS:** five links reference `evidence/graphics-2.3`; all `blob` attachment targets exist in `/workspace/noar-cloud/evidence-graphics-2.3`.
- **PASS:** retained handoff JPG count is exactly 20. Desktop captures are 1440 pixels wide, phone captures 390 pixels wide, and output frames 640×360. Some captures are full-page heights; these are not claimed to be the uniform README gallery images.
- **NOT_RUN:** remote HTTP link availability and GitHub rendering. Local archive presence is not proof of a future remote commit's availability.
- **NOT_RUN:** product UI, media output, rollback, Clippy, build or soak execution by this reviewer.

The path-based documentation impact script was run with `--base f925f608`. Its output was treated as a candidate list, not semantic proof. No product/source documentation, migrations, Git references or services were changed by this reviewer. Only this requested external review report was written.
