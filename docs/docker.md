**Docker**
================

NoAr ships as a single container. `docker-compose.yml` in the repository root
has everything a normal installation needs.

## First start

```sh
docker compose up -d
docker compose logs -f
```

Open `http://<server>:8787`. The setup page asks for the admin user and shows
the data paths, already pointing at the `/data` volume:

| Setting | Path in the container |
| --- | --- |
| Logging | `/data/logs` |
| Playlists | `/data/playlists` |
| Public (HLS) | `/data/public` |
| Storage | `/data/media` |

Keep them unless you mount other volumes. With *shared storage* enabled, each
channel gets its own subfolder (`/data/media/1`, `/data/media/2`, ...).

## Data

The container keeps everything under `/data`, mounted from `./data`. Back up
that folder to back up NoAr. To use media that already lives elsewhere on the
server, mount it inside the storage, for example:

```yaml
    volumes:
      - ./data:/data
      - /srv/videos:/data/media/1/videos:ro
```

## Ports

| Port | Use |
| --- | --- |
| `8787/tcp` | Web interface, API, HLS (`/public/...`) and XMLTV (`/xmltv/...`) |
| `1936/tcp` | Example RTMP ingest listener |
| `9000/udp` | Example SRT ingest listener |

Ingest listeners are created in the channel settings. Publish each listener's
port in `docker-compose.yml`, using the same number inside and outside. Lives
that NoAr pulls itself (scheduled SRT caller, RTMP or HLS sources) need no
published port.

## Restarting

*Settings › Server and e-mail › Restart NoAr* (global administrators) stops
every channel and starts NoAr again inside the same process, so the container
keeps running and no restart policy is involved. The channels that were on
air start again by themselves, and a scheduled event on air resumes where it
was. To restart only one channel's playout, use the playout card at the top
of the settings menu.

## Time zone

Set `TZ` in `docker-compose.yml` before the first start. Playlists, schedules
and the guide follow the channel time zone, and new channels take it from
`TZ`. Channels that already exist keep theirs; change it in the channel
settings.

## Environment

The image sets these variables:

| Variable | Default | Purpose |
| --- | --- | --- |
| `DB` | `/data/db/noar.db` | Database file |
| `NOAR_SETUP_STORAGE` | `/data/media` | Storage path suggested on the setup page |
| `NOAR_SETUP_PLAYLISTS` | `/data/playlists` | Playlist path suggested on the setup page |
| `NOAR_SETUP_PUBLIC` | `/data/public` | HLS path suggested on the setup page |
| `NOAR_SETUP_LOGS` | `/data/logs` | Log path suggested on the setup page |
| `TZ` | `UTC` (`America/Sao_Paulo` in the compose file) | Time zone |

After the setup, each channel uses the paths saved for it; change them in
the channel settings. Do not set `STORAGE`, `PLAYLISTS`, `PUBLIC` or `LOGS`:
they override the paths of every channel.

## Prebuilt image

Every push to `main` publishes `ghcr.io/luizstsantos/noar:latest` through
GitHub Actions. While the package is private on GitHub, servers need
`docker login ghcr.io` first; make it public in the package settings to skip
that. With the image published, `docker compose pull && docker compose up -d`
updates without building.

## Behind a reverse proxy

Forward `X-Forwarded-Proto` and `X-Forwarded-Host` so XMLTV image links use
the public address. A minimal nginx location:

```nginx
location / {
    proxy_pass http://127.0.0.1:8787;
    proxy_set_header Host $host;
    proxy_set_header X-Forwarded-Proto $scheme;
    proxy_set_header X-Forwarded-Host $host;
    proxy_http_version 1.1;
    proxy_set_header Connection "";
    proxy_buffering off;
}
```
