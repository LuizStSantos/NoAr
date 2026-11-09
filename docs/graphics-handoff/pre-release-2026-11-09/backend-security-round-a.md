# Backend council — security reviewer — round A

Reviewer identity: `noar-security-reviewer` (independent, read-only).
Revision: `f925f608` (`claude/branch-review-opz967`), compared with `origin/main` at `dd4529e2935b07d7a5bf8deb764421e4d201fc41`.
Date: 2026-11-09. No ballot cast in this round.

Read repository `AGENTS.md`, `CLAUDE.md`, `backend/AGENTS.md`, `.claude/agents/noar-security-reviewer.md`, both audit gate documents and the repository map. Used read-only diffs and current source to trace new graphics entry points, font uploads/downloads, image resolution and decoding, feeds, compatibility controls, scheduling validation and extracted configuration update authorization. No product changes, credentials, network attacks, builds, servers or production operations.

## SEC-A1 — HIGH — manual take fields can cause massive allocation before rendering

- Primary location: `backend/app/src/graphics/templates/graphics.rs:605`.
- Status: **VERIFIED_STATIC**. Backend/HTTP execution: **NOT_RUN**. Arithmetic below was evaluated locally without invoking the backend or allocating expanded strings.
- Trust boundary and source: `POST /api/graphics/{id}/take` accepts arbitrary `HashMap<String, String>` fields (`backend/app/src/api/routes/graphics.rs:282`) and admits regular channel users through `authorize_channel_access(..., false)` at line 320. Channel authorization works; the problem is that an account authorized for one channel can exhaust the shared backend process.
- Complete trace: route lines 334–342 forwards unbounded fields to `take_template`; `backend/app/src/graphics.rs:211` builds the complete spec synchronously through `graphic_spec_for_records`; `backend/app/src/graphics/sources/graphics.rs:661` copies every nonempty manual value without length or count validation; `backend/app/src/graphics/templates/graphics.rs:751` invokes `fill_fields` for every visible text layer, and line 605 appends the entire value for each occurrence of the placeholder. All expanded layers remain allocated in the spec's vector. This happens before `spawn_blocking` at `backend/app/src/graphics.rs:170`, so engine pixel/scroll/GIF budgets cannot prevent the initial allocations.
- Reproducible construction: a channel administrator can save a temporary template containing 64 visible text layers, each with `"{x}".repeat(333)` (999 characters, within the existing 1000-character template limit at line 445 and 64-layer limit at line 503). A subsequent take with a 1 MiB ASCII `fields.x` has a JSON body of 1,048,611 bytes, yet expands to 349,175,808 bytes in one layer and 22,347,251,712 bytes across 64 layers (about 20.8 GiB). An administrator of a single channel can construct this without GlobalAdmin privileges. A regular user can supply the payload when a suitable template already exists. Even less repetitive templates pass very large strings into text shaping.
- Impact / blast radius: memory exhaustion or extreme allocation/shaping CPU in the common backend process; all channels and APIs hosted by that process can lose continuity. Actual OOM and broadcast interruption were deliberately not attempted.
- Relevant comparison: scheduled takes already reject more than 64 fields, field values above 1000 characters, and more than 16 record selectors at `backend/app/src/utils/schedule/graphics.rs:167`. Manual takes omit those checks; final expanded text also has no budget.
- Next minimized test: use the existing isolated API security fixture with an admin-authored simple `{x}` template and a regular channel user; submit a 1001-character value and expect a validation error. Add a bounded spec-construction test using repeated `{x}` placeholders and small values to verify a total expanded-text budget is enforced before allocation. Do not run a multi-GiB/OOM reproduction.

## SEC-A2 — MEDIUM — queued wait takes survive operator cancellation and template deletion

