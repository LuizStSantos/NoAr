# Video cards (SDI/HDMI)

NoAr can play the channel's program on a **Blackmagic DeckLink** card and
take video cards as **live inputs** in the Scheduler.

| Card | Output | Input | GStreamer plugin |
| --- | --- | --- | --- |
| Blackmagic DeckLink, UltraStudio, Duo 2, Quad 2, 8K Pro… | Yes | Yes | `decklink` |
| V4L2 capture cards (Magewell Pro Capture, Epiphan and others with a Linux V4L2 driver) | No | Yes (raw video; audio through ALSA) | `v4l2`, `alsa` |
| AJA | No | No | `ajasrc`/`ajasink` are not part of the Debian GStreamer build |

The plugins ship with NoAr. The **card driver** is installed on the server:
Blackmagic Desktop Video for DeckLink, or the card vendor's V4L2 driver.
NoAr discovers the cards it can see; it never assumes a model.

## Output (DeckLink)

1. *Settings › Output*: choose **SDI**.
2. Pick the **card output** found on the server and the **video mode**, such
   as `1080i 59.94`, `1080p 29.97` or `1080p 30`. Choosing a mode sets the
   output size and frame rate to match it.
3. Save. Changing the destination restarts the playout.

The address is stored as `decklink://<device>?mode=<mode>`; the device number
is the sub-device the driver lists (a Duo 2 has four).

The channel renders whole frames per second. For 29.97 and 59.94i modes it
renders at 30 and the card output repeats or drops a frame when needed to
keep 30000/1001 exactly. Interlaced modes carry each picture in both fields.
The output frame rate must match the mode (for example 29.97 or 30 for
`1080i5994`, 25 for `1080i50`).

The output plays on the card's clock; NoAr keeps 200 ms of buffering to absorb
timing jitter. The video codec and audio settings on the page only encode the
Player preview. Recording works with its own encoding settings. SCTE-35 is not
carried on SDI.

Supported modes: NTSC, PAL, 720p50/59.94/60, 1080p23.98–60, 1080i50/59.94/60
and 2160p23.98–60.

## Input (lives)

*Storage* or *Scheduler › Live*: choose the **Video card** tab and press
**Find video cards**, or type the address:

- `decklink://<device>?mode=auto&connection=sdi` — DeckLink input. `mode`
  may name a mode (`1080i5994`) or detect it (`auto`); `connection` is
  `auto`, `sdi`, `hdmi`, `optical-sdi`, `component`, `composite` or `svideo`.
  Interlaced input is deinterlaced; two audio channels are captured.
- `v4l2:///dev/video0?audio=hw:1,0` — V4L2 capture card, with the ALSA
  capture device that carries its audio. Without `audio` the live gets
  silence. The card must deliver raw video (not MJPEG).

**Test signal** opens the card briefly and shows the picture size and frame
rate.

## Docker

The container needs the devices and, for DeckLink, the driver's API library.
Uncomment the lines in `docker-compose.yml`:

```yaml
services:
  noar:
    devices:
      - /dev/blackmagic:/dev/blackmagic   # DeckLink
      # - /dev/video0:/dev/video0         # V4L2 capture card
      # - /dev/snd:/dev/snd               # its ALSA audio
    volumes:
      - /usr/lib/libDeckLinkAPI.so:/usr/lib/libDeckLinkAPI.so:ro
```

Install Blackmagic Desktop Video on the server first (it provides the kernel
module, `/dev/blackmagic` and `libDeckLinkAPI.so`), check the card with
Blackmagic's `BlackmagicFirmwareUpdater status`, then `docker compose up -d`.
Set the Duo 2/Quad 2 connector profile (half or full duplex) with Blackmagic
Desktop Video Setup.

## Verification status

- Mode names against the installed `decklink` plugin, address parsing,
  frame-rate validation, the output video chain's caps negotiation
  (1080i59.94, 1080p29.97, 1080p30) and the refusal without a card are
  covered by tests or were checked in a GStreamer 1.26 container.
- Playing to and capturing from real cards has **not** been verified: no card
  was available. Test on the server, including a long run, before going on
  air.
