# Changelog

Every release of NoAr, newest first. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html); see
[docs/releasing.md](docs/releasing.md) for what each part of the number means
here.

Work merged since the last release goes under **Unreleased** until the next
one is tagged.

## [Unreleased]

### Removed

- **Breaking:** the keyed `/logo/{id}/{action}`, `/lshape/{id}/{action}` and
  `/scene/{id}/{name}` routes and `/api/control/{id}/logo|lshape|scene`. Use
  the graphics commands instead: `/graphics/{id}/take/{template}` and
  `/graphics/{id}/out/{template}` with the same graphics key, or
  `POST /api/graphics/{id}/take|out` with a login token. Encoders, vMix, OBS
  or Stream Deck buttons that call the old URLs must be updated.

### Changed

- Separate configuration updates, scheduled playback, XMLTV serialization and
  native output policy into domain owners; split playlist, Scheduler, Storage,
  report and device-discovery coordination from their presentation panels.
  Preserve APIs, migrations and operator behavior, document source ownership,
  and guard presentation/data boundaries in architecture checks.

- Organize logo, L-shape and model rendering under the Graphics domain; move
  shared controller actions out of HTTP handlers and separate Composer panels.
- Remove unreachable legacy graphics editors, give subtitles/controller examples
  their own components, and document ownership and maintenance boundaries.
- Enforce service/API/data/engine dependencies with source-level CI checks.
- Refresh the API tutorial to describe live clock layers, mapped text and free
  permanent compositions.

### Fixed

- Localize converted channel-template and generated-layer names for display
  without rewriting saved identities or custom names.
- Preserve authored fractional logo geometry on compatibility saves and output
  changes; adopt surviving intermediate-conversion bindings without reimporting
  frozen settings. Keep installed migration checksums unchanged.
- Bound manual graphics expansion and pending takes, cancel stale work on edits
  and output lifecycle changes, and isolate unavailable optional permanent art.
- Cache model-logo movement, prepare artwork outside frame-critical locks, and
  preserve animation epochs and controlled-image indices on refresh.
- Save Composer content and channel assignment atomically; protect dirty
  navigation and asynchronous authoring operations, restore natural image size,
  and expose visible, keyboard-operable resize and upload controls.

- Share Composer's neutral transparency checkerboard with On air and template
  thumbnails; keep alignment grids and safety margins specific to the editor.
- Capture every README gallery image at exactly 1440×1000 in dark mode, including
  Templates, and reject PNG dimension mismatches in the screenshot helper.

- Preserve a ticker hide issued before its runtime definition loads, including
  the next playout start, without changing its saved startup visibility.

- Clear external and clip artwork/placement when an assigned template is deleted
  or its compatibility binding is removed; leaving L-shape returns to full screen.

- Desktop output composes the active model, clip or externally replaced logo
  into the video frame, matching encoded output instead of using a stale
  startup bitmap. L-shape artwork replacements and image-only clip overrides
  preserve the assigned model's freely positioned picture window.

### Added

- One take/out command pair for every graphics template, GC, L-shape, logo
  or ticker alike, by id or by name, with an optional time on air
  (`duration`, 0 keeps it on until taken off), with a login token or the
  graphics key. The External control page shows them ready to copy.
- Graphics cues in a video's marks (Storage › Marks › Graphics): at a time of
  the video, put a template on air for a time or until taken off, or take one
  off. They run while the video plays, from wherever it starts.

- Author continuous left/right scrolling on Composer text layers, with output-pixel
  speed, repeat spacing, clipped previews, manual fields and live data mappings.
  Temporary and permanent models share the existing take clock; static models
  retain their behavior without a database migration.

- Download stored media and sidecar files from Storage, with fresh access tickets
  for list, image-grid and viewer actions; accept WebVTT subtitle uploads.

- Replace Composer's scenery background with a neutral checkerboard and 10%
  grid, plus optional action/text safety guides at 5%/10% from each edge.

- Schedule Composer models with previews, fields, data records and second-level
  time inputs; retain legacy action events through migration `00034`. Each timed
  composition owns its take, and permanent visibility leases restore prior state.
- Play bounded animated GIFs with transparency and per-frame delays in image
  layers, logo and L-shape artwork. First image insertion uses natural output
  geometry, including full-frame artwork; replacements retain authored layout.

- Add Graphics Data sources for uploaded/published CSV (including public Google
  Sheets CSV), RSS/Atom and HTTP JSON, with Composer field mappings, record
  selection, live updates and persistent last-good data during provider failures.

- Upload immutable channel fonts from Composer for text/clock layers; the browser
  and server use the same bytes, persisted in SQLite and restored on restart.

- Add Composer shape outlines, rounded rectangles, ellipses and lines, plus live
  server-time clock layers with channel or explicit timezone and date formats.

- Free template names/groups, optional compatibility assignments with explicit
  image targets, deletion of assigned models with a usage confirmation, and
  user-authored permanent compositions. Fresh installations/new channels start
  with only one removable logo example; existing installations retain their models.

- The graphics API tutorial explains layer-based channel models, startup
  visibility and temporary take/out, using real model IDs and separate token/key
  examples. A disposable-machine Compose file builds this branch locally.

- Channel logo, L-shape and ticker models are edited as Composer layers, with
  permanent visibility and video-window corner/size controls. Migration `00030`
  upgrades already converted layouts without duplicating models or changing
  frozen legacy columns. Compatibility image/placement controls act on the
  composed layers and retain additional artwork. The independent guide icon
  has an XMLTV logo tab in Graphics.

- Assigned channel graphics and scenes have separate library cards, localized
  role names, saved-model previews, visibility/scene controls and Composer editing for channel layers
  (with a scene action editor). Logo, L-shape and Quick scenes no longer have separate menu entries;
  old browser URLs redirect with the channel preserved. Manage scenes remains
  available in an empty library group. Static ticker text is edited in Composer. Assigned models
  are excluded from the ordinary On air GC picker.
