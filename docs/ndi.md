# NDI output

NoAr can publish the channel's program as an NDI source: the composited
picture (logo, L-shape, text) and the processed program audio (volume,
loudness), uncompressed, to NDI receivers on the local network such as NDI
Studio Monitor, vMix, OBS or a hardware converter.

NDI input in Scheduler live events and Blackmagic DeckLink SDI input/output
are planned and not available yet.

## How it is packaged

- The GStreamer NDI elements (`gst-plugin-ndi` from gst-plugins-rs) are
  compiled into NoAr. Nothing extra is needed in the image.
- The **NDI Runtime** (`libndi.so.6`) is proprietary and is **not** shipped
  with NoAr. Install it on the server and make it visible to NoAr. Without it,
  NoAr runs normally and the NDI destination in *Settings › Output* is
  disabled with an explanation.
- NoAr checks for the runtime when it starts. After installing it, restart
  NoAr.

## Installing the NDI Runtime on the server

1. Download the NDI SDK for Linux from <https://ndi.video/for-developers/ndi-sdk/>
   and accept its license. Run the installer, which unpacks a folder such as
   `NDI SDK for Linux/`.
2. Copy the runtime library for your architecture to a fixed place, for
   example:

   ```sh
   sudo mkdir -p /opt/ndi
   sudo cp "NDI SDK for Linux/lib/x86_64-linux-gnu/"libndi.so* /opt/ndi/
   ```

3. Discovery uses mDNS. Make sure `avahi-daemon` runs on the server, or
   configure an NDI Discovery Server for your network.

## Docker

Uncomment the NDI lines in `docker-compose.yml` (they are already there,
commented):

```yaml
services:
  noar:
    # NDI receivers find sources through mDNS on the server's network.
    network_mode: host
    environment:
      NDI_RUNTIME_DIR_V6: /opt/ndi
    volumes:
      - /opt/ndi:/opt/ndi:ro
```

With `network_mode: host`, the `ports:` section is ignored and the web
interface stays on port 8787 of the server. Then:

```sh
docker compose up -d
```

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

## Verification status

- Compiled-in elements, name validation and the refusal without the runtime
  are covered by automated tests.
- Publishing to a real NDI receiver has **not** been verified by the
  developers yet; please test on your network before going on air.