- Primary location: `backend/app/src/graphics.rs:231`.
- Status: **VERIFIED_STATIC**. Runtime: **NOT_RUN**.
- Source/precondition: any authenticated user of the channel can take an existing temporary template with `during_lshape = wait` while the channel's L-shape is on (`backend/app/src/api/routes/graphics.rs:320`, then `backend/app/src/graphics.rs:219`).
- Complete trace: the wait branch spawns a detached task at line 231 capturing the manager, template, already-prepared spec and supplied values. It polls for up to an hour and then calls `draw_and_take` at lines 239–247. The response at line 257 exposes only `Waiting`, without a pending-command ID. The task has no cancellation token, queue registration, capacity check, or edit-generation check. `POST /graphics/{id}/out` at `backend/app/src/api/routes/graphics.rs:359` only takes out active engine graphics, and template deletion at lines 255–265 only identifies active instances. Both therefore miss a waiting task. Deletion updates the graphics edit generation, but the wait task never reads that generation or reloads the saved template.
- Deterministic sequence: create a wait template, turn L-shape on, take it and receive `waiting`; then call Out-all or delete the template successfully; turn L-shape off. The detached task still publishes the captured template, including after its deletion. It also survives the playout ticker cancellation at `backend/app/src/player/output/playout.rs:388`, since it has no reference to that token.
- Impact / blast radius: stale graphics can go on air after an operator cleared them or removed their model. Repeated accepted wait requests also retain separate captured request data/tasks for an hour without the 16-on-air cap applying, creating an additional process-memory exhaustion path. The directly affected visible output is the authorized channel; resource exhaustion can affect other channels sharing the process.
- Next minimized test: extend the existing API fixture wait test (`backend/app/tests/api/graphics.rs:356`) with two small cases: enqueue → Out-all → L-shape off, and enqueue → DELETE template → L-shape off; assert neither publishes a graphic. Use controlled task time or the existing short polling interval and tiny synthetic/no-media layers. Separately verify pending takes have a bounded admission policy.

## Checked boundaries and rejected suspicions

- Font upload/edit routes check admin role and channel membership before domain work. Download lookup constrains both channel and font ID (`backend/app/src/api/routes/graphics/fonts/graphics.rs:74`). Bytes have a 10 MiB route limit and size/magic/one-face parsing checks. Uploaded aliases are UUID-based and template validation checks channel ownership. Persistent 32-font and 16-source quotas also have SQLite insert triggers in migrations 00032 and 00033; a read/count race alone is therefore **not** a confirmed quota bypass.
- New template/source/binding lookups and modifications inspected use channel-bound SQL parameters. Manual control by Role::User follows the pre-existing operator control permission model; it is not itself classified as privilege escalation.
- Image paths, including manual image placeholders and mapped feed image values, use `norm_abs_path`, which rejects parent traversal and checks resolved existing ancestors. Compatibility swap paths reuse it. No new path/symlink escape was established. SVG is not an accepted Graphics image extension. Existing file-preview routes add `nosniff` and sandbox active content; those serving semantics are unchanged in the reviewed diff.
- Feed fetching rejects non-HTTP(S), embedded credentials and nonpublic literal addresses; resolves and rejects every nonpublic DNS answer, pins addresses on the reqwest client, disables automatic redirects and validates each redirect destination. Documents are limited while streaming to 1 MiB, fetches have deadlines, parsing occurs off the frame thread, JSON has serde's normal depth checks, and RSS rejects document types. No confirmed SSRF, XXE or unbounded-feed-body finding is asserted. Proxy-mediated DNS and live redirect behavior were not runtime-tested.
- Configuration route extraction keeps role/channel authorization at the HTTP boundary before calling the new update service. Key comparison was moved into `api/auth.rs` without changing its logic. Existing updater/release machinery does not authorize release operations.

## Evidence ledger and limits

| Check | Status | Evidence / limit |
| --- | --- | --- |
| Changed security paths and caller/callee tracing | PASS | Read-only source/diff review described above; findings are static |
| Safe allocation arithmetic | PASS | Python computed request size and expansion lengths; did not allocate expanded strings or invoke product code |
| Font/parser/API tests | NOT_RUN | No build or server started; existing tests inspected only |
| Unauthenticated/non-admin/cross-channel staging checks | NOT_RUN | Parent owns isolated staging |
| SSRF redirects/DNS/native image runtime checks | NOT_RUN | No network attack or decoder execution |
| Formatting / Clippy | NOT_RUN | No Rust files changed; toolchain absent on this shell PATH |
| Media output / restart / persistence | NOT_RUN | No runtime readiness claim |

Static security handoff: **NOT_READY** due to SEC-A1. This is a gate assessment, not the round-C advisory ballot and not deployment authorization.
