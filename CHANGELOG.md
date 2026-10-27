# Changelog

Every release of NoAr, newest first. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html); see
[docs/releasing.md](docs/releasing.md) for what each part of the number means
here.

Work merged since the last release goes under **Unreleased** until the next
one is tagged.

## [Unreleased]

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

### Fixed

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

[Unreleased]: https://github.com/LuizStSantos/NoAr/compare/v1.4.5...HEAD
[1.4.5]: https://github.com/LuizStSantos/NoAr/compare/v1.4.4...v1.4.5
[1.4.4]: https://github.com/LuizStSantos/NoAr/compare/v1.4.2...v1.4.4
[1.4.2]: https://github.com/LuizStSantos/NoAr/compare/v1.4.1...v1.4.2
[1.4.1]: https://github.com/LuizStSantos/NoAr/compare/v1.4.0...v1.4.1
[1.4.0]: https://github.com/LuizStSantos/NoAr/compare/v1.3.0...v1.4.0
[1.3.0]: https://github.com/LuizStSantos/NoAr/compare/v1.2.0...v1.3.0
[1.2.0]: https://github.com/LuizStSantos/NoAr/compare/v1.1.0...v1.2.0
[1.1.0]: https://github.com/LuizStSantos/NoAr/compare/v1.0.0...v1.1.0
[1.0.0]: https://github.com/LuizStSantos/NoAr/releases/tag/v1.0.0
