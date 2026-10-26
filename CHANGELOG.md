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

- **Ad breaks (SCTE-35)** on SRT and UDP outputs. Runs of commercials in the
  playlists and scheduled events, and breaks marked by hand, go out as a
  `splice_insert` announced ahead (6 s by default, three copies), on key
  frames at both splice points, with a cancel when a break is called off.
  Settings › Ad breaks turns it on and shows how it works and every break;
  the Player shows the next break and the one on air, and marks one by hand;
  an API does the same ([docs](docs/ad_breaks.md)).

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

[Unreleased]: https://github.com/LuizStSantos/NoAr/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/LuizStSantos/NoAr/releases/tag/v1.0.0
