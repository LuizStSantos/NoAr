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
- **Logo animations** in Settings › Logo: the logo can come up and go off
  with a fade or a cut, each set on its own, over a chosen length. Swapping
  the logo for another image now cross-fades instead of switching at once.
  Changes take effect without restarting the playout
  ([docs](docs/logo.md#how-the-logo-comes-up-and-goes-off)).
- **Release automation**: Actions › Prepare release works out the next
  version from these notes, dates them, sets the version everywhere and opens
  the release pull request; merging it tags the version and publishes the
  release and images ([docs](docs/releasing.md#making-a-release)).

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

[Unreleased]: https://github.com/LuizStSantos/NoAr/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/LuizStSantos/NoAr/releases/tag/v1.0.0