- Channel graphics conversion: sqlx migration `00029` and automatic, atomic
  conversion to bound logo, ticker and L-shape templates and quick-scene models.
  Compatibility configuration writes save these models; legacy columns stay
  unchanged. Returning to an older image requires restoring the pre-update
  database backup. See [Docker rollback](docs/docker.md#restoring-before-graphics-conversion-00029).
- Graphics Composer: admins can create and edit templates from Templates or
  On air, arrange and resize layers in percent or output pixels, lock proportions,
  undo edits, choose fonts and fields, upload or select storage artwork, set a
  video corner, preview animation timing and save a named template in a group.
  Failed loads cannot overwrite a template; failed saves preserve the draft.
- Graphics templates: channels store their own graphics (layers of text,
  images and boxes in percent of the frame, with `{field}` names) through
  `/api/graphics/{id}/templates`. Admins create and change them; everyone on
  the channel can read them. See
  [docs/graphics-roadmap.md](docs/graphics-roadmap.md).
- Templates go on air through `/api/graphics/{id}/take`, `/out` and
  `/on-air`, with their fields filled in and entry and exit animations
  (fade, slide, wipe). They are drawn under the L-shape, so a lower third
  follows the picture into the L-shape's corner; a template can also wait
  for the L-shape to end or skip it.
- The Graphics tab opens on **On air**, the operator's screen: a preview
  that draws the picked template with what is typed in its fields, a program
  monitor with the real output, Put on air / Take out / Next, the graphics on
  air, and below them the live logo, L-shape and quick scene controls of the
  former Live now page, which now opens On air. **Templates** shows the
  library as cards, filtered by group. The menu keeps Logo, L-shape and Quick
  scenes apart, as channel settings, until they move over to templates.

### Changed

- The soak test checks decoded moving pictures as well as transport progress,
  detecting frozen video even while output bytes and `/health` keep advancing.
  Its CI matrix covers SRT and HLS separately, with bounded allowances for the
  intentional live-source faults. NDI and SDI still require separate fixtures.

## [2.2.1] - 2026-11-05

### Changed

- The Docker documentation explains how to encode on an NVIDIA card (NVENC),
  and `docker-compose.yml` carries the settings, commented out. Not yet
  tested on NVIDIA hardware.
- `Cargo.toml` names the license `GPL-3.0-only` (the current identifier of
  the same GPL v3), and the image keeps the license of each GStreamer part it
  builds.

### Fixed

- Generating a day from folders of long programmes left a gap at its end
  (for example 18 minutes when the next episode was an hour long), since a
  programme that did not fit was left out and only a filler could close the
  day. Without a filler, a short day made the channel start the next day's
  playlist early. The next programme now closes the day, cut where the day
  ends.
- The Storage page's folder panel matches the Playlist page's library: the
  same width and a title like the library's. The disk space no longer breaks
  into a column of short lines.
- The Playlist page's library no longer stretches to the bottom of the screen
  with nothing in it: it is as tall as its folders and files, at most as tall
  as the playlist, and scrolls inside.
- The Log page's level buttons show their own level only: Information shows
  information lines, Warnings warnings, Errors errors, and Everything shows
  all. Before, each also showed the levels above it, so filtering by
  Information still listed warnings and errors. The page opens on
  Everything.

## [2.2.0] - 2026-11-04

### Added

- `GET /health` for uptime monitors: `200` when every channel set to run is
  on air, `503` when one is down, failed or frozen (running, but its output
  carried nothing for over 10 s). See [docs/api.md](docs/api.md#health-check).

### Fixed

- NoAr aborted while playing an H.264 video whose width is not a multiple of
  16, such as 854×480 (`video meta uses 864x480 instead of 854x480`): a fault
  of gst-libav 1.26.2's decoder. The image's gst-libav is now 1.26.11, built
  from the release sources like its GStreamer core. Found by the new soak test.

## [2.1.4] - 2026-11-04

### Fixed

- After the playout started or restarted in the middle of a clip, the Player
  showed only the rest of the clip as its length (for example "0:12 of 0:13"
  for a 20 s clip) and the wrong time left. The status reports the whole clip
  again.

## [2.1.3] - 2026-11-03

### Changed

- The image's GStreamer core and gst-plugins-base are 1.26.11, built from the
  release sources over Debian trixie's 1.26.2; the other plugin sets stay
  Debian's packages. Provenance: `/usr/local/share/noar/gstreamer-build.txt`.

### Fixed

- NoAr no longer stops when a live MPEG-TS source changes its streams (an
  encoder that drops its video, say): GStreamer 1.26.2's `decodebin3` aborted
  the process ("Stream switch requested for future collection"), which
  1.26.11 handles.
- Starting a clip or a source could hang for good, freezing the channel
  until the playout was restarted: an appsink waiting for room in its full
  queue held the lock that the change to playing needed, while nobody pulled
  its samples. The queues have no limit for the moment of that change.
- An RTMP live from a server that sends sound before the picture's first key
  frame (MediaMTX relaying an SRT or MPEG-TS source) dropped and reconnected
  every few seconds: the source opened as sound only, its picture branch was
  removed as for a file without a picture, and the picture arriving next
  stopped the whole source. Network sources now keep both branches, and a
  live follows a track announced late.

## [2.1.2] - 2026-11-03

### Changed

- The log says why a live's signal check failed (the connection error, "still
  connecting, or no picture yet" or "no media" after 12 s) instead of only "no signal".

### Fixed

- A live whose picture stopped while its sound kept arriving (a stalled
  decoder after packet loss, an encoder or NDI sender that dropped its video)
  froze the output with no warning until the playout was restarted: sound
  alone counted as signal. A live with a picture that sends no video for 6 s
  is now lost like one that sends nothing, so the waiting screen airs and the
  source is connected again; its signal check also waits for a picture, so
  sound alone does not put it back on air. This covers every live input
  (SRT, RTMP, RTSP, RTP, UDP, TCP, RIST, NDI and video cards).

## [2.1.1] - 2026-11-03

### Fixed

- Clips no longer freeze the picture for up to 12 s now and then when they
  start. GStreamer sometimes stalls while discovering a file's tracks, with
  nothing flowing; a file in the storage that has not announced its tracks
  after 2 s (4 s, then 6 s on later tries) is now opened again, within the
  same 12 s limit. Network sources and live signals keep their wait.
- A clip whose decoder fails before its first frame ("negotiation problem",
  "decoder not initialized", a race inside GStreamer when the clip starts
  with a seek) is opened once more instead of airing the fallback for the
  whole clip.

## [2.1.0] - 2026-11-03

### Added

- _Ident or promo_ in a video's marks (_Storage › trim, logo, breaks and
  L-shape_): a marked clip airs inside the program before it, so the Player
  keeps that program's name and the XMLTV guide does not list it. Saved as
  `"insert": true` in the video's `<name>.marks.json`, not in a sheet, so a
  clip in a series folder keeps using the folder's `.nfo`. The report items
  gain an `insert` field.
- Assembled programs (`.program`, made with _Storage › New program_) are
  listed in the Scheduler's storage panel and in the event dialog, so they
  can be scheduled.
- `docker/ndi/enable-ndi.sh --test` sets NDI up for the isolated GStreamer
  test project, and `--compose-only` writes only the Compose settings when the
  runtime is already installed.

### Changed

- NoAr no longer links the FFmpeg libraries: the engine keeps video and audio
  in its own frames and time bases, and GStreamer does all decoding, encoding
  and muxing. Output is unchanged (decoded frames are identical to the
  previous release's with lossless encoding). Building needs neither the
  `libav*-dev`/`libsw*-dev` packages nor clang; the `ffmpeg-filter` and
  `ffmpeg-device` Cargo features are gone. The image still carries
  `gstreamer1.0-libav` for codecs GStreamer has no native decoder for.
- The log level of the media engine's messages is `logging.media_level` in the
  configuration API and the database (migration 00027 renames the column).
  The API still accepts `ffmpeg_level` and returns it as a deprecated copy,
  so older clients keep working. Internal names follow (`MediaCapabilities`,
  `OutputTarget`, `media_log_level`, `log_ignore_lines`), and three unused
  `ffmpeg_*` fields of the general settings are gone.
- Idents are no longer guessed. Short clips without a sheet used to count as
  idents of the program before them, so a playlist of music videos aired in
  the Player and the guide under one clip's name. Every clip is now a program
  under its own name unless its marks say _Ident or promo_. **On updating:**
  mark the idents and promos that should stay inside their programs.
- `index` in the playout status (`media/current` and the event stream) is the
  entry of the saved playlist on air; the clips of an assembled program share
  their entry, and `elapsed` counts from its start. During a scheduled event
  `media` covers the whole event.

### Fixed

- NDI output is announced to receivers when NoAr runs in Docker:
  `docker/ndi/enable-ndi.sh` mounts the server's `/run/dbus`, through which
  the NDI library registers its sources with avahi-daemon. **On updating:**
  run the script again (or add the line from docs/ndi.md) and recreate the
  container. NDI output was verified on a real network (picture and sound on
  a receiver).
- NDI and SDI outputs that fall behind for a moment drop frames and catch up
  instead of taking the channel off air; they hold up to a second of video,
  and the playout restarts them only after five seconds without taking any.
- The Player keeps the clip on air (name and progress) after an assembled
  program in the playlist, the progress of a scheduled program runs over the
  whole program instead of restarting with each block, and _Up next_ and the
  playlist's past and coming entries take the time shift into account.
- `docker/ndi/enable-ndi.sh` run with sudo gives the Compose file it writes
  to the user who ran it.

## [2.0.0] - 2026-11-02

### Added

- Assembled programs: a `.program` file in Storage holds a program's blocks in
  order and commercial breaks between them under a chosen title. Drag it into
  a playlist or schedule it as one item; the playout expands it when it airs,
  so edits apply to every airing still to come. The guide shows one
  programme; the as-run log lists every clip. See `docs/programs.md`.

- NDI output: publish the uncompressed program as an NDI source on the local
  network (_Settings › Output › NDI_). The NDI elements are compiled in; the
  proprietary NDI Runtime is installed on the server and NDI stays disabled,
  with an explanation, until it is present. Migration `00024` adds an inactive
  NDI output to every channel. `docker/ndi/enable-ndi.sh` installs the
  runtime (after the NDI license is accepted), mDNS discovery and the Compose
  override in one step. See `docs/ndi.md`.
- NDI input for lives: `ndi://SERVER (Source name)` addresses in Storage and
  the Scheduler, a _Find NDI sources_ list from network discovery
  (`GET /api/schedule/{id}/lives/ndi`) and the signal test. Picture-only
  sources are accepted and get silence.
- Video cards: play the program on a Blackmagic DeckLink card
  (_Settings › Output › SDI_, any DeckLink/UltraStudio sub-device, NTSC/PAL
  to 2160p modes, including 1080i59.94, 1080p29.97 and 1080p30), and take
  DeckLink or V4L2 capture cards as lives (`decklink://`, `v4l2://`).
  `GET /api/playout/devices/{id}` lists the cards the server sees. Migration
  `00025` adds an inactive SDI output to every channel. See
  `docs/video-cards.md`.

- A disposable actual-NoAr smoke harness verifies scheduled live transitions,
  source-loss recovery, decoded markers, timestamps, receiver progress and
  persistent SRT sessions independently of the standalone Python prototype.
- Shared GStreamer reference skills for Codex and Claude, with official native
  and Rust documentation, installed-plugin checks and integrated-media test
  requirements.

### Changed

- The live key's tutorial is a section of its own at the bottom of the
  Scheduler, **Live by API**, like the SCTE-35 one: this channel's addresses to
  extend by minutes, until a time or end the live, ready to copy, with curl,
  vMix, Stream Deck and authenticated-API examples. **By API…** in the
  Player's live controls opens it.
- The sign-in page lists what NoAr does now: lives and outputs over NDI and
  video cards, assembled programs, lives that can be extended, SCTE-35 breaks,
  logo and L-shape. The README screenshots show the English interface with a
  demo channel, rebuilt by `qa/screenshots/`.
- Extend the live on air from the Player's Now panel (+5/+15/+30 min, or until
  a time) or with `POST /api/schedule/{id}/extend`, for this airing only; it
  goes on without reconnecting. The panel names the end and ends the live at
  once with its button (`/stop`).
- A channel **live key** (migration `00026`) lets encoders and scripts extend
  or end the live on air without logging in:
  `/live/{id}/extend?key=…&minutes=30|until=13:30` and `/live/{id}/end?key=…`,
  set under _Scheduler › Lives › Live remote control_.
- Lives go into playlists (dragged from the Player library for their default
  length) and into assembled programs as a **Live** part with its own length;
  both air the live's current address. A program's clips follow trims made
  in Storage after it was saved (programs keep each file's length), and keep
  their own logo, L-shape and break marks.
- NDI, DeckLink and V4L2 addresses are no longer taken for storage paths in
  playlists.
- A live's guide data also takes its year, countries and credits (hosts, cast
  and crew, once saved), like a clip's sheet, and a **default length**
  (`<runtime>` in its `tvshow.nfo`, minutes). Scheduling the live proposes its
  end from that length (12:00 with 60 min ends at 13:00) and follows changes
  of the start until the end is edited by hand.
- Encoded outputs can select a 32, 44.1 or 48 kHz audio sample rate
  (migration `00023`, default 48 kHz). The engine keeps processing at 48 kHz
  and resamples just before the audio encoder; recordings with their own
  encoding settings stay at 48 kHz.
- Use GStreamer as the single playout engine for decoding, normalization,
  encoding, muxing and media transports. FFmpeg frame structures remain the
  compositor/audio memory interface and FFmpeg tools remain available for tests.
- Configure live events with read URLs from an external service such as
  MediaMTX. Clock and Continue scheduling remain available.
- Install GStreamer development libraries and runtime plugins in the existing
  Docker image and validation workflow.

### Removed

- Built-in RTMP/SRT input servers, their runtime controls and the input-server
  settings page. Historical database rows remain stored; current configuration
  responses omit input-server settings. **Breaking**: encoders can no longer
  push to NoAr on ports 1936 (RTMP) or 9000/udp (SRT); `docker-compose.yml` no
  longer publishes them, so remove those two port lines from your own copy and
  send streams to an external service such as MediaMTX, then schedule its read
  URL as a live.

### Fixed

- Building the image no longer fails when a slow link cuts the SRT source
  download: it uses HTTP/1.1 and retries on any transfer error (the SHA-256
  check still applies).
- A playout that cannot go on air (a DeckLink card missing, an output that
  does not open) no longer looks on air or stopped while NoAr retries it: the
  Player and the header show OFF AIR with the reason and the next attempt, and
  the status (`media/current`, playout events) carries a `failure` field. The
  Player's format label also names NDI and SDI outputs instead of "Desktop".
- The LIVE mark on the playlist's day bar no longer breaks letter by letter
  when the live is short or the screen narrow.
- An unknown address (an old bookmark, a page that was removed) opens the home
  page instead of a blank screen without the menu.
- Building the image behind a proxy with the `proxy_ca` secret: apt can read
  the certificate, and Debian packages come over HTTPS for proxies that refuse
  plain HTTP. Builds without the secret are unchanged.
- Ending a live twice in quick succession (a double press on a Stream Deck,
  or the Player and the API at once) no longer leaves a pending skip that
  could cut the programme clip coming back after the live.
- Clips without sound (common in stock footage), or without picture, can be
  resumed in their middle again. Their unused audio or video branch stayed in
  the pipeline unlinked and made every seek fail ("Failed to seek; fallback
  generated", black on air), for example when the programme continued after
  a scheduled event. The missing branch now leaves once the file has
  announced its tracks.
- Resuming a clip in its middle (for example when the programme continues
  after a scheduled event) no longer fails with "Failed to seek" when the
  clip has not finished prerolling, as with large 4K files: the seek waits
  for the clip to become seekable, up to five seconds, and a failure names the
  position and the clip length.
- Saving the channel logo (image, on/off, size, opacity or position) in the
  Graphics tab puts it on air at once, cross-fading from the previous image,
  instead of requiring a playout restart. An image that cannot be read is
  refused when saving.
- Resolve SRT hostnames in NoAr, bounded by a timeout, and hand GStreamer an
  IP literal. The SRT plugin's own GIO lookup reused a cancellable that state
  changes reset, which aborted (output) or crashed (input) the whole process
  and, with the URL saved, on every restart (#33). SRT `localaddress` must be
  an IP address.
- Keep live audio/video interleaved when only one source track stalls; hold the
  picture or insert silence instead of allowing the output muxer to wait for
  the missing track. Preserve the pacing anchor across unchanged clip rates.

## [1.11.0] - 2026-10-28

### Added

- Repository-local Codex instructions and six skill adapters share NoAr's
  existing development, Rust, Vue, Docker, FFmpeg and video-processing
  references with Claude Code.
- A scoped FFmpeg documentation helper provides official-reference topic
  routing, bounded excerpts, a local cache and native capability checks.
- Scoped Rust, Vue, Docker and video-processing reference playbooks, plus a
  deterministic check planner for changed files, support focused development
  and verification.

### Changed

- Consolidate AI navigation and responsive-layout review workflows, reduce
  default context loading, and validate toolkit scripts and adapters in CI.
- Move AI toolkit installation and maintenance guidance into `docs/ai` and
  exclude development-only Claude tooling and browser tests from Docker builds.
- Skip Docker image publication for documentation, QA and Claude-tooling-only
  pushes to `main`; stable image tags still change only on release tags.

### Fixed

- Docker SRT output stays connected when a scheduled SRT live input closes.
  Use native GnuTLS SRT 1.5.5 with corrected session lifecycle and test that
  closing auxiliary sessions preserves an existing connection.
- Use the SQLite dialect in the repository's SQL formatter configuration.

## [1.10.1] - 2026-10-28

### Added

- For development: `qa/layout/check-layout.cjs` checks every page of a
  disposable staging instance at computer widths or at phone and tablet
  widths (pages scrolling sideways, text past the screen, split words, the
  menu), with screenshots; and two layout agents for Claude Code,
  `noar-desktop-layout` and `noar-mobile-layout`, that use it.

## [1.10.0] - 2026-10-28

### Added

- **A logo of its own for a video**: in a video's marks (Storage, or one
  playlist item), **Logo of this video** shows another image in place of the
  channel's logo while it plays (`logo_image` in the marks), at the channel's
  size and opacity. A swap from outside still goes first.

### Changed

- **Schedule graphics** and **Quick scenes** use the same controls as a
  video's marks: the logo is placed by dragging or typing on a small screen
  (leave it, the channel's place, or its own), the logo action and the
  L-shape action are buttons, and the L-shape corner is picked on a small
  screen.
- **Graphics › Logo** places the logo anywhere by dragging or typing X % ×
  Y %, keeping the margin from the edges; **Graphics › L-shape** picks the
  corner on the same small screen as the marks.

## [1.9.0] - 2026-10-28

### Changed

- The log moved into **Settings › Log** (`/configure/logging`); the old
  `/logging` address leads there. The main menu now reads Home, Player,
  Playlist, Scheduler, Graphics, Storage, Report, Settings.
- Every screenshot in `docs` was taken again.

### Fixed

- The desktop menu stays on one line whatever the channel name: its icons
  hide only when they do not fit, instead of "Settings" dropping to a second
  line.

## [1.8.2] - 2026-10-28

### Fixed

- Dialogs fit the screen of a phone: the Scheduler's "Agendar grafismo" form
  (and any dialog sized to the screen) no longer runs past the dialog's edge.
- On a phone, the editor of a playlist item stays in view instead of being
  cut off by the list that scrolls sideways.

## [1.8.1] - 2026-10-27

### Changed

- On phones and tablets (below 1280 px) the main menu sits behind a menu
  button and opens as a panel: one column on a phone, a grid on a tablet. It
  closes when a page opens or on Escape. Between 1280 and 1439 px the menu
  hides its icons so it stays on one line.

### Fixed

- Long names with no spaces (files, folders, lives, scheduled events) wrap
  inside their card or dialog instead of running past the edge of a phone;
  table cells and buttons keep whole words.
- The on-air badges no longer blink: a channel on air shows as stopped only
  after the status says so for 1.5 s, and nothing is shown until the first
  status arrives instead of "stopped" for a moment.

## [1.8.0] - 2026-10-27

### Added

- **Scheduled graphics** in the Scheduler: at a time, bring the L-shape in
  (graphic, corner and size), show, hide or swap the logo (the channel's one
  too) and move it to a place or a free `X% × Y%` position; undone when the
  event ends. Once, daily or on chosen weekdays, with their own lane on the
  timeline and a list for the day. API under `/api/schedule/{id}/graphics`
  (new migration `00022_graphics_events`).
- **Marks of one playlist item**: in the Playlist page, an item's logo,
  breaks and L-shape marks can be changed for that item alone, saved with the
  playlist in its `marks` field; the video's marks in the Storage stay as
  they are.

### Changed

- **Playlist** is its own item in the main menu (`/playlist`) instead of the
  Player's "Build playlist" tab; the Player shows only what is on air.
- The README lists all of NoAr's features, and every screenshot in `docs`
  was taken again.

## [1.7.0] - 2026-10-27

### Added

- **Logo and L-shape of one video**: in a video's marks, the logo can go
  somewhere else for that video (`logo_position`), and its L-shape can use
  another corner and size (`lshape_corner`, `lshape_scale`) besides its own
  graphic, for that video alone, wherever it plays. A move from outside still
  goes first.
- **Typed positions**: every small screen that places the logo (Graphics ›
  Live now, the Player bar, a video's marks) also takes X % × Y % typed in.
- **Graphics lock in the Player**: like the transport, the graphics bar starts
  locked while the channel is on air.

### Changed

- The marks editor of a video is wider and in two columns: the video and a
  timeline with one lane per kind of mark (cut, logo, breaks, L-shape) on the
  left, the tab's tools and list on the right. Tabs show how many marks each
  has, and a lane opens its tab.

## [1.6.0] - 2026-10-27

### Added

- **Graphics tab** in the main menu. The logo, the L-shape, the subtitles and
  the remote control left Settings for their own pages: **Live now**,
  **Logo**, **L-shape**, **Subtitles**, **Quick scenes** and **External
  control (API)**. Settings › Logo and subtitles links there.
- **Quick scenes**: named sets of logo and L-shape changes (hide, swap or
  move the logo; L-shape in or out with a graphic, corner and size), up to 24
  per channel, applied with one click in Graphics › Live now and in the
  Player, or by an encoder at `/scene/{channel}/{name}?key=…` (and
  `POST /api/control/{id}/scene`). Kept in the channel configuration (new
  migration `00021_graphics_scenes`).
- **Graphics bar in the Player**, under the preview: the logo on and off and
  where it is, the L-shape in and out and its corner (each move confirmed
  first), the quick scenes, and a link to the Graphics tab.

- **Place the logo anywhere.** The logo's `move` takes `x` and `y` from 0
  to 100 (shares of the room around it, on any output size) or `X%:Y%`,
  besides the corners, which now include `top`, `bottom`, `left`, `right`
  and `center`.
- **Move the L-shape video to another corner on air.** The L-shape has a
  `move` action (and `on` takes a `position`) with a corner and `scale` for
  the video's size, for a graphic whose space for the video is in another
  corner, until moved back.
- Settings › Logo › Live graphics: a small screen to place the logo (click,
  drag or arrow keys), and corner buttons and a size slider for the L-shape
  video, each with a button back to its own place. A new place goes on air
  only after **Move on air**, and can be cancelled; until then a dashed box
  shows where it is now.

### Fixed

- A scheduled live from SRT, RTMP, HLS or UDP no longer freezes the channel
  when the source's timestamps jump, as when its encoder restarts: the
  picture goes on from the next frame instead of repeating one frame for the
  length of the jump (while the source piled up unread) or waiting for the
  time to catch up. Pacing no longer waits more than 2 s for one packet.
- When a source changes its picture size or pixel format midway (an encoder
  restarting with other settings), the scaling follows it instead of failing
  the clip and showing black for the rest of the slot.

### Removed

- The German and Russian interface languages, which were mostly
  untranslated. Whoever had chosen one gets the browser's language if NoAr
  has it, else English.

## [1.5.1] - 2026-10-27

### Added

- Marks editor: **Watch stretch**, **Watch it go in** and **Watch it come
  out** on the logo, SCTE-35 breaks and L-shape tabs, like the trim's, and a
  play button on each stretch of the list. They play with 3 seconds around
  and stop by themselves.

### Changed

- The L-shape now takes the logo off before the picture shrinks (0.4 s, then
  0.8 s), and brings it back only after the picture grew back, on air and in
  the marks editor's preview, which also hid nothing of the logo before.
- Settings › Logo: the live card is now **Live graphics: logo and L-shape**,
  right below the L-shape settings. It has a row for the logo (hide, swap,
  position) and one for the L-shape (in, out, swap the graphic, back to the
  channel's), one **graphics key** for both, and a tutorial in two parts with
  every address for each. The L-shape card keeps only its settings.

## [1.5.0] - 2026-10-27

### Added

- **L-shape**: the programme picture shrinks into a corner over a
  full-screen graphic (news, sponsors, what is on next) and grows back,
  animated, on clips, lives and ingest. Set the graphic, the corner and the
  picture's size in Settings › Logo › L-shape, which also switches it on air.
  It goes in for stretches marked in a video (Storage › marks, new L-shape
  tab, kept in `<name>.logo.json` as `"lshape"`) and from encoders with the
  logo key: `/lshape/{id}/{on|off}`, or `/api/control/{id}/lshape` with a
  login. The settings are in migration 00020.
- The uploads panel follows each video after its upload: a blue bar while it
  waits for or goes through conversion to the output format, then whether it
  was converted, was already in the format or failed.
- A video put in a playlist before its conversion plays the converted file;
  one that became an `.mp4` (a WebM, AVI…) plays in place of the original.
- Logo API `move`: put the logo in a corner, at an `X:Y` expression, or back
  in its place (`/logo/{id}/move?position=bottom-left`, also from the logo
  card in Settings). A swapped image goes to the same place.
- L-shape API `replace` and `reset`: swap the graphic before or while it is
  on (`on` with `graphic` swaps and goes in at once), and go back to the
  channel's.
- A video can have its own L-shape graphic for its marked stretches (L-shape
  tab of the marks editor, `"lshape_graphic"`).
- **Download a base for the image editor** in Settings › Logo › L-shape: a
  PNG of the output's size with the video's space, its size and place in
  pixels, and the title safe area, for whoever designs the graphic.

### Changed

- The logo goes off while the picture is in the L-shape, and comes back with
  it.
- The marks file next to a video is now `<name>.marks.json` (trim, logo,
  breaks and L-shape). `<name>.logo.json` is still read, and saving the marks
  replaces it; moving or deleting the video in Storage takes either along.
- The main menu is larger, with an icon for each page, and **Build the
  playlist** stands out in the Player.
- The converter setting and its guide recommend converting uploads: it avoids
  stutters, sound out of sync and playout errors.

### Fixed

- The **ON AIR** badge, and the playout card in Settings, read **STOPPED**
  for a moment at every click in the menu: each page reloaded the channel
  list, which the status took for a new channel. It now starts over only for
  a new login or another channel.
- The logo and SCTE-35 keys could change by themselves: they were saved with
  the rest of the settings, so a page or a browser with an older copy brought
  an old key back, and a key made in the page worked only after a save. Keys
  are now made and saved at once by **New key**, after a confirmation, and
  stay the same until renewed or turned off; saving the settings never
  changes them (`POST /api/playout/config/{id}/keys/{logo|scte35}`).

## [1.4.5] - 2026-10-27

### Fixed

- Settings › General showed "1 unsaved change" without any edit once the
  playout had started or stopped since the page was first opened (the page
  compared its copy against fields the playout changes on its own). Only the
  fields the page edits count now, and a copy without edits follows the
  server.

## [1.4.4] - 2026-10-27

Includes the changes prepared for 1.4.3, which was never published as its own release.

### Security

- A Scheduler item's title was shown as HTML in the **Remove** confirmation,
  so a title with markup ran script in the browser of whoever removed it.
  Confirmation texts are now always plain text.
- Files opened from the storage are sent with `X-Content-Type-Options:
nosniff`, and HTML, SVG and XML files also with `Content-Security-Policy:
sandbox`, so such a file opened on its own runs no script with access to
  NoAr.
- **Test signal** on a live answers every failure with the same message (the
  cause is in the server log) and runs at most two tests at once, so it can
  no longer be used to map the machines the server reaches.
- A playlist import is written to a temporary file of its own name, created
  new and always removed, instead of one named after the uploaded file.
- The request log leaves out the values of `key`, `access`, `uuid` and
  `token` in addresses, such as the SCTE-35 key.

### Fixed

- Operators were signed out when renewing their session was rate limited
  (three renewals a minute per address, shared by a team behind one office
  address). Renewals now have their own limit of 30 a minute, and only a
  rejected session signs anyone out; a busy or restarting server keeps it.
  Signing out from an expired session also no longer fails with an error.
- Player › Build the playlist: **Save** and **Discard** wait while another
  day's playlist loads, so the list on screen cannot be saved over the day
  being opened. Saving a day whose clips were all removed now asks to delete
  its playlist instead of doing nothing.
- Report: clearing a date in **Period** no longer freezes the page, and a
  range that cannot be shown (over 62 days, or reversed) no longer leaves the
  previous range's figures under the new title.
- Settings: a value out of its range is pointed at in its field before
  anything is sent or the playout restarts, and a value the server refuses
  is explained in words instead of the server's raw message.
- Trim: an out point past the end of the video is refused, and the handles
  keep their minimum distance with typed times.
- Storage: a video with only a trim no longer shows the "marks" tag.
- The Scheduler's remove confirmation says **Remove** instead of **Ok**; the
  New folder and Rename dialogs put the cursor in their field; Escape closes
  the user menu.
- A panic in one channel's playout ended the whole server, every channel
  with it, instead of restarting that channel as intended; one such panic
  came from a remote clip whose server sent an unreadable `Last-Modified`
  date. Panics are now caught per channel and logged.
- The playlist generator hung, or stopped the channel, on a folder or
  template source without playable clips (all of length 0, or empty). Such
  clips are skipped and an empty source gives an empty list.
- Converted uploads lost the gap between picture and sound at the start of a
  file: sound that began half a second after the picture began with it.
  The gap is now kept, as silence or the first picture held.
- A conversion's hidden temporary file (`.Name.converting.mp4`) was taken as
  a video by folder playout and the generator, and one left by a server
  stopped midway stayed for good. It is never media now, and leftovers are
  removed at start. A conversion whose original could not be deleted
  afterwards is reported as converted, with a warning, instead of failed.
- A video's marks file (`<name>.logo.json`, with its trim, logo stretches
  and breaks) now follows the video when it is renamed, moved or deleted in
  Storage, and is written in one piece.
- A remote playlist or clip whose server does not answer no longer holds up
  the playlist reader: requests give up after 15 seconds.
- A channel whose playout ends as soon as it starts waits two seconds before
  starting again, instead of restarting in a tight loop.
- The light preview froze after the playout restarted, until the time caught
  up with where the previous run had stopped. It now continues at once.

## [1.4.2] - 2026-10-26

### Fixed

- A playlist saved from the Player, the API or an import could be read back
  empty or half-written for a moment, because the write was not finished
  when saving reported success. Playlists are now written in full under a
  hidden name and then put in place in one rename, so the playout and the
  API always read a whole playlist. This also made the release of 1.4.1
  fail its tests.

## [1.4.1] - 2026-10-26

### Fixed

- On a phone, **Add program** in the Player went past the edge of the
  screen when an episode had a long file name with no spaces, and on short
  screens its **Add** button was out of reach. It now opens as a sheet from
  the bottom whose buttons stay in view while the options scroll, and long
  names wrap. **Generate** works the same way. Fieldsets no longer widen to
  their longest word anywhere in NoAr.

## [1.4.0] - 2026-10-26

### Added

- **Drag the stretches without logo and the SCTE-35 breaks**, like the
  trim: each stretch has two handles on the bar of the video's panel in
  Storage, moved with the mouse, a finger or the arrow keys, and stops at
  the next stretch. The bar shows the trimmed parts, and the other kind of
  marks below it ([docs](docs/commercials.md#a-break-inside-a-video)).

### Changed

- Under each bar of the video's panel and the Player's clip editor, the
  play position, what happens at that point (on air or cut, logo shown or
  off, programme or ad break) and the video's length are labelled, and the
  trim tells how much airs and how much it cuts at each end.
- On a phone: Storage shows each file's whole name with its tags beneath
  (conversion, trimmed, marks); the Report's figures no longer overflow
  their cards; Player playlist tags wrap and the in and out column is left
  for wider screens.

- **Converting uploaded videos is faster**: a video whose picture already
  matches the output only has its sound converted, in seconds, and the
  others are encoded about twice as fast at the same quality
  ([docs](docs/uploads.md)).

### Fixed

- Converted uploads with sound at another sample rate (such as 44.1 kHz)
  lost part of it: the sound fell behind the picture and its last seconds
  were cut.

## [1.3.0] - 2026-10-26

### Added

- **Trim a video once, in Storage**: drag the in and out points on a bar
  while the video plays (or press **In here** / **Out here**, type the
  times, or use the arrow keys), and the video comes in trimmed in every
  playlist, the Scheduler, folder mode and the playlist generator. The
  Player library shows the trimmed length, and the clip editor has the same
  draggable bar, uses the saved trim, or saves a clip's in and out with the
  video ([docs](docs/trim.md)).
- **Convert uploaded videos to the output format**: with the new switch in
  Settings › Playlist and storage, each uploaded video is converted once to
  the output's size and frame rate, high-quality H.264 and 48 kHz AAC, so
  the playout no longer scales it or changes its frame rate on air, and
  damaged files are rewritten from what decodes. Storage shows the progress
  of each one ([docs](docs/uploads.md)).
- **SCTE-35 breaks from encoders and automation**: a key, like the logo
  key, lets vMix, OBS, Companion or a script mark and cancel breaks at
  `/scte35/{id}/break` and `/scte35/{id}/cancel` without logging in.
  Settings › Ad breaks has a tutorial with the addresses ready to copy
  ([docs](docs/ad_breaks.md#from-an-encoder-or-an-automation-with-the-scte-35-key)).

### Changed

- The Player's **Live** view shows only the air: preview, clock and
  controls. The library and the playlist are in **Build the playlist**,
  where they fill the screen: the library and the playlist header (date,
  undo, Save) stay put and only the list scrolls.

### Fixed

- The light Player preview flooded the channel log with h264 errors
  ("non-existing PPS 0 referenced", "Missing reference picture", "no
  frame!"). Joining the stream mid-way, its analysis often ended before the
  first key frame of a 1080p stream, so it failed and started again every
  second. It now waits for a key frame, keeps FFmpeg's messages out of the
  channel log, and when it fails, waits longer each time and logs one
  warning with the reason.
- After a seek, or when joining a live source, the playout waits for a key
  frame instead of decoding pictures whose references never came.

## [1.2.0] - 2026-10-26

### Added

- **Breaks marked inside a video (SCTE-35)**: where a commercial break was
  edited into a programme file, mark where it leaves and comes back, the same
  way as the logo, and NoAr signals it with SCTE-35 wherever the video plays
  ([docs](docs/ad_breaks.md#breaks-marked-inside-a-video)).
- **Light preview**: the Player opens the preview at 640×360, 15 frames per
  second and about 0.6 Mb/s, for slow connections; **High** switches to the
  output itself. Light is encoded only while someone watches it
  ([docs](docs/output.md#light-and-high-quality)).
- The Player says how the output reaches its destination: sending,
  connecting, waiting for a receiver, or no connection and why
  ([docs](docs/output.md#when-the-destination-is-down)).
- Building a playlist in the Player:
    - a **Build the playlist** view that puts the library and the list on top;
    - **undo and redo** (Ctrl+Z, Ctrl+Shift+Z);
    - add after the selected clips, at the start or at a time;
    - **To the start** and **To the end** for the selection;
    - keys for the selection (Delete, Alt+↑/↓, Esc);
    - how much of the day is still to fill or runs over, with **Fill the day**
      at the end of the list.

### Changed

- The + of each file in the Player library is always shown, as it is for
  folders. Adding a file goes after the selected clips when some are
  selected, and the message says where it went.
- Programmes of fewer than 40 clips show all their clips in the grouped
  view.
- The playlist header, with Save, stays in view while scrolling, and
  **Discard** asks first.

### Fixed

- **An SRT, RTMP or UDP destination that is down no longer stops the
  channel.** The connection now runs on its own and reconnects, resuming
  on the next key frame; the playout, the Player preview, the recording and
  the as-run log carry on. Before, a refused connection made the channel
  restart every few seconds with a black Player preview, and an SRT output
  in listener mode held everything until a receiver connected.
- The Player preview no longer stays black without a word: it says when it
  waits for pictures and, when it cannot play, why.
- When the update check cannot reach GitHub, Settings › Updates now says why
  (a DNS failure, a refused connection, a time-out or a certificate problem)
  instead of only "error sending request".
- Storage showed the name of every video and image twice unless the video
  had stretches without logo.

### Documentation

- How to get back into NoAr after forgetting the admin password
  ([docs](docs/docker.md#forgotten-admin-password)).
- New screenshots in the README, and the image tags explained there.

## [1.1.0] - 2026-10-26

### Added

- **Ad breaks (SCTE-35)** on SRT and UDP outputs. Runs of commercials in the
  playlists and scheduled events, and breaks marked by hand, go out as a
  `splice_insert` announced ahead (6 s by default, three copies), on key
  frames at both splice points, with a cancel when a break is called off.
  Settings › Ad breaks turns it on and shows how it works and every break;
  the Player shows the next break and the one on air, and marks one by hand;
  an API does the same ([docs](docs/ad_breaks.md)).
- **Logo animations** in Settings › Logo: the logo can come up and go off
  with a fade or a cut, each set on its own, over a chosen length. Swapping
  the logo for another image now cross-fades instead of switching at once.
  Changes take effect without restarting the playout
  ([docs](docs/logo.md#how-the-logo-comes-up-and-goes-off)).
- **Release automation**: Actions › Prepare release works out the next
  version from these notes, dates them, sets the version everywhere and opens
  the release pull request; merging it tags the version and publishes the
  release and images ([docs](docs/releasing.md#making-a-release)).

### Changed

- The `latest` image now is the newest stable release, never a build of
  `main` or a release candidate; builds of `main` are published as `main`.
  The update notice and **Update now** follow `latest`, so they offer only
  releases, and show the version number
  ([docs](docs/releasing.md#image-tags)).

### Fixed

- A clip usually lasted one frame longer than its duration: its last audio
  frame ran past the end, and the video padded a frame to cover it. Clips now
  last exactly their duration.
- When a playlist failed to load, the Player showed an empty day with no
  error, and saving it could overwrite the real playlist. The error is now
  shown and saving that day is blocked until it loads.
- A server error or lost connection while renewing the login no longer logs
  out every open tab; only a rejected login does.
- The live status could stop updating for good after a failed reconnect; it
  now keeps retrying.
- Saving a playlist that had not changed now says there is nothing new to
  save instead of showing an error.
- Switching dates or channels quickly in the Player could show, and then
  save, the playlist of the day you had left; only the last one asked for
  is shown now.
- A crash inside a channel's playout no longer leaves the channel silently
  off air while still shown as running: the playout restarts like after any
  other failure.
- A damaged spot in a clip or a live feed no longer ends it: damaged data is
  skipped, as the FFmpeg command line does. Only a long run of it ends the
  clip, which then falls back.
- A clip failing partway now falls back only for the rest of its slot.
  Before, the fallback ran for the whole clip length again, and everything
  after it aired late by the part already played.
- When marking where a video keeps the logo off, the preview now shows the
  channel's logo where, as large and as transparent as it goes on air, in
  the shape of the output picture; logos set by full path showed only a
  placeholder. Settings › Logo also sizes its preview exactly, including
  custom scale and position expressions.
- A scheduled clip placed in the hour the clock skips when summer time
  starts (for example 02:30) was silently never aired; it now starts when the
  gap ends.
- A database hiccup at the moment a scheduled event was due made it never
  air; the scheduler now tries again on its next check.
- Changing a scheduled event when part of its bookkeeping failed could leave
  the old version on air; the change now always takes effect.
- A scheduled event cut by a short output failure (for example the RTMP
  server dropping the connection) was closed as failed; it now goes back on
  air where it was when the playout restarts, like after a stop.
- After a stall (a slow file open, a long fallback), the output sent all the
  time it had fallen behind at once, flooding RTMP, SRT and UDP receivers.
  It now catches up only on delays under a second and otherwise carries on
  from the late packet.
- A first start interrupted while setting up the database could make every
  later start fail; setting up is now done in one step.
- Creating channels at the same time as other database writes could fail with
  "database is locked".
- Before an update changes the database, NoAr now keeps a copy of it to go
  back to ([docs](docs/docker.md#if-an-update-goes-wrong)).
- Deleting a channel no longer deletes the record of what it aired: the
  entries move to an archive table ([docs](docs/report.md)).
- Start, stop and restart can no longer run twice at once, and a restart
  finishes even if the browser disconnects halfway. Before, that could leave
  the channel off air and refusing every further command until NoAr was
  restarted.
- Dialogs now take keyboard focus when they open, keep Tab inside, close
  with Escape (the scheduling and live dialogs did not), and hand focus back
  when they close. Escape in a dialog opened from another closes only the
  inner one.

### Security

- A playlist date that was not a date could make saving, deleting, importing
  or generating a playlist write or delete `.json` files outside the playlist
  folder. Playlist paths are now built only from a valid `YYYY-MM-DD` date;
  anything else is refused.
- Login rate limits could be dodged by sending a made-up `X-Forwarded-For`
  header, and without a proxy every client shared one limit. NoAr now
  identifies clients by their connection and trusts `X-Forwarded-For` only
  from a proxy on the same host or a private network
  ([docs](docs/docker.md#behind-a-reverse-proxy)). Signed-in users are now
  exempt from the general request limit, as it was meant to work.

## [1.0.0] - 2026-10-21

The first release under the NoAr name. NoAr started from
[ffplayout](https://github.com/ffplayout/ffplayout) 2.6 by Jonathan Baecker and
keeps its engine, its playlist format and its API; this list covers what NoAr
adds and changes.

### Added

- **Scheduler**: breaks, clips and lives (SRT, RTMP, HLS) placed at a time over
  the running programme. Each one either waits for the clip on air or
  interrupts it and resumes at the exact point. Afterwards the programme
  continues where it stopped or catches up with the clock. Events repeat
  daily or on chosen weekdays, are moved by dragging them on a timeline, and
  are put back on air after a restart.
- **Lives**: created and edited in Storage with a poster and a wide image.
  They are connected ahead of their start, so the switch is immediate and a
  source that takes a single caller is called once. A live that loses its
  signal shows the filler and retries for its retry time.
- **Preloading**: the next clip, the next event and the clip after a live or
  an event are opened before their turn, so the output never waits for a
  file or a network source.
- **Player**: a live preview straight from the output, about a second behind
  the air, and a clock with what is on air and what comes next. The day's
  playlist is grouped by programme, the way the guide names it. Whole
  programme folders are added from the library: the next episode, every
  video in order, or the ones picked.
- **Storage**: programme folders with their guide sheet edited in the
  browser (Kodi `.nfo`), posters, wide images, episode images and credits
  with photos. Series use one sheet for every episode. Uploads keep going in
  the background.
- **Commercials folders**: everything in them plays as a commercial in
  playlists and breaks, out of the guide and without the logo.
- **Logo**:
    - stretches without logo inside a video, for breaks edited into a
      programme;
    - hiding or swapping the logo on air by API, with a key for encoders and a
      tutorial in Settings;
    - a separate logo for the XMLTV guide;
    - a preview in the shape of the output, with reference lines.
- **Report**: what aired, with the actual times and how each item ended, and
  what is still to come. It covers a day or a period, can be narrowed to one
  commercial, programme, live or event, and downloads as a PDF or a
  spreadsheet.
- **XMLTV guide** built from the playlists and the programme sheets, with
  images served by NoAr.
- **Playlist generator** that fills a day from chosen folders or block by
  block, as a draft to review before saving.
- **Log** page that follows the channel live.
- **Settings** in eleven pages with search, unsaved-change markers, one save
  bar, and a restart of the playout or of the whole of NoAr.
- **Updates**: NoAr tells the global admin when a newer image is published,
  and installs it from the settings, at once or at a set time, through an
  updater container.
- **Docker-only install**: one command, then a setup page in the browser.
- Interface in Brazilian Portuguese and English.

### Changed

- The crates are now `noar` and `noar-engine`, and the default paths,
  database name (`noar.db`) and messages say NoAr. Installs that still have an
  `ffplayout.db` or `/usr/share/ffplayout` keep using them.
- New sign-in, first-run setup, Player, Storage, Scheduler, Log and Settings
  screens.

### Removed

- The text overlay ("Legenda") page and feature.
- Installation methods other than Docker.

### Fixed

- A scheduled event that was removed or changed while it waited for the clip
  on air no longer airs in its old form.
- The playout keeps running when a scheduled live starts or stops.

[Unreleased]: https://github.com/LuizStSantos/NoAr/compare/v2.2.1...HEAD
[2.2.1]: https://github.com/LuizStSantos/NoAr/compare/v2.2.0...v2.2.1
[2.2.0]: https://github.com/LuizStSantos/NoAr/compare/v2.1.4...v2.2.0
[2.1.4]: https://github.com/LuizStSantos/NoAr/compare/v2.1.3...v2.1.4
[2.1.3]: https://github.com/LuizStSantos/NoAr/compare/v2.1.2...v2.1.3
[2.1.2]: https://github.com/LuizStSantos/NoAr/compare/v2.1.1...v2.1.2
[2.1.1]: https://github.com/LuizStSantos/NoAr/compare/v2.1.0...v2.1.1
[2.1.0]: https://github.com/LuizStSantos/NoAr/compare/v2.0.0...v2.1.0
[2.0.0]: https://github.com/LuizStSantos/NoAr/compare/v1.11.0...v2.0.0
[1.11.0]: https://github.com/LuizStSantos/NoAr/compare/v1.10.1...v1.11.0
[1.10.1]: https://github.com/LuizStSantos/NoAr/compare/v1.10.0...v1.10.1
[1.10.0]: https://github.com/LuizStSantos/NoAr/compare/v1.9.0...v1.10.0
[1.9.0]: https://github.com/LuizStSantos/NoAr/compare/v1.8.2...v1.9.0
[1.8.2]: https://github.com/LuizStSantos/NoAr/compare/v1.8.1...v1.8.2
[1.8.1]: https://github.com/LuizStSantos/NoAr/compare/v1.8.0...v1.8.1
[1.8.0]: https://github.com/LuizStSantos/NoAr/compare/v1.7.0...v1.8.0
[1.7.0]: https://github.com/LuizStSantos/NoAr/compare/v1.6.0...v1.7.0
[1.6.0]: https://github.com/LuizStSantos/NoAr/compare/v1.5.1...v1.6.0
[1.5.1]: https://github.com/LuizStSantos/NoAr/compare/v1.5.0...v1.5.1
[1.5.0]: https://github.com/LuizStSantos/NoAr/compare/v1.4.5...v1.5.0
[1.4.5]: https://github.com/LuizStSantos/NoAr/compare/v1.4.4...v1.4.5
[1.4.4]: https://github.com/LuizStSantos/NoAr/compare/v1.4.2...v1.4.4
[1.4.2]: https://github.com/LuizStSantos/NoAr/compare/v1.4.1...v1.4.2
[1.4.1]: https://github.com/LuizStSantos/NoAr/compare/v1.4.0...v1.4.1
[1.4.0]: https://github.com/LuizStSantos/NoAr/compare/v1.3.0...v1.4.0
[1.3.0]: https://github.com/LuizStSantos/NoAr/compare/v1.2.0...v1.3.0
[1.2.0]: https://github.com/LuizStSantos/NoAr/compare/v1.1.0...v1.2.0
[1.1.0]: https://github.com/LuizStSantos/NoAr/compare/v1.0.0...v1.1.0
[1.0.0]: https://github.com/LuizStSantos/NoAr/releases/tag/v1.0.0
