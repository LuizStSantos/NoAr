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

## SRT continuity

The image includes GnuTLS SRT 1.5.5 with corrected session lifecycle so
closing a scheduled live input does not destroy the channel's existing SRT
output connection.
The loader selects `/usr/local/lib/libsrt-gnutls.so.1.5`; the distribution
package version can still report 1.5.4. Build provenance is recorded in
`/usr/local/share/noar/srt-build.txt`. FFmpeg's ABI and channel configuration
are unchanged. See [developer.md](developer.md#optional-srt-logging-integration)
for the regression check.

Before deploying a candidate image, test repeated playlist/live/playlist
transitions with synthetic sources and disposable MediaMTX and VLC instances.
Confirm that the publisher and reader connections remain active and frames
continue arriving. A successful library test is not a complete playout test.
Keep the previous image digest available for rollback; do not update an
on-air container's libraries in place.

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

## Updates

NoAr checks every six hours whether a newer image was published: it asks the
GitHub API for the newest stable release whose image was built (the `latest`
image) and compares its commit with the one NoAr was built from (the image
build passes it in as the `NOAR_COMMIT` build argument). A newer image lights a dot on Settings and a
notice on the home page for the global administrator; **Settings › Updates**
lists what changed and shows the command that updates the server. The check
can be turned off on that page, and **Check now** runs it at once.

A fork that publishes its own image sets `NOAR_UPDATE_REPOSITORY=owner/repo`
on the container so the check looks there. An image built locally has no
commit to compare: the page then only shows the newest published one.

### Updating from the settings

The `noar-updater` service of `docker-compose.yml` installs a new version
when the page asks: it runs [Watchtower](https://github.com/nicholas-fedor/watchtower)
with only its HTTP API on, so it never updates anything by itself. It is the
only container that talks to Docker; NoAr only calls its API with a token.
Turn it on once, in the folder of `docker-compose.yml`:

```sh
echo "COMPOSE_PROFILES=updater" >> .env
echo "NOAR_UPDATER_TOKEN=$(openssl rand -hex 24)" >> .env
docker compose up -d
```

Settings › Updates then shows **Updater connected**, and offers:

- **Update now**: the updater pulls the newest image and replaces the NoAr
  container. The channels go off air for a few seconds; a Scheduler event on
  air goes back on air where it was. The page waits and reloads by itself.
- **Schedule**: the same at the next given time of the server clock. If NoAr
  is off at that time, the scheduled update is dropped rather than run late.
- **Update on its own**: every day at the given time, when a new version is
  out.

### If an update goes wrong

Before a new version changes the database, NoAr copies it next to itself, as
`noar.db.pre-update-<date>-<time>-v<N>.bak` in the database folder
(`/data/db` in the container); the three newest copies are kept. To go back:
stop the container, run the previous image tag again, and put the copy in
place of `noar.db` (removing `noar.db-wal` and `noar.db-shm` beside it). If
the copy cannot be made, for example on a full disk, the log says so and the
update goes on.

The updater only replaces containers labelled
`com.centurylinklabs.watchtower.enable: "true"`, which the compose file puts
on NoAr. A private image needs the `docker login ghcr.io` credentials mounted
into the updater (the line is in the compose file, commented out).

## Forgotten admin password

The password can be set again from the server, without losing anything and
without stopping the channels:

```sh
docker exec -it noar noar --user-set
```

It asks for the **username** (the existing one, to change its password), the
new **password**, the **email** and whether to keep **two-factor
authentication**. Answer *no* there unless the server's mail (SMTP) works:
with it on, signing in waits for a code sent by mail. Sign in with the new
password right away. Without the prompts:

```sh
docker exec -it noar noar --user-set -u admin -m admin@example.org -p 'new password' --two-factor false
```

(this one leaves the password in the shell history). A username that does
not exist yet creates another global administrator, which can then change or
remove the old account in **Settings › Users**. Outside Docker, run
`noar --user-set --db /path/to/noar.db` on the server.

If signing in then fails with *"Could not sign in right now"* (*"Não foi
possível entrar agora"*), wait a minute and try once: sign-in allows three
attempts a minute from one address. A wrong password says so instead. Keeping
a session open (the browser renewing its access every few minutes) allows 30
renewals a minute from one address, so a team behind one office address stays
signed in; a renewal the server cannot answer right away does not sign anyone
out.

## Behind a reverse proxy

Forward `X-Forwarded-Proto` and `X-Forwarded-Host` so XMLTV image links use
the public address, and `X-Forwarded-For` so NoAr sees each client's address
for login rate limits and file links. NoAr believes `X-Forwarded-For` only
from a proxy on the same host or on a private network (loopback, 10/8,
172.16/12, 192.168/16, link-local, fc00::/7); a client connecting directly is identified
by its own address. A minimal nginx location:

```nginx
location / {
    proxy_pass http://127.0.0.1:8787;
    proxy_set_header Host $host;
    proxy_set_header X-Forwarded-Proto $scheme;
    proxy_set_header X-Forwarded-Host $host;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    proxy_http_version 1.1;
    proxy_set_header Connection "";
    proxy_buffering off;
}
```
