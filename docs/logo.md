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
  it, open **Logo and breaks in this video** for that video (a row action in Storage, or the
  same button in the Player's clip editor). Then mark, while it plays, where the
  logo goes off and where it comes back, or type the times, and drag the
  handles of each stretch on the bar to adjust it. **Watch stretch**, **Watch
  it go in** and **Watch it come out** play the picked stretch (or the first)
  with 3 seconds before and after, as it airs, and stop by themselves; the
  play button of each row of the list does the same. The same buttons are on
  the SCTE-35 breaks and L-shape tabs. See
  [commercials](commercials.md#a-break-inside-a-video).

## Hiding, swapping or moving the logo on air

While the channel plays, the logo can be hidden, swapped for another image
from the storage, or moved to another place, until the next command or a
server restart. A typical case is
a live whose encoder runs its own commercial break.

- From NoAr: the **Live graphics: logo and L-shape** card in Settings › Logo
  and subtitles shows what the logo and the L-shape are doing. Its Logo row
  has **Hide**, **Back to the channel logo**, **Swap for…** and **Position**
  (a corner, or back to the channel's); its L-shape row is described
  [below](#l-shape).
- From an encoder or a script: create the channel's **graphics key** (logo
  and L-shape) in the same card. It works at once and stays the same until
  you make a new one there.
  Calling these addresses (GET or POST) then changes the logo,
  with the channel's logo animation:

  ```text
  http://<server>:8787/logo/<channel>/hide?key=<KEY>
  http://<server>:8787/logo/<channel>/show?key=<KEY>
  http://<server>:8787/logo/<channel>/replace?key=<KEY>&logo=00-assets/other-logo.png
  http://<server>:8787/logo/<channel>/move?key=<KEY>&x=90&y=5
  http://<server>:8787/logo/<channel>/move?key=<KEY>&position=bottom-left
  http://<server>:8787/logo/<channel>/move?key=<KEY>&position=default
  ```

  `x` and `y` go from 0 to 100: shares of the room around the logo, so
  `x=0` is the left edge, `x=100` the right one and `50` the middle, on any
  output size. `position` is also a place 12 pixels from the edges
  (`top-left`, `top`, `top-right`, `left`, `center`, `right`,
  `bottom-left`, `bottom`, `bottom-right`), an `X:Y` expression like the one
  in the logo settings (`W-w-40:H-h-40`), or `default` for the channel's
  place. A swapped image goes to the same place. In the card, the logo's
  small screen does the same: click or drag the logo where it goes, or move
  it with the arrow keys.

  The card has a tutorial with these addresses ready to copy, and examples for
  curl, vMix, OBS (Advanced Scene Switcher) and Bitfocus Companion.

A swapped logo takes the size, position and opacity of the configured one, and
cross-fades with the image it replaces. A
commercial still keeps any logo off. The API is described in
[api.md](api.md#logo-on-air).

## L-shape

In an L-shape the programme picture shrinks into a corner of the screen and a
full-screen graphic fills the band around it, with news, a sponsor or what is
on next, over the picture of clips, lives and ingest alike; texts stay on
top. Going in, the logo goes off first (0.4 s) and then the picture shrinks
(0.8 s, easing in and out); coming out, the picture grows back first and then
the logo returns. A stretch marked in a video starts this at its first mark
and ends it at its second.

Set it up in **Settings › Logo › L-shape**:

- **L-shape graphic**: an image in the storage, made for the whole screen
  (1920×1080 for a 1080p output; another size is stretched to the output).
  Draw the band and leave the video's space plain: the picture covers it.
  **Download a base for the image editor** gives a PNG of the output's size
  with that space marked in magenta, its size and place in pixels, and the
  title safe area: open it in the editor, draw the band around it on a layer
  above, and export the full image.
- **Video corner**: top right, top left, bottom right or bottom left.
- **Video size**: how much of the screen the picture keeps, from 30% to 95%
  (75% by default), its aspect ratio kept.

The preview there shows the graphic and the picture's rectangle as on air.
Saving loads the graphic at once, without a restart; one that cannot be read
is refused when saving. Without a graphic the L-shape stays off.

It goes on air:

- **For stretches of a video**, marked in **Storage › Trim, logo, breaks and
  L-shape** on the L-shape tab, like the [stretches without logo](#where-the-logo-goes-off-by-itself).
  They are kept in the video's `<name>.marks.json` as `"lshape"` and apply
  wherever the video plays; a playlist that plays part of the video gets the
  stretches of that part. **This video's L-shape graphic** on that tab gives
  the video a graphic of its own (a sponsor of that programme), used for its
  stretches with the channel's size and corner; empty uses the channel's.
- **From NoAr**: the L-shape row of the **Live graphics** card, just below,
  with **Go to L-shape**, **Back to full screen**, **Swap graphic…** and,
  after a swap, **Channel graphic**. It stays until switched back or the
  server restarts. Its four corner buttons and size slider move the video to
  where another graphic needs it; **Back to the graphic’s position** undoes
  that.
- **From an encoder or a script**, with the graphics key, like the logo:

  ```text
  http://<server>:8787/lshape/<channel>/on?key=<KEY>
  http://<server>:8787/lshape/<channel>/off?key=<KEY>
  http://<server>:8787/lshape/<channel>/replace?key=<KEY>&graphic=00-assets/sponsor.png
  http://<server>:8787/lshape/<channel>/on?key=<KEY>&graphic=00-assets/sponsor.png
  http://<server>:8787/lshape/<channel>/reset?key=<KEY>
  ```

  `replace` swaps the graphic without going in, so the next `on` shows it;
  `on` with `graphic` swaps and goes in at once; `reset` goes back to the
  channel's graphic. A swap lasts until `reset` or a server restart.

  For a graphic whose space for the video is in another corner, move it:

  ```text
  http://<server>:8787/lshape/<channel>/move?key=<KEY>&position=bottom-left&scale=70
  http://<server>:8787/lshape/<channel>/on?key=<KEY>&graphic=00-assets/news.png&position=top-left
  http://<server>:8787/lshape/<channel>/move?key=<KEY>&position=default
  ```

  `position` is `top-left`, `top-right`, `bottom-left` or `bottom-right`;
  `scale` is the video's size in percent of the screen (30 to 95), the
  graphic's when left out. It lasts until moved again,
  `position=default` or a server restart.

  The API is described in [api.md](api.md#l-shape).

## Logo in the guide

The XMLTV guide shows the logo on the video as the channel icon, unless
**Logo in the guide (XMLTV)** sets another image, such as a version in colour
instead of a white one. See [xmltv.md](xmltv.md).
