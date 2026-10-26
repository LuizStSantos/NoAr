**Logo**
================

NoAr puts the channel logo over the picture of everything it plays: clips,
scheduled lives and live ingest. The logo is set in **Settings › Logo and
subtitles**.

## Placing the logo

- **File**: an image from the storage, PNG with transparency being the usual
  choice.
- **Position**: one of nine places, and the **margin** from the edges in
  pixels of the output. **Advanced** takes an FFmpeg-style expression instead,
  such as `W-w-30:30` (`W`/`H` are the output size, `w`/`h` the logo's).
- **Size**: a share of the output width, or the image's own size; Advanced
  takes `WIDTH:HEIGHT` in pixels or percent, with `-1` keeping the proportion.
- **Opacity**.

The preview frame has the shape of the output (16:9 for 1920×1080, and so on)
and can show reference lines: thirds and center, the action safe (93 %) and
title safe (90 %) areas, and the margin.

## How the logo comes up and goes off

**Logo animations**, in the same settings page, sets how the logo appears and
disappears, each on its own:

- **Fade**: gradually (the default).
- **Cut**: at once.

It applies everywhere the logo comes and goes: at the start and end of clips,
around commercials and marked stretches, and when it is hidden or shown live.
**Length** sets how long a fade takes, up to 10 s; empty keeps the built-in
lengths, 1 s on clips and 0.4 s when the logo is hidden, shown or swapped live.
Swapping the logo for another image always cross-fades, the old image fading
out as the new one fades in, over that length. Changes take effect at once,
without restarting the playout.

## Where the logo goes off by itself

- **Commercials**: clips marked as commercials in the playlist, and everything
  in a [commercials folder](commercials.md), play without the logo. It fades
  back in on the next clip.
- **Stretches inside a video**: when a programme file has a break edited into
  it, open **Logo in this video** for that video (a row action in Storage, or the
  same button in the Player's clip editor). Then mark, while it plays, where the
  logo goes off and where it comes back, or type the times. See
  [commercials](commercials.md#a-break-inside-a-video).

## Hiding or swapping the logo on air

While the channel plays, the logo can be hidden, or swapped for another image
from the storage, until the next command or a server restart. A typical case is
a live whose encoder runs its own commercial break.

- From NoAr: the **Live logo: hide or swap** card in Settings › Logo and
  subtitles shows what the logo is doing, with **Hide**, **Back to the channel
  logo** and **Swap for…**.
- From an encoder or a script: create the channel's **logo key** in the same
  card and save. Calling these addresses (GET or POST) then changes the logo,
  with the channel's logo animation:

  ```text
  http://<server>:8787/logo/<channel>/hide?key=<KEY>
  http://<server>:8787/logo/<channel>/show?key=<KEY>
  http://<server>:8787/logo/<channel>/replace?key=<KEY>&logo=00-assets/other-logo.png
  ```

  The card has a tutorial with these addresses ready to copy, and examples for
  curl, vMix, OBS (Advanced Scene Switcher) and Bitfocus Companion.

A swapped logo takes the size, position and opacity of the configured one, and
cross-fades with the image it replaces. A
commercial still keeps any logo off. The API is described in
[api.md](api.md#logo-on-air).

## Logo in the guide

The XMLTV guide shows the logo on the video as the channel icon, unless
**Logo in the guide (XMLTV)** sets another image, such as a version in colour
instead of a white one. See [xmltv.md](xmltv.md).
