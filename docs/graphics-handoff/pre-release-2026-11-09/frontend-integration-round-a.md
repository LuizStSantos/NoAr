# Frontend council — integration reviewer — round A

Reviewer role: `noar-backend-reviewer`, serving as one independent frontend-council integration voter. Phase 1 only; no ballot or release authorization.

Reviewed `/workspace/NoAr`, branch `claude/branch-review-opz967`, HEAD `f925f608671c17c32f4788993b84d044a6621907`, against `origin/main`. Current working-tree frontend localization patches and ongoing backend fixes were visible. Findings below refer to the working-tree source read on 2026-11-09.

Mode: read-only source/git inspection. No source edits, builds, servers, migrations, browser interactions, or runtime tests. All runtime checks below are proposed, NOT_RUN. Report-file creation is the only mutation by this reviewer. Read repository AGENTS/CLAUDE, backend conventions, reviewer protocol, repository map, and managed cloud runtime skill.

## FI-A1 — MEDIUM — “First image” cannot reset an existing explicit control target

**VERIFIED_STATIC.** `frontend/src/components/graphics/GraphicsComposer.vue:711` sends `null` for the “First image” choice. With an unchanged assignment, `saveTemplate` only calls the template PUT (`:347`); the assignment PUT is conditional on a role change (`:350`). `backend/app/src/db/handles/graphics.rs:155` selects `template.control_layer_id.as_deref().or(saved_target.as_deref())`, so null keeps the previous explicit image ID instead of choosing the first image. The fallback at `:165` is reached only if that old target is missing or invalid.

Reproduce in isolated staging: create a logo/L-shape model with image A followed by image B; explicitly choose B and save; reopen, select “First image,” save, and reopen. The selected/saved target remains B. Compatibility replace/move commands consequently continue addressing B after the user requested A.

Target check: exercise the UI reset plus GET template/control-layer ID, then a synthetic compatibility replace command; assert A is the resulting target. Preserve the intended distinction between an omitted target and an explicit reset, or resolve the UI choice to A's ID before saving.

## FI-A2 — MEDIUM — Remapping a shared field can conflict with its invisible former-layer mapping

**VERIFIED_STATIC.** `frontend/src/components/graphics/GraphicsComposer.vue:117` skips layers that no longer contain a field when applying or clearing a mapping. Text changes only apply a patch (`:169`; inspector emits text alone at `frontend/src/components/graphics/composer/ComposerLayerInspector.vue:102`), leaving old `data_fields` on that layer. `backend/app/src/graphics/templates/graphics.rs:525` prunes against the template's *global* field list, so it retains that obsolete mapping while the same field still occurs in another layer. `backend/app/src/graphics/sources/graphics.rs:692` rejects different bindings for the same field.

Reproduce in isolated staging: create two text layers containing `{price}`; map price to source/column A (mapping is copied to both); change the first layer to literal text; remap the remaining price field to source/column B, or switch it to manual; save. The former first layer still carries A, while the second carries B (or none). For B the API rejects the save with `Conflicting mappings for {price}`; for manual the old A mapping survives and still resolves the field automatically. The mapping panel at `frontend/src/components/graphics/composer/ComposerFieldMappings.vue:34` only exposes currently discovered fields, not these stale per-layer references.

Target check: repeat both remap and manual-clear cases, assert save succeeds and all obsolete per-layer mappings are removed. The simpler removal of the last occurrence is already handled by backend global pruning and is **not** a finding.

## FI-A3 — MEDIUM — Saving a role change validates edited content against the old assignment first

**VERIFIED_STATIC.** `frontend/src/components/graphics/GraphicsComposer.vue:347` PUTs edited template content before applying the newly selected role at `:351`. `backend/app/src/api/routes/graphics.rs:177` reads the existing binding and `:192` rejects content whose visible-video status does not match that old role. The UI offers assignment changes in the same save form (`frontend/src/components/graphics/GraphicsComposer.vue:697`).

Reproduce in isolated staging: open an assigned L-shape, remove its video layer, choose “No assignment” (or Logo), and save. PUT fails with “Only the channel L-shape model must contain a visible video window”; the assignment request never runs. Similarly, changing a logo/ticker into an L-shape with a video window is rejected against its old non-L-shape binding. The user must discover an undocumented two-save sequence (unassign without content changes, then edit/reassign), despite the form presenting one combined save action.

Target check: save each transition in one UI operation and assert the final layers and binding match the draft. Coordinate content and assignment validation atomically, or make the required staged workflow explicit and actionable without dropping draft edits.

## Coverage and exclusions

- Reviewed graphics templates, role assignment, take/out/channel payloads, font raw-byte upload/download, chunked image uploads, source mapping, scheduled model DTOs, and additive migrations 00028–00034. No new role/auth or basic font/schedule DTO mismatch established from static inspection.
- Existing migrations preceding 00028 are absent from this branch's migration diff; no new migration-checksum finding.
- Did not duplicate backend-council pending-wait cancellation, invalid permanent startup, field limits, or target-index/running-dimension findings.
- Additional lifecycle concern relayed to the lead for ownership/deduplication, not filed as a separate finding here: save responses and follow-up assignment requests are not tied to the original editor/channel version, while same-component route updates can proceed during busy operations (`GraphicsComposer.vue:592`, `graphicsTemplates.ts:205`).
- No runtime readiness claim. Phase 2 peer challenge and phase 3 independent ballot are pending the lead's digest and final finding list.
