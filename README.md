**NoAr**
================

[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://www.gnu.org/licenses/gpl-3.0)

NoAr is a 24/7 TV playout system: it plays daily playlists or folders without
interruption, lets you schedule breaks and live shows over the running
programme, and publishes an XMLTV guide with artwork for IPTV players.

It is based on [ffplayout](https://github.com/ffplayout/ffplayout) by Jonathan
Baecker and keeps its engine and API, adding:

- **Player**: live preview straight from the output (about a second behind
  the air, for HLS, RTMP or SRT), a clock with what is on air and what comes
  next, transport controls, and the day's playlist grouped by program the way
  the guide names it. Drag files from the library, or add a whole program
  folder: its next episode, every video in order, or the ones you pick.
- **Scheduler**: place a break, a clip or a live show (SRT, RTMP, HLS) at a
  time over whatever is on air, either waiting for the current clip to end or
  interrupting it and resuming at the exact point, without dropping the
  output ([docs](docs/scheduler.md)).
- **Storage**: program folders with their guide sheet edited in the browser,
  poster and wide image, series with one sheet for every episode, and uploads
  that keep going in the background while you work on other pages.
- **Commercials folders**: mark a folder once and everything in it plays as a
  commercial in playlists and scheduled breaks: out of the guide and without
  the logo ([docs](docs/commercials.md)).
- **XMLTV guide** built from the playlists and a Kodi-style `.nfo` file in each
  program folder, with posters and episode images ([docs](docs/xmltv.md)).
- **Playlist generator**: fill a day from the folders you pick, or block by
  block at set times, as a draft you review before saving
  ([docs](docs/playlist_gen.md)).
- **Log** that follows the channel live.
- **Settings**: eleven pages in a side menu, with search, unsaved-change
  markers, one save bar, and a restart of the playout or of the whole of NoAr.
- **Docker-only install**: one command, then a setup page in the browser.

The interface is in Brazilian Portuguese and English; the screenshots show the
Portuguese one.

![Player](docs/images/player.png)

## Screens

**Scheduler**: a break that waits for the clip on air and a live show at
19:30; the programme moves to make room for them.

![Scheduler](docs/images/scheduler.png)

**Storage**: a series folder with one sheet for every episode, its poster and
how each video will show up in the guide.

![Storage](docs/images/storage.png)

**Adding a program to the playlist**: the next episode, the whole folder or
the videos you pick.

![Adding a program folder](docs/images/program-folder.png)

**Settings**: live ingest receivers and the playout restart.

![Settings](docs/images/settings.png)

## Install with Docker

Requirements: Docker with the Compose plugin.

```sh
git clone https://github.com/LuizStSantos/NoAr.git
cd NoAr
docker compose up -d
```

The first run builds the image, which takes a while. When it is up, open
`http://<server>:8787` and complete the setup page: create the admin user and
keep the suggested paths under `/data`.

Everything NoAr stores lives in `./data` next to `docker-compose.yml`:

| Folder | Content |
| --- | --- |
| `data/media` | Media storage: videos, program folders with `.nfo` and artwork |
| `data/playlists` | Daily playlists |
| `data/public` | HLS output |
| `data/db` | Database |
| `data/logs` | Channel logs |

Set your time zone in `docker-compose.yml` (`TZ`, default
`America/Sao_Paulo`) and open the ports of the live ingest listeners you
configure. More in [docs/docker.md](docs/docker.md).

Update:

```sh
git pull
docker compose up -d --build
```

## Features

Inherited from ffplayout:

- dynamic daily playlists with a custom day start, editable while playing
- folder mode, fillers for missing clips, playlist generator with templates
- outputs: stream (RTMP, SRT, UDP) and HLS with adaptive renditions
- RTMP and SRT live ingest that takes over without dropping the output
- logo overlay, WebVTT subtitles, loudness normalization, recording
- import playlists from text or m3u files
- REST API, multiple channels
- mail and HTTP push notifications for log alerts

Each feature has a page in [docs](docs/README.md); for the install you only
need this README and [docs/docker.md](docs/docker.md).

## Development

See [docs/developer.md](docs/developer.md). The Rust crates keep their
original names (`ffplayout`, `ff-engine`); the binary is `noar`.

## License and credits

NoAr is free software under the [GNU GPL v3](LICENSE), like ffplayout, from
which it is derived. The original copyright and history are kept in this
repository. If you distribute NoAr, or a modified version, you must make its
source code available under the same license.
