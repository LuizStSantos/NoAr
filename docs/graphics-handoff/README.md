# Graphics implementation handoff

Implementation branch: `claude/branch-review-opz967`. Before changing graphics,
read this file, [the roadmap](../graphics-roadmap.md), `CLAUDE.md` and
`backend/AGENTS.md`.

## Current pre-release verification

Status: **IN PROGRESS — NOT READY FOR RELEASE**. The current review covers
converted display names, graphics lifecycle/rendering, migrations, uploads,
the code reorganization, comparison with main, a 90-minute graphics soak and
1080p25 processing measurements. Record results from executed checks only.
Council findings and pending checks do not authorize a release.

The published fire test at `f925f608` was `PASS_WITH_CAPTURE_RECHECK`, not a clean
PASS: a short-mark snapshot was rechecked across decoded frames. Its audio-only
UDP picture hold of 21.513 seconds and SRT-loss pacing reset of 1 second require
main-versus-branch investigation in this review. Old artifacts do not verify
the new fixes.

## Implementation contract

- Composer layers are the single source of graphics geometry, assets, opacity
  and animation. Channel bindings select logo, ticker and L-shape models.
  Legacy logo/L-shape/scene routes adapt to the same controls.
- Logo/L-shape/ticker Edit opens Composer. Scenes have an action editor inside
  Graphics. Player displays graphics status and links to Graphics.
- Templates support user groups, deletion, shapes, clock layers, uploaded fonts,
  CSV/RSS/JSON field mappings, animated GIFs and horizontal scrolling text.
  A fresh channel has one removable Logo example.
- New Scheduler graphics events reference models and their fields/records;
  existing scene snapshots retain compatible behavior. XMLTV logo and WebVTT
  remain distinct from on-air composition.
- Converted default names are localized only for display. Saved names, layer
  IDs, bindings and custom user names must survive saves and repeated startup
  conversion unchanged.

See [data-source guidance](../graphics-data-sources.md),
[logo/control compatibility](../logo.md), [Scheduler](../scheduler.md),
[API contracts](../api.md) and [maintenance ownership](../graphics-maintenance.md).

## Migration invariants

Migrations **00029 through 00034 are already installed in real databases**.
Never renumber, squash, edit or replace them. Fix conversion behavior in code,
or add a new migration if a schema change is necessary. Preserve SQLx checksums
and the frozen legacy configuration columns. Startup conversion runs
transactionally and must not recreate intentionally deleted models or overwrite
edited layers. Downgrade uses the pre-migration backup and old image; see
[Docker rollback](../docker.md).

The retained [compatibility matrix](compat-c-matrix.md) describes its historical
76-response/graphics checks and explicitly bounded output evidence.

## Evidence archive

Full historical handoffs, approved HTML prototypes, raw JSON, compressed logs,
archives, duplicate captures and subsequent detailed test artifacts live on
[evidence/graphics-2.3](https://github.com/LuizStSantos/NoAr/tree/evidence/graphics-2.3/docs/graphics-handoff).
Its starting commit is `f925f608`, preserving the original evidence before this
cleanup. Follow that branch for raw reports rather than adding large payloads
back to this directory.

This implementation branch retains this handoff, concise matrices and at most
20 principal review images. Desktop UI captures are 1440 px wide, phone captures
are 390 px wide/full-page, and output frames are 640×360. Image captions identify
their historical scope; they are not new runtime assertions.

| Area | Principal image |
| --- | --- |
| Composer / checkerboard and guides | [Desktop](preview-checkerboard-composer-1440-pt-BR.jpg), [phone](preview-checkerboard-composer-390-pt-BR.jpg) |
| On air / actual monitor | [Desktop](fire-test-2026-11-08/fire-onair-verified-pt-BR.jpg), [phone](fire-test-2026-11-08/fire-onair-verified-mobile-pt-BR.jpg) |
| Templates | [Desktop viewport](fire-test-2026-11-08/fire-models-pt-BR.jpg), [phone](preview-checkerboard-templates-390-pt-BR.jpg) |
| Scheduler | [Model scheduling](fire-test-2026-11-08/fire-scheduler-pt-BR.jpg) |
| Converted logo / L-shape editing | [Logo](composer-layers-logo-pt-BR.jpg), [L-shape](composer-layers-lshape-pt-BR.jpg) |
| Shapes and fonts | [Shapes](composer-shapes-pt-BR.jpg), [uploaded font](composer-uploaded-font-pt-BR.jpg) |
| Data and scrolling | [Mapping](composer-data-mapping-pt-BR.jpg), [CSV](data-sources-csv-pt-BR.jpg), [scroll](composer-scroll-pt-BR.jpg) |
| Guide logo | [XMLTV](composer-layers-xmltv-pt-BR.jpg) |
| Actual HLS | [L-shape with GC](fire-test-2026-11-08/output-fire-lshape-and-gc.jpg), [clip mark](fire-test-2026-11-08/output-fire-clip-lshape-aligned.jpg) |
| Continuity checkpoints | [Restart](fire-test-2026-11-08/output-fire-after-restart.jpg), [UDP recovery](fire-test-2026-11-08/output-fire-udp-recovered.jpg), [SRT/logo](fire-test-2026-11-08/output-fire-srt-live.jpg) |
