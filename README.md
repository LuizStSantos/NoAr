**NoAr**
================

[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://www.gnu.org/licenses/gpl-3.0)

NoAr is a 24/7 TV playout system: it plays daily playlists or folders without
interruption, lets you schedule breaks and live shows over the running
programme, and publishes an XMLTV guide with artwork for IPTV players.

It is based on [ffplayout](https://github.com/ffplayout/ffplayout) by Jonathan
Baecker and keeps its engine, web interface and API, adding:

- **XMLTV guide** built from the playlists and a Kodi-style `.nfo` file in each
  program folder, with posters and episode images ([docs](docs/xmltv.md)).
- **Scheduler**: place a break, a clip or a live show (SRT, RTMP, HLS) at a
  time over whatever is on air, either waiting for the current clip to end or
  interrupting it and resuming at the exact point, without dropping the
  output ([docs](docs/scheduler.md)).
- **New Player**: live preview straight from the output (about a second
  behind the air, for HLS, RTMP or SRT), a clock with what is on air and what
  comes next, transport controls, and the day's playlist grouped by program
  the way the guide names it, with drag and drop from the library and clip
  editing in place.
- **New settings**: eleven pages in a side menu, with search, unsaved-change
  markers and one save bar.
- **Docker-only install**: one command, then a setup page in the browser.

![Player](docs/images/player.png)

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
- web interface and REST API, multiple channels
- mail and HTTP push notifications for log alerts

![Settings](docs/images/settings.png)

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
