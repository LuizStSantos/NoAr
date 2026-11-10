**Docker**
================

NoAr ships as a single container. `docker-compose.yml` in the repository root
has everything a normal installation needs.

## Testing a source branch on a disposable machine

For the graphics review branch, use the [test-machine guide](graphics-testing.md)
and `docker-compose.graphics-test.yml`. It builds a local image, starts with a
new test volume and disables automatic replacement by the stable updater.

## First start

```sh
docker compose pull
docker compose up -d --no-build
docker compose logs -f
```

`pull` downloads the published image (`latest`, the newest release; see
[Image tags](releasing.md#image-tags)), so nothing is compiled on the server.
The service also has a `build` section: without `--no-build`, Compose builds
the image from the source when it cannot download it (for example a private
package without `docker login ghcr.io`), which takes a long time. To build
from the source on purpose: `docker compose build && docker compose up -d`.

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
`/usr/local/share/noar/srt-build.txt`. Native GStreamer SRT plugins use this
library. The image includes GStreamer 1.26, codec/container plugins and TLS
support. Its GStreamer core, gst-plugins-base and gst-libav are 1.26.11, built
from the release sources over Debian's 1.26.2 (`docker/gstreamer/build.sh`):
in 1.26.2, `decodebin3` aborted the whole process when a live MPEG-TS source
changed its streams, such as an encoder dropping its video, and `avdec_h264`
aborted it on video whose width is not a multiple of 16 (854×480, for
example). The good, bad and ugly plugin sets stay Debian's packages. Build provenance is recorded in
`/usr/local/share/noar/gstreamer-build.txt`. NoAr itself does not link the FFmpeg libraries; they are present
only through `gstreamer1.0-libav`, GStreamer's plugin for the codecs it does
not have natively (MPEG-2, ProRes and others).

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

NoAr does not host RTMP/SRT input listeners. Publish sources to MediaMTX or
another ingest server, then configure their client URLs as scheduled lives in
NoAr. These outgoing connections need no published input port on NoAr.

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

## NVIDIA encoding (NVENC)

With an NVIDIA card, NoAr can encode its output on the card instead of the
processor: choose **H.264 · NVIDIA** (`h264_nvenc`) or **H.265 · NVIDIA**
(`hevc_nvenc`) as the video codec in Settings › Output. The image already has
GStreamer's NVIDIA plugin (`nvh264enc`, `nvh265enc`, from
`gstreamer1.0-plugins-bad`); the card's driver libraries come from the host
when the container is given the card. Decoding the clips, the logo and the
L-shape stay on the processor.

1. On the server, install the NVIDIA driver (`nvidia-smi` must work) and the
   [NVIDIA Container Toolkit](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/latest/install-guide.html),
   then let Docker use it:

   ```sh
   sudo nvidia-ctk runtime configure --runtime=docker
   sudo systemctl restart docker
   ```

2. In `docker-compose.yml`, uncomment the NVIDIA lines of the `noar` service
   (the `NVIDIA_DRIVER_CAPABILITIES` variable and the `deploy` block). The
   `video` capability gives the container the encoder library.

3. Recreate the container and check that GStreamer sees the card:

   ```sh
   docker compose up -d --no-build
   docker exec noar nvidia-smi
   docker exec noar gst-inspect-1.0 nvh264enc | head -5
   ```

   The NVIDIA codecs appear in Settings › Output only when the last command
   finds the element.

GeForce cards limit how many encodes run at once (8 on current drivers);
professional cards do not. Run a new encoder on a test server for a while
before airing with it. The project has not been tested on NVIDIA hardware
yet; report what you find.

## Prebuilt image

Each release publishes `ghcr.io/luizstsantos/noar:X.Y.Z`, `:X.Y` and
`:latest` through GitHub Actions, and every push to `main` publishes `:main`
(see [Image tags](releasing.md#image-tags)). While the package is private on
GitHub, servers need `docker login ghcr.io` first; make it public in the
package settings to skip that. `docker compose pull && docker compose up -d
--no-build` updates to the newest release without building.

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

#### Restoring before graphics conversion (00029)

Migrations `00029` and `00030` are normal sqlx migrations. Startup then converts every
channel's logo, L-shape and quick scenes to templates in one transaction.
It leaves the old graphics columns intact, but the new application no longer
reads them after conversion. Logo/L-shape/ticker editing now opens Composer;
legacy configuration writes update the same layers and bindings. Quick scenes
are no longer used after 3.0. Migration 00030 also upgrades existing
00029 templates once, preserving edited layers on subsequent starts. Clock
support is not part of this conversion.

Keeping old columns does **not** make an upgraded database usable by an old
binary: sqlx rejects migrations unknown to that binary. Released 2.2.1 knows
through `00027`, so even a database with `00028` already needs restoration.
Rollback means restoring the **pre-update database and previous image
together**. Changes made after the backup, including edited templates, are
not included in the restored database.

Before updating, record the current image digest and keep a database backup.
After startup, confirm the pre-update `.bak` file exists beside the database
and record its exact name and pre-update migration version. Verify the file
itself rather than relying only on console messages. Automatic backups are best effort,
keep only three copies, and run only when sqlx migrations are pending. A
failed conversion can leave the schema migrations applied; a retry does not create another
pre-update copy. Keep the first successful copy separately.

For the default Compose bind mount (`./data:/data`), restore as follows,
substituting the verified backup filename and recorded image digest:

```sh
docker compose stop noar
# Preserve the stopped upgraded database and any SQLite sidecars together.
mkdir -p rollback-saved
cp -a data/db/. rollback-saved/
cp data/db/noar.db.pre-update-<date>-<time>-v27.bak data/db/noar.db
rm -f data/db/noar.db-wal data/db/noar.db-shm
```

Run these commands with the permissions needed by the installation. Preserve
the database's owner and mode. Set `services.noar.image` in a Compose override
to the recorded digest, for example
`ghcr.io/luizstsantos/noar@sha256:<previous-image-digest>`, then run
`docker compose up -d --no-build noar`. Check `/health`, login, configuration
and the real output before resuming normal operation. Never restore a file
while either version is using that database. Custom `DB` paths and named
volumes need the equivalent steps at their actual database location.

The disposable 2.2.1 upgrade/restoration test and HLS captures are recorded in
the [graphics handoff](graphics-handoff/README.md). This procedure does not
choose a release version or authorize publishing an image; see the
[release compatibility policy](releasing.md#what-each-number-means).

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

### Uploaded graphics fonts

Composer's **Upload font** stores validated TTF/OTF bytes with channel metadata
in SQLite (migration 00032), and registers them again when the backend starts.
Back up/restore the database to retain those assets. Do not install them into a
running container or expect uploaded fonts to appear as Storage files. Duplicate
filenames create independent immutable assets. A channel accepts up to 32 fonts,
10 MiB each; deleting templates preserves them, and this stage does not provide
a font deletion command. Use fonts whose license permits your intended usage.
