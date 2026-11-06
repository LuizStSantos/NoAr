# Open templates, fonts, clocks and data sources: proposed revision

Status: approved for implementation. Completion and executed checks are recorded
separately in the handoff. Approval does not imply implemented/tested behavior. Branch: `claude/branch-review-opz967`.

## Requested operator model

Every graphic is an operator-authored template. Its name and group are editable;
a group is organization only, never an implicit rendering mode. Operators can
create groups such as Channel, Programme, News or Sponsors. Converted logo,
L-shape and ticker templates remain editable starting points and can be deleted.
The library must not force those templates into reserved display groups or
replace their authored names with role labels.

Composer combines image, text, shape, video-window and clock layers in one saved
composition. A logo may therefore include an image and text. Template fields
can be supplied manually or mapped to a saved data source. The same template
can be used with different field values without copying its composition.

Always on air is available on compatible ordinary templates, not restricted to
the imported logo/ticker roles. L-shape templates retain explicit video-window
entry/exit behavior. An optional compatibility assignment selects which template
and image layer existing logo/L-shape/ticker controls target. That assignment is
separate from its group and does not create a second layout definition.

A fresh installation and each newly created channel start with only one removable
logo example, at its natural size with 12 px top/right margins. Empty ticker,
L-shape or clock examples are not created. Upgrades preserve existing models.

Groups remain free labels on templates: empty groups have no separate record;
editing a template's group moves that template without changing render behavior.
Removing the last template removes that group from the picker.

## Stage 1: templates, groups, assignment and deletion

- Show authored names and groups for all editable template cards. Show an
  optional channel-control assignment as metadata, rather than a reserved group.
- Add template deletion with a usage summary and explicit confirmation. Show
  active use and references before deleting. Remove the active graphic and
  detach its channel assignment in a coordinated operation. Never delete its
  uploaded images or fonts implicitly.
- Permit deleting imported role templates. Record intentional removal so startup
  conversion cannot recreate them from the frozen legacy configuration.
- Let a newly created template receive an optional compatibility assignment in
  Composer. Replace an assignment explicitly, without renaming or copying the
  template. Allow the channel to have no assigned logo or ticker.
- Generalize permanent template state so multiple user-authored compatible
  templates can stay on air. Keep their startup visibility separate from current
  requested visibility. Do not infer either from their group name.
- Use a new normal sqlx migration for required schema changes. Preserve earlier
  migration checksums, existing template IDs, authored layers, groups and frozen
  processing columns. Conversion remains repeatable and does not change the
  output merely because the application starts.
- Preserve old API/key/scene/clip behavior against the selected compatibility
  assignment. Removing an assignment disables that graphic; old controls must
  not silently manufacture a new model or restore frozen legacy artwork.

Tests: create/rename a group; combine image and text; reassign a role; delete an
ordinary, assigned and active template; restart without resurrection; repeat
conversion; verify persistence and existing API compatibility. Verify permanent
models and their temporary visibility across playout/backend restart.

## Stage 2: shapes and clock

- Expand Shape with rectangle, rounded rectangle, ellipse/circle and line.
  Provide applicable fill, stroke and stroke-width controls. Keep the existing
  drag, size, proportional, opacity and animation controls.
- Add Clock as a real rendered layer with time, date or combined display, an
  explicit format and the channel timezone by default. A default `HH:mm:ss`
  display is editable. It can share a template with artwork and other text.
- Use server/channel time for actual output. Updating a clock does not restart
  its entry animation. Preview must reflect the saved format and timezone;
  a saved template is not a frozen bitmap of the authoring time.
- Clock support is explicitly included by the user's new request, superseding
  its exclusion in the earlier delivery.

Tests: preview/render geometry for each shape; outline/opacity; clock changes on
actual output; timezone/date rollover; entry/exit animations; save/reload;
permanent clock beside another permanent composition.

## Stage 3: font upload

- Add Upload font next to the Composer text/clock font selector. Accept validated
  TTF/OTF files and retain them in persistent storage, without a Docker rebuild
  or manual installation inside the container.
- Register the uploaded font for the server renderer and load the corresponding
  font for browser preview. A browser-only font installation is insufficient.
- Preserve font-file identity so duplicate names and uploads cannot silently
  replace a font used by another composition. Make unavailable fonts explicit.

Tests: valid/invalid upload, duplicate-name handling, preview/server family
selection, text/clock HLS rendering and persistence after backend restart.

## Stage 4: data sources

Add **Graphics > Data sources** on desktop and the phone page selector. Initial
sources cover RSS feeds, CSV spreadsheets (including a Google Sheets published
CSV URL), uploaded CSV data and HTTP JSON APIs for quotations and other values.
Private Google Sheets account/OAuth integration is not implicitly claimed by
published-CSV support; it requires a separately agreed authentication flow.

Each source has a name, type, update interval, field/column mapping, preview,
manual refresh and last-success/error status. Composer can map named fields such
as `{symbol}`, `{price}`, `{headline}` and image fields to a source record.
Operators can preview a record before taking the composition on air.

Fetching, parsing and rendering updates run outside the playout frame thread.
Live updates change mapped values without resetting placement, entry animation
or take time. Keep the last successful data during temporary source failure and
show its age/error. Never replace the picture with an unmarked empty result.
Use the existing authorization/channel boundaries and bounded URL-fetch policy.

Tests: RSS/CSV/JSON parsing and mappings, invalid/missing fields, source errors,
manual/interval refresh, last-good fallback, image-field mapping and actual HLS
value changes while audio/video continues. Test published-CSV integration with
controlled fixtures; document whether a real external provider was reachable.

## Delivery and verification

For each implementation stage, update the handoff, make an English commit only
on the authorized branch, and supply pt-BR desktop/phone screenshots. Use the
running backend, disposable SQLite and synthetic output. Run Vue type checks,
ESLint, Prettier and the browser suite. Every Rust change requires `cargo fmt
--all`, `cargo clippy --no-default-features --all-targets -- --deny warnings` and
`cargo test --no-default-features` with GStreamer 1.26.

Retain regression coverage for the 76 legacy HTTP responses, clip logo/L-shape
marks, scenes, scheduled-event reversal, image/placement overrides, logo
transitions, XMLTV and desktop output. Repeat relevant deterministic raw/HLS
comparisons after renderer changes, documenting compression tolerance and
requiring exact model position, size and timing. State every unavailable check
explicitly. A planned test is not a passing result.
