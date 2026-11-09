# Backend database reviewer — Round B peer challenge

Reviewer: `noar-database-reviewer`, independent backend council voter.
Scope: `f925f608` plus current worktree fixes on `claude/branch-review-opz967`.
Read-only product review. No product/migration edits, builds, services, real databases,
streams, Git mutations or ballot were performed in this round.

## B-010 / DB-A1 — challenge and resolution

**Original finding is conditional, not proof of an operator-reachable upgrade failure.**
At historical `23d73aa4^`, `git grep delete_graphics_binding` finds its definition and
two test calls only. The historical HTTP delete handler explicitly rejected any
bound template, and migration 29's foreign key used `ON DELETE RESTRICT`. This
independently supports the implementer's challenge: ordinary pre-31 API deletion
has not been shown to create the synthetic partial state. The Round A SQLite
fixture proves a constraint failure for a manually constructed state, rather
than a known installed production database.

The defensive fix at
`backend/app/src/db/handles/graphics/compatibility/conversion/graphics.rs:47`
accepts surviving role **or scene** bindings as evidence of an existing import.
It upgrades those definitions in place, inserts a completion marker and returns
before reading frozen processing columns. The all-channel caller retains its
single SQLite transaction, so marker/layer changes roll back together. Existing
marker state still avoids import even after every template is deleted.

The zero-marker/zero-role/zero-scene case is explicitly ambiguous at line 67.
The fix correctly does not infer prior import merely from an unrelated custom
template. It retains virgin conversion behavior. I do not classify that
remaining indistinguishability as a confirmed operator blocker without a
reachable historical sequence. Do not claim the fix can recover arbitrary
manually erased migration metadata.

Source regression coverage added:

- `partial_pre31_import_adopts_surviving_roles_and_scenes_without_frozen_reads`
  exercises real SQLx schema 29 and 30 fixtures, retained role counts 0..2 with a
  surviving scene, edited L-shape coordinates, corrupt frozen scene data,
  preserved IDs, and repeat startup conversion.
- `partial_pre31_roles_alone_prove_import` covers role-only adoption.
- `unrelated_custom_template_does_not_suppress_virgin_import` protects the
  negative inference above.

**Resolution: implemented and plausible by source; execution remains parent-owned.**
No residual confirmed DB blocker from B-010.

## B-011 / DB-A2 — challenge and resolution

The original precision-loss claim remains supported by the previous unconditional
assignment. The new code at
`backend/app/src/db/handles/graphics/compatibility/graphics.rs:363` computes the
same rounded projection used by the read DTO, compares position and scale
independently, and only replaces the corresponding authored pair when its legacy
field changes. An opacity-only or unrelated save therefore retains normalized
coordinates and dimensions in source.

The output update ordering was specifically checked:
`backend/app/src/utils/config/update.rs:240` passes the **previous saved output
width and height** to `update_configuration_with_projection_on` after updating
the output row in the same transaction. That avoids comparing an old projected
DTO against the new output dimensions. The public non-output-changing wrapper
queries the active output before applying configuration changes. Geometry newly
specified by an explicit legacy edit is still evaluated against the selected
output, as expected for that edit.

Source tests cover unchanged and audio/opacity-only saves, independent position
and size changes, output resize, projected DTO reread, repeat save and converter
restart. These tests use the actual migration runner, rather than a handwritten
replacement table.

**Resolution: implemented and plausible by source; execution remains parent-owned.**
No residual confirmed geometry blocker from B-011.

## Atomic assignment / frontend peer concern

At `backend/app/src/db/handles/graphics.rs:208`, authored content, binding changes
and displacement of a previous permanent model share one transaction. The
existing update and assignment paths reuse per-connection helpers. Early false
return leaves the transaction uncommitted, while late binding errors roll back
both content and the displaced model's permanent flag. A new synthetic
`BEFORE INSERT` trigger test explicitly checks the late-failure case. API
validation selects the effective requested role before mutation; omitted
assignment retains the previous endpoint contract.

The replacement role's permanent slot is released before enabling the incoming
model, avoiding a transient seventeenth permanent model under migration 31's
quota trigger. Bind values remain parameterized and channel-scoped. Explicit null
assignment-target handling falls back to the first image through the existing
SQL behavior; omitted target preserves a valid prior target. No newly confirmed
transaction/cross-channel flaw was found in these inspected paths.

## Migration integrity and evidence limits

**PASS — source bytes:** all six files `00029` through `00034` are byte-identical
to `f925f608`; independent SHA-256 calculation returned:

| Migration | SHA-256 |
| --- | --- |
| 00029 | `29042bb749a9e8dfdae103b250f5566d9ad5808923a7c080e8955748e26df3d7` |
| 00030 | `ab5ba065652d611e19d4ee6a3418ad960993a8010991c97a425ba514dca15ba2` |
| 00031 | `6b0f4a52c4e7a762ca373de5bbccf199e080dbc333ee1ab72fdafa073338ad14` |
| 00032 | `d9a80abb29390ab20fb16a6674af2e12f944694ce244f56eb52be7e2693c709f` |
| 00033 | `c7f5815cc091f9466ae57fbb01afd074f8b54f609335d5519b9f5ecf76c670b6` |
| 00034 | `8748e893a0ae70bcfebbb58c3ee40516172d8961d32675798e1efcb2ca1162c1` |

`docs/docker.md` correctly distinguishes frozen-column retention from downgrade
compatibility, requires restoration of the pre-update backup **and old image**,
warns that backups are best effort and only created for pending SQLx migrations,
and preserves the first successful backup across a conversion retry.

This round did **not** execute Rust/SQLx tests, backup restoration, server restart,
API, browser, native decoder, processing-bench or soak checks. Their presence in
source is not a runtime PASS. Peer security/lifecycle/media objections remain for
the relevant reviewers and parent to resolve. No overall readiness approval or
Round C ballot is asserted here; runtime evidence and the final issue list are
still required.
