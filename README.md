**NoAr**
================

[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://www.gnu.org/licenses/gpl-3.0)

NoAr is a 24/7 TV playout system: it plays daily playlists or folders without
interruption, lets you schedule breaks and live shows over the running
programme, and publishes an XMLTV guide with artwork for IPTV players.

**Website:** <https://luizstsantos.github.io/NoAr/>

Everything is run from the browser: the Player for what is on air, the
Playlist page to build the days, the Scheduler for timed programmes and graphics,
Graphics to design and operate reusable templates, Storage for media and guide
artwork, and the Report for what aired.

## Features

**On air**

- **Player**: live preview straight from the output (about a second behind
  the air, for HLS, RTMP or SRT), in a **light** quality for slow connections
  or the output's own; a clock with what is on air and what comes next, the
  next break and scheduled event, the sync state and the audio levels; and
  transport controls that lock against a stray click.
- **Graphics status** under the preview: a read-only list of active channel
  models and temporary graphics, with **Open Graphics** to operate them.
  Graphics › On air has the model preview, program monitor and channel controls.
- **Output that keeps going**: HLS (with adaptive renditions) and SRT, RTMP or
  UDP streams. A destination that is down never stops the channel: NoAr
  reconnects by itself, the preview keeps playing, and the Player says whether
  the output is sending, waiting for a receiver or cannot connect, and why
  ([docs](docs/output.md#when-the-destination-is-down)).
- **Assembled programs**: a program delivered in blocks becomes one file with
  its commercial breaks, dragged into playlists or scheduled as one item, and
  edited once for every airing ([docs](docs/programs.md)).
- **NDI output**: the uncompressed program as an NDI source on the local
  network, once the NDI Runtime is installed on the server
  ([docs](docs/ndi.md)).
- **Video cards**: the program on a Blackmagic DeckLink card (SDI/HDMI), and
  DeckLink or V4L2 capture cards as live inputs
  ([docs](docs/video-cards.md)).
- **Scheduled live sources**: NoAr pulls SRT, RTMP, RTSP, HLS or UDP streams
  from an external ingest server such as MediaMTX and switches between files
  and live sources through GStreamer ([docs](docs/live_ingest.md)).
- **Recording** of the output, loudness normalization and an audio meter.

**Playlists**

- **Playlist** page (its own item in the menu): the library and the day's
  playlist, grouped by program the way the guide names it, filling the
  screen with only the list scrolling. Add files or a whole program folder (its
  next episode, every video in order, or the ones you pick) at the end, after
  the selection or at a time; move, duplicate, mark as a break, with undo,
  keys and how much of the day is still to fill.
- **Per-item edits**: an item's in and out points, title, audio, filter and
  its own **marks** (trim, ad breaks, graphics cues) that apply to that item
  only and leave the video in Storage as it is.
- **Daily playlists** with a custom day start, editable while playing, or
  **folder mode**, which plays the storage folder without a playlist
  ([docs](docs/folder_mode.md)); fillers cover missing clips.
- **Playlist generator**: fill a day from the folders you pick, or block by
  block at set times, as a draft you review before saving
  ([docs](docs/playlist_gen.md)). Playlists can also be imported from text
  or m3u files.
- **Videos from a URL** next to local files ([docs](docs/remote_source.md)).

**Scheduler**

- Place a break, a clip or a live show at a time over
  whatever is on air, either waiting for the current clip to end or
  interrupting it without dropping the output. In playlist mode, continue the
  interrupted programme or return to the clock; once, every day or on chosen weekdays ([docs](docs/scheduler.md)).
- **Scheduled graphics**: choose a Composer template, fill its fields or select
  a data-source record, and set its start and end. Temporary graphics leave
  automatically; scheduled visibility changes restore the earlier state
  ([docs](docs/scheduler.md#scheduled-graphics)).

**Graphics**

- **Templates and Composer**: design text, images and animated GIFs, rectangles,
  rounded rectangles, ellipses, lines, live clocks and a video corner as layers.
  Drag, resize in percent or pixels, keep proportions, set opacity and animations,
  and use the timeline, grid and optional safety margins to arrange the design.
  Layers can be hidden, deleted and restored with undo. Open **Edit** or
  **+ New template** from Templates or On air.
- **Your names and groups**: organize templates by channel, programme, sponsor
  or any group you create. Logo, clock and text compositions can stay **Always
  on air**; a template can be assigned as the channel's logo, ticker or
  L-shape. New installations contain only one removable Logo example.
- **Fonts and fields**: upload TTF/OTF fonts from Composer for text and clock
  layers. Add `{fields}` for operators to fill before taking a template on air;
  preview and output use the uploaded font assets.
- **Data sources**: CSV files and public Sheets CSV links, RSS/Atom feeds and
  HTTP JSON supply fields such as prices, scores and headlines. Choose a record,
  refresh it periodically and retain the last good values when a provider fails
  ([docs](docs/graphics-data-sources.md)). Values update in place. Text layers
  can scroll continuously in either direction, with speed and repeat spacing
  set in Composer; this works with manual text and mapped fields.
- The logo and L-shape are edited as layers in Composer. The logo fades or
  cuts, cross-fades when swapped and can go anywhere on screen.
- **L-shape**: the picture shrinks into a corner over a full-screen graphic,
  with artwork layers and an editable video window.
- **Two commands for every graphic**: put a template on air (for a time, or
  until taken off) and take it off, from On air, from cues marked inside a
  video, from the Scheduler, or from an encoder, vMix, OBS or a Stream Deck
  with the graphics key ([docs](docs/logo.md)). With the key, encoders can also
  list what is on air and take everything off except the templates you name.
- **On air**: a template preview, a real program monitor and put on air /
  take off, with the templates, the graphics on air and the always-on ones in
  three tabs. Duplicate a template to start a new one from it.
- **Animated GIF and WebP** images play on air; a very long animation plays
  one frame in two (or four, …) instead of being refused.
- **WebVTT subtitles** in the HLS output ([docs](docs/closed_captions.md)).

**Storage**

- Program folders with their guide sheet edited in the browser, poster and
  wide image, and series with one sheet for every episode.
- Uploads that keep going in the background while you work on other pages.
  Uploaded videos can be **converted to the output format** once, so the
  playout does less on air ([docs](docs/uploads.md)), and **trimmed** once, by
  dragging the in and out points while the video plays, so they come in
  trimmed everywhere ([docs](docs/trim.md)).
- **Downloads**: save a stored video, image or sidecar file to your computer
  from its row, image card or viewer. WebVTT subtitle files can be uploaded
  beside their videos ([docs](docs/storage.md)).
- **Marks in a video**: its trim, ad breaks and graphics cues on one
  timeline, played back as they air, with each template drawn over the video
  with its entry and exit animations.
- **Commercials folders**: mark a folder once and everything in it plays as a
  commercial: out of the guide and without the logo
  ([docs](docs/commercials.md)).

**Ad breaks and the guide**

- **SCTE-35**: runs of commercials, breaks marked by hand, by API from an
  encoder, or inside a programme video are signalled in the SRT or UDP output
  for FAST platforms and ad servers to replace, frame-accurately and announced
  ahead; HLS tags are experimental ([docs](docs/ad_breaks.md),
  [study](docs/scte35.md)).
- **XMLTV guide** built from the playlists and a Kodi-style `.nfo` file in
  each program folder, with posters, episode images, credits with photos and a
  channel logo of its own ([docs](docs/xmltv.md)).
- **Report**: what aired, with the actual times and how each item ended, and
  what is still to come, for a day or a period; narrowed to one commercial,
  program, live or event, and downloaded as a PDF or a spreadsheet
  ([docs](docs/report.md)).

**Operation**

- **Multiple channels**, users with roles, and a **REST API**
  ([docs](docs/api.md)). **Settings › External control** gathers the keys and
  ready-to-copy commands for lives, graphics and SCTE-35 breaks.
- **Alerts** by mail and HTTP push (ntfy) for log errors
  ([docs](docs/notifications.md)); a **log** that follows the channel live, in
  **Settings › Log**.
- **External tasks**: a script run when a clip starts
  ([docs](docs/external_tasks.md)).
- **Settings**: a side menu of pages, with search, unsaved-change markers, one
  save bar, and a restart of the playout or of the whole of NoAr; updates from
  the browser.
- **Docker-only install**: one command, then a setup page in the browser.

The interface is in English and Brazilian Portuguese. All screenshots below
show **dark mode**, in English, at **1440×1000 pixels**, on the synthetic
**Example TV** demo channel.
Its programmes, artwork, templates and market records are examples.
The [screenshot guide](qa/screenshots/README.md) rebuilds and verifies the gallery.

![Player](docs/images/player.png)

## Screens

**Scheduler**: the day's programme with each clip at its real length, a
scheduled break and a live on the events lane, and a Composer template scheduled
on its own graphics lane. Below it, one list of the day's events, filtered by
all, materials, lives or graphics.

![Scheduler](docs/images/scheduler.png)

**Storage**: a series folder with one sheet for every episode, its poster and
how each video will show up in the guide.

![Storage](docs/images/storage.png)

**Playlist**: its own page in the menu, filling the screen; the library and
the list header stay put while the list scrolls. Two clips selected, with
what can be done to them, and how far the day runs over.

![Building a playlist](docs/images/builder.png)

**Assembled program**: the blocks of a programme and its commercial break in
one `.program` file, built from the storage pane by dragging or with +; the
playlist and the Scheduler play it as one programme.

![Editing an assembled program](docs/images/program-editor.png)

**Adding a program to the playlist**: the next episode, the whole folder or
the videos you pick, at the end, after the selection or at a time.

![Adding a program folder](docs/images/program-folder.png)

**Marks in a video**: the trim, the SCTE-35 breaks and the graphics cues on
one timeline. Here, the graphics cues: a market update on air for 20 seconds and
a presenter put on and taken off. The player previews each template over the
video, with its entry and exit animations.

![Marks in a video](docs/images/marks.png)

**Graphics › Templates**: custom groups, thumbnails on a neutral transparency
checkerboard and controls appropriate to each model's assignment. Edit opens
Composer and Duplicate copies a template to change it; scroll or filter groups
to see the other templates.

![Graphics templates in dark mode](docs/images/graphics.png)

**Composer**: image, shape and text layers, named fields, resize handles,
percent/pixel geometry, font upload, animation timing and editor safety margins.
Layers are reordered by dragging and renamed in the list; the layer's settings
are split into Position, Content, Style, Animation and Template tabs, with a
colour picker for every colour. The shared checkerboard gains an alignment grid
in the editor.

![Composer in dark mode](docs/images/composer.png)

**Graphics › On air**: fill the presenter fields on the preview, compare them
with the real program monitor and take graphics in/out. The right column has
three tabs: Templates, with search and a group filter; On air, the temporary
graphics with their time left; and Always on air, the permanent ones.
Preview uses the same checkerboard as Templates; Program shows the actual video.

![Graphics On air in dark mode](docs/images/onair.png)

**Graphics › Data sources**: a CSV example with two market records, ready to
map to template fields. RSS/Atom and HTTP JSON sources use the same page.

![Graphics data sources in dark mode](docs/images/data-sources.png)

**Report**: a day with the actual times of what aired and how each item ended; on
the current day, also what is still to come
from the playlist and the Scheduler, with totals for commercials and lives.

![Report](docs/images/report.png)

**Settings › Output**: HLS, a stream (RTMP, SRT or UDP), NDI or an SDI/HDMI
card as the destination, with the video encoder and the output audio sample
rate.

![Settings](docs/images/settings.png)

**Settings › External control**: one page for everything controlled from
outside without a login, with a tab each for lives, graphics and SCTE-35
breaks. Here the graphics tab: its key, a template picked by search, and the
commands to put it on air and take it off, ready to copy.

![External control](docs/images/remote.png)

**Sign in**: what NoAr does, next to the sign-in form.

![Sign in](docs/images/login.png)

## Install with Docker

Requirements: Docker with the Compose plugin.

```sh
git clone https://github.com/LuizStSantos/NoAr.git
cd NoAr
docker compose pull
docker compose up -d --no-build
```

This downloads the published image (`ghcr.io/luizstsantos/noar:latest`, the
newest release) instead of compiling NoAr on the server. Without
`--no-build`, Compose builds the image from the source when it cannot
download it, which takes a long time; to build on purpose, run
`docker compose build` first. When it is up, open `http://<server>:8787` and
complete the setup page: create the admin user and keep the suggested paths
under `/data`.

Everything NoAr stores lives in `./data` next to `docker-compose.yml`:

| Folder           | Content                                                        |
| ---------------- | -------------------------------------------------------------- |
| `data/media`     | Media storage: videos, program folders with `.nfo` and artwork |
| `data/playlists` | Daily playlists                                                |
| `data/public`    | HLS output                                                     |
| `data/db`        | Database                                                       |
| `data/logs`      | Channel logs                                                   |

Set your time zone in `docker-compose.yml` (`TZ`, default
`America/Sao_Paulo`). Live source URLs connect to an external server and need no
input ports published by NoAr. More in [docs/docker.md](docs/docker.md).

Update from **Settings › Updates** (with the `noar-updater` service on, see
[docs/docker.md](docs/docker.md#updates)), or on the server:

```sh
docker compose pull && docker compose up -d --no-build
```

Forgot the admin password? Set a new one from the server with
`docker exec -it noar noar --user-set`; see
[docs/docker.md](docs/docker.md#forgotten-admin-password).

## Versions

NoAr follows [Semantic Versioning](https://semver.org); every release is
listed in [CHANGELOG.md](CHANGELOG.md). The images on
`ghcr.io/luizstsantos/noar` are tagged:

| Tag            | What it is                                           |
| -------------- | ---------------------------------------------------- |
| `latest`       | The newest stable release                            |
| `3.0`, `3.0.0` | A release line (gets its fixes) or one exact release |
| `main`         | The development build, not for air                   |
| `sha-…`        | One commit, for tests                                |

To stay on a release line, pin it in `docker-compose.yml`, for example
`ghcr.io/luizstsantos/noar:3.0`. Releases are prepared and published by the
**Prepare release** and **Tag release** workflows; see
[docs/releasing.md](docs/releasing.md).

## Documentation

Each feature has a page in [docs](docs/README.md); for the install you only
need this README and [docs/docker.md](docs/docker.md).

## Development

See [docs/developer.md](docs/developer.md) for builds and
[Architecture and maintenance](docs/maintenance.md) for module ownership and
regression checks. The workspace has two crates:
`noar`, the server and web interface (the binary is `noar`), and
`noar-engine`, which decodes, composites and encodes.

## License and credits

NoAr is free software under the [GNU GPL v3](LICENSE). It is derived from
ffplayout, by Jonathan Baecker and its contributors, also under the GPL v3;
the original copyright and history are kept in this repository. If you
distribute NoAr, or a modified version, you must make its source code
available under the same license.
