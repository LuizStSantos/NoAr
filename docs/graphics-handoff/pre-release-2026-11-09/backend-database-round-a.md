# Backend database reviewer — Round A

Reviewer: `noar-database-reviewer` (independent backend council voter).
Scope: `/workspace/NoAr`, supplied HEAD `f925f608`; comparison `origin/main` `dd4529e2935b07d7a5bf8deb764421e4d201fc41`; released migration baseline `6cf680f6` (2.2.1). Read-only product review; no product files, migration files, Git references, servers, production databases, or streams changed. Runtime environment skill read; no network work required.

## Findings

### DB-A1 — Partial pre-31 conversion is mistaken for an unconverted channel

- Severity: **MEDIUM** (conditional upgrade blocker).
- Locations: `backend/app/src/db/handles/graphics/compatibility/conversion/graphics.rs:36`, `:109`; marker population is `migrations/00031_graphics_open_templates.sql:6`.
- Evidence: Migration 31 marks only channels retaining exactly three role bindings. A schema-29/30 channel whose converted logo/ticker/L-shape binding was removed has no marker after the upgrade. `convert_channel_graphics_on` then rereads frozen processing fields and attempts to insert all three roles rather than adopting/upgrading surviving models. One or two surviving roles cause a primary-key collision; zero surviving roles allow deleted models to be recreated from frozen legacy fields. `db_migrate` propagates that error at `backend/app/src/db/handles/init.rs:38`, so a partial channel can prevent application startup for every channel.
- Reproduction: Disposable Python SQLite fixture applies source migrations 1..30, inserts 0/1/2/3 role bindings, applies migration 31, and executes the conversion's first binding insert. Retained counts 1/2 produce `UNIQUE constraint failed: graphics_channel_bindings.channel_id, graphics_channel_bindings.role`; only retained count 3 receives the completion marker. Result stored in `backend-database-source-fixture.json`.
- Verifiedness: **VERIFIED_STATIC**, with the marker/constraint behavior directly observed in disposable SQLite. The actual Rust/SQLx conversion was **NOT_RUN**; no claim that an existing operator installation contains this state. Pre-31 deletion reachability in the historical branch remains to be confirmed. The current public deletion handles and the explicit marker comment make this a concrete state worth guarding.
- Blast radius: Installations that used intermediate schema 29/30 and removed/reassigned roles before schema 31; ordinary released schema-27 upgrades with no partial conversion are outside this finding.
- Next check: Run the real SQLx migrator/converter against synthetic pre-31 fixtures with deleted roles and scenes, including zero bindings; verify startup, retained IDs/layers, and no frozen-data resurrection. If confirmed reachable, handle recovery/adoption in conversion or an additive migration. **Do not edit, renumber, or squash migrations 00029..00034.**

### DB-A2 — Unchanged compatibility save rewrites authored logo coordinates

- Severity: **MEDIUM**.
- Locations: `backend/app/src/db/handles/graphics/compatibility/graphics.rs:344`, projection rounding at `:38` and `:69`.
- Evidence: Reading configuration converts a Composer image's percentage coordinates and size to even integer pixels using `layer_pixels`. Every subsequent compatibility save assigns the reconstructed native image geometry unconditionally to the saved image, even if the request contains the unchanged projected logo settings. For example, at 1280px width, authored `x=5.1%` projects to 64px and is persisted back as `5.0%`. Any unrelated full configuration save can therefore alter authored logo layout. The L-shape path at lines 417..420 already avoids rebuilding unchanged projected geometry, but the logo path does not.
- Reproduction: Source formula calculation in the disposable fixture records 5.1% -> 64px -> 5.0%; actual source confirms the unconditional assignment and `logo_layer_geometry` returns integer-pixel geometry as percentages (`backend/engine/src/compositor/graphics.rs:57`).
- Verifiedness: **VERIFIED_STATIC**; formula reproduction only, not an executed Rust adapter/HTTP test or observed playout change.
- Blast radius: Bound logo models using coordinates or dimensions that are not exactly representable as even pixels at the active output resolution; Composer-authored precision can be lost after configuration saves and survives restart.
- Next check: Synthetic model with a real tiny logo and fractional geometry, GET configuration then unchanged PUT or change only logging/audio, compare persisted layer JSON. Preserve saved geometry when the incoming compatibility values equal its projected representation; change only fields deliberately edited through the compatibility contract.

## Positive observations and checks

- **PASS (source checksum comparison):** Migrations 00001..00027 are byte-identical to Git baseline `6cf680f6`; migration 00028 is absent at that baseline. A first overbroad comparison attempted 00028 and stopped on that expected absence; the corrected bounded comparison passed.
- **PASS (disposable SQLite, source SQL):** All 34 migrations apply to a fresh in-memory database; `PRAGMA integrity_check` returns `ok`. This is not SQLx migration-history validation.
- **PASS (disposable SQLite, source SQL):** The permanent-template trigger rejects a seventeenth permanent model; deleting a channel with a bound template cascades without a foreign-key failure; font BLOB bytes survive `VACUUM INTO` plus reopen; migration 34 preserves existing action text bytes and leaves the new template column NULL.
- **VERIFIED_STATIC:** Main conversion uses one transaction for every channel and writes a durable per-channel marker; normal post-31 template deletion does not remove that marker. Binding/template writes, assignment, and configuration graphics saves have transaction boundaries; value queries use bound parameters. Shared API edit guards serialize Composer/configuration/source changes for a channel.
- **VERIFIED_STATIC:** Sources preserve last-success data on fetch error; revision plus fetch sequence predicates reject stale asynchronous completions. Fonts are stored as immutable BLOB rows with channel ownership, byte length, and count constraints. No confirmed defect found in those paths.
- Existing source tests reviewed: complete schema-29 upgrade/ID preservation, rollback on invalid layers, conversion frozen-column idempotence, restart/deletion behavior, binding channel ownership, pre-update backup, migration-34 action-byte preservation, font/source API tests. They do not cover DB-A1 or the unchanged fractional-logo save in DB-A2.
- Existing checked-in evidence read: `docs/graphics-handoff/migration-b-evidence.json` reports schema 27 -> 29 and baseline-image rollback; `layers-rollback-evidence.json` reports a restored schema-27 image; `layers-frozen-columns-evidence.json` reports frozen equality from a schema-29 backup; font/source evidence reports historical tests. These are **historical supplied artifacts**, not a fresh proof of HEAD and not proof that migration 34/current binary restores successfully.

## Verification ledger

- Product edits: NONE.
- Existing migrations 00029..00034 edited: NONE; SHA-256 captured in adjacent JSON evidence.
- Rust build/tests/fmt/Clippy: **NOT_RUN**, per scoped prohibition on heavy compilation and because this review changes no product Rust.
- Real SQLx upgrade/restart/rollback at HEAD: **NOT_RUN**.
- Server/UI/stream tests: **NOT_RUN**, per read-only scope.
- SQLite source fixture: **PASS**, with two candidate defect reproductions bounded as above.
- Independent ballot: **PENDING** peer digest and final deduplicated issue list. No readiness vote cast in Round A.

Reproducer: `/tmp/noar-database-source-fixture.py`; results: `/workspace/noar-cloud/review-20261109/backend-database-source-fixture.json`.
