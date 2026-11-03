# NDI output and input

NoAr can publish the channel's program as an NDI source: the composited
picture (logo, L-shape, text) and the processed program audio (volume,
loudness), uncompressed, to NDI receivers on the local network such as NDI
Studio Monitor, vMix, OBS or a hardware converter.

It can also take an NDI source as a **live** in the Scheduler, like an SRT or
RTMP live.

Video cards (Blackmagic DeckLink SDI/HDMI, V4L2 capture) are described in
[video-cards.md](video-cards.md).

## How it is packaged

- The GStreamer NDI elements (`gst-plugin-ndi` from gst-plugins-rs) are
  compiled into NoAr. Nothing extra is needed in the image.
- The **NDI Runtime** (`libndi.so.6`) is proprietary and is **not** shipped
  with NoAr. Install it on the server and make it visible to NoAr. Without it,
  NoAr runs normally and the NDI destination in *Settings › Output* is
  disabled with an explanation.
- NoAr checks for the runtime when it starts. After installing it, restart
  NoAr.

## Turning NDI on (script)

From the NoAr folder (the one with `docker-compose.yml`) on the server:

```sh
sudo sh docker/ndi/enable-ndi.sh
docker compose up -d
```

The script:

1. downloads the NDI SDK for Linux and runs its installer, which shows the
   NDI license: read it and accept it to continue;
2. copies the NDI Runtime (`libndi.so.6`) for the server's architecture to
   `/opt/ndi`, with the license files;
3. installs and starts `avahi-daemon` (mDNS discovery) when it is missing;
4. writes `docker-compose.override.yml`, which Docker Compose loads by
   itself: NoAr uses the server's network (needed for NDI discovery; the web
   interface stays on port 8787), reads the runtime from `/opt/ndi` and
   reaches the server's system D-Bus (`/run/dbus`): the NDI library announces
   its sources through `avahi-daemon` over D-Bus, and without it receivers do
   not list NoAr.

Then allow 5353/UDP (mDNS) and 5959-5999/TCP (NDI) in the firewall. In
*Settings › Output*, NDI is now available. To check that NoAr can load the
runtime: `docker compose exec noar ldd /opt/ndi/libndi.so.6 | grep 'not found'`
(no output means it can).

Options: `--sdk FILE` uses an installer already downloaded,
`--runtime-dir DIR` another folder, `--no-compose` installs the runtime only.
The script never overwrites a `docker-compose.override.yml` of your own; it
stops and asks you to add the lines below to it. Docker Compose 2.24 or newer
is needed. To turn NDI off: delete `docker-compose.override.yml` and run
`docker compose up -d`.

To check that the source is announced, choose NDI in *Settings › Output*, then
on the server run `avahi-browse -rt _ndi._tcp` (package `avahi-utils`): it
lists `SERVER (name)`.

For the isolated test project, run it from the test checkout with `--test`.
It writes `qa/gstreamer/compose.ndi.yaml`, which keeps the test interface on
port 18787 (it does not take the installation's 8787) and leaves out the
updater the test project does not have. Pass that file with the other `-f`
files:

```sh
cd ~/NoAr-gstreamer-test
sudo sh docker/ndi/enable-ndi.sh --test
docker compose --env-file .env -f qa/gstreamer/compose.integrated.yaml \
  -f qa/gstreamer/compose.ndi.yaml up -d --no-build
```

When the runtime is already in `/opt/ndi` (from an earlier run, or the
installation), `sh docker/ndi/enable-ndi.sh --test --compose-only` only writes
the file, without downloading anything or needing sudo.

With host networking the `mediamtx` service name does not resolve inside
NoAr; use `127.0.0.1` with the published test ports (for example
`srt://127.0.0.1:19890?streamid=read:input`). Without NDI, start the test as
usual, without `-f qa/gstreamer/compose.ndi.yaml`.

## Docker settings by hand

The lines the script writes, to add to your own override file:

```yaml
services:
  noar:
    network_mode: host
    ports: !reset []
    environment:
      NDI_RUNTIME_DIR_V6: /opt/ndi
      NOAR_UPDATER_URL: http://127.0.0.1:18080
    volumes:
      - /opt/ndi:/opt/ndi:ro
      - /run/dbus:/run/dbus:ro
  noar-updater:
    ports:
      - "127.0.0.1:18080:8080"
```

On the server's network NoAr cannot resolve the `noar-updater` service name,
so the updater is published on the server's loopback for *Settings ›
Updates*.

## Without Docker

Put `libndi.so.6` where the dynamic loader finds it (for example
`/usr/local/lib`, then `sudo ldconfig`), or start NoAr with
`NDI_RUNTIME_DIR_V6=/opt/ndi`.

## Using it

1. *Settings › Output*: choose **NDI** as the destination and set the **NDI
   source name**. Receivers list it as `SERVER (name)`.
2. Save. Changing the destination restarts the playout.

The output size and frame rate are the ones set on the same page. The video
codec and audio settings on that page only encode the Player preview in
NoAr's interface; NDI itself is uncompressed. Recording works with its own
encoding settings (*Recording › source: encode*); copying the stream output is
not available for NDI.

SCTE-35 is not carried over NDI.

## NDI input (lives)

1. *Storage* or *Scheduler › Live*: choose the **NDI** tab of the signal.
2. Press **Find NDI sources** and pick one, or type the address as
   `ndi://` followed by the name receivers list, for example
   `ndi://STUDIO (Camera 1)`. Spaces are allowed in NDI addresses.
3. **Test signal** receives the source briefly and shows its picture size and
   frame rate.

The live is then scheduled like any other: Clock or Continue, with the same
return to the playlist. A source that sends picture without audio is accepted
after 1.5 seconds and gets silence. Discovery needs mDNS on the server's
network (host networking in Docker) or an NDI Discovery Server.

## Verification status

- Compiled-in elements, name and address validation, and the refusal of NDI
  output and input without the runtime are covered by automated tests.
- Publishing to and receiving from real NDI devices, discovery and the signal
  test have **not** been verified by the developers yet; please test on your
  network before going on air.
