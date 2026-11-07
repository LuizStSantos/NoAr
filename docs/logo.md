**Logo**
================

When a logo model is assigned and enabled, NoAr puts it over the picture of everything it plays: clips,
scheduled lives and live ingest. Everything about it is in the **Graphics**
tab of the main menu. Its navigation has six pages:

- **On air**: model preview, real program monitor, temporary take/out,
  logo/ticker visibility and channel L-shape entry/exit (every user);
- **Templates**: models organized by their authored group labels, with editable
  names. Use **Edit** to open their layers in Composer. Optional logo/L-shape/
  ticker assignments identify the targets of compatibility controls without
  reserving a group. **Manage scenes** also works before the first scene exists;
- **Data sources**: CSV/public Sheets CSV, RSS/Atom or HTTP JSON mapped to fields;
- **XMLTV logo**: the independent guide icon, with storage selection and preview;
- **Subtitles**: the WebVTT subtitles of the HLS output;
- **External control (API)**: the graphics key and a tutorial for encoders.

Logo, L-shape and Quick scenes are no longer separate menu entries. Existing
`/graphics/logo`, `/graphics/lshape` and `/graphics/scenes` links redirect to
Composer (or the scene action editor) with the selected channel preserved. `/graphics/logo#ticker`
opens the ticker model in Composer. Existing API control URLs and responses are unchanged.

The Player reports requested channel-model state and temporary graphics below
its preview, with **Open Graphics** linking to On air. It has no graphics
switches, placement controls or scene buttons. Apply scenes from the Templates
**Scenes** group. Clip marks retain their own placement editor.

The On air **Preview** shows the selected saved model and supplied field values;
it is a browser illustration. **Program** shows actual decoded playout output.
When stopped, unsupported or waiting for a picture, it reports that state.
The first configured channel model is selected even when no ordinary GC exists.
Logo/ticker switches report requested visibility: clip marks, breaks and an
L-shape can still suppress the logo in the actual output.

A fresh installation or new channel starts with only one removable **Logo**
example; no empty ticker or L-shape cards are created. Upgrades retain existing
models. Names and groups are yours to edit, and **Delete template** also works
on assigned models. Its confirmation lists live use and retained event actions.
Deleting detaches assignments and preserves uploaded assets. Legacy scheduled events keep their saved actions and may still run later.
Model schedules retain their reference; a missing template is logged and skipped. Startup does not recreate
removed examples. A missing legacy editor target opens Templates.

## Image sizing and animated GIF

The first upload/storage selection in an empty image layer uses its natural
pixel size relative to the channel output, with proportional fit-down only when
it exceeds the output. Position is clamped inside the canvas. An image with the
same dimensions as the output becomes X/Y 0 and width/height 100%. Replacing an
existing image preserves its authored position and size. Undo restores the prior
image/geometry; the logo's explicit natural-size/corner repair remains available.

GIF animation is decoded by GStreamer before publishing a composition, then
cached frames are selected by their durations on air, preserving transparency.
It works in ordinary/permanent compositions, channel logo and L-shape artwork,
and their image overrides. GIFs play as continuous loops; finite file repeat
counts are not used. Use template duration, event end or take-out to stop one.
Hide/show does not reset an already loaded sequence. Matching file identity,
size and radius share a loop phase across models/refreshes; a new decode, changed
geometry/file or backend restart starts a new sequence. Browser GIF preview runs
independently and is not frame-synchronized to Program.

Limits: 10 MiB file, 256 frames, 60-second cycle, 32 MiB decoded GIF buffers at
its rendered size and 64 MiB GIF buffers per composition. Source dimensions are
bounded too. Invalid/unsupported/oversized animations fail preflight or take
explicitly, instead of silently becoming a still. Native GIF demux/decoder
plugins from GStreamer's libav plugin are required. PNG/JPEG/WebP still-image
rendering retains its previous path. See the [executed handoff](graphics-handoff/README.md)
for actual output proofs and untested output/device boundaries.

## Placing the logo

Open **Graphics › Templates › Channel logo › Edit**. The logo is a model with
an image layer; add other image, text or shape layers if needed. Select the
image to choose a storage path or upload to `Graphics/`, drag it and resize
with the canvas handles, or type X/Y/width/height in percent of the output.
The px toggle uses the actual channel output size. Proportional, scale,
centering and opacity controls apply to the selected layer.

A fresh installation places the bundled 42×42 px logo at the top right with
12 px margins, even before Storage has copied its assets. Existing authored
models are preserved. To repair a previously saved oversized logo, select its
primary image, click **Restore image size and top-right position**, review the
preview and **Save template**. This uses the natural image size, reduces it if
necessary to fit the output and supports **Undo**; it does not save automatically.
The permanent logo/ticker cannot contain a video window; use the channel L-shape
or an ordinary temporary model for that layer.

Enable **Always on air** to load the logo model at startup. The library's
**On air / Off** switch hides or shows it during operation. Hiding does not
delete the definition, and saving a temporarily hidden model keeps it hidden.
Saving a model updates output without restarting playout.

Legacy position/scale expressions remain accepted by the configuration API.
Conversion evaluates them at the output size and stores the resulting layer
geometry; the Composer does not maintain a second expression-based layout.
Live/clip placement controls retain their legacy room-around-the-logo units.

## How the logo comes up and goes off

Select the image layer's entry and exit animation in Composer. **None** gives
a cut; **Fade** gives a fade. Duration and delay are in milliseconds. Additional
layer presets are available in the editor. The timeline previews their timing.

An imported logo without an explicit duration retains the legacy defaults:
1000 ms for clip marks and 400 ms for external controls. Editing Duration in
Composer makes the new duration apply to both. Swapping the primary image
cross-fades that image and preserves other composed layers. External placement
translates the composition relative to its controlled image. Commercial gaps
and L-shape transitions still suppress the logo.

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

- From NoAr: **Graphics › On air › Always on air** hides or shows the logo
  or ticker. Select **Channel L-shape** to enter/leave its composed window.
  To change the saved artwork or position, use **Edit** to open Composer.
  Temporary replacement/placement still uses the compatible API or a scene
  applied from Templates; the former live-control panel is no longer shown.
- From an encoder or a script: create the channel's **graphics key** (logo,
  L-shape and scenes) in **Graphics › External control (API)**. It works at once and stays the same until
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
    place. A swapped image goes to the same place. These are temporary API
    overrides. For a persistent change, edit the primary image layer in Composer
    and save; its coordinates are percentages of the whole output frame.

    **External control (API)** has these addresses ready to copy, and examples
    for curl, vMix, OBS (Advanced Scene Switcher) and Bitfocus Companion.

A swapped logo takes the size, position and opacity of the configured one, and
cross-fades with the image it replaces. A
commercial still keeps any logo off. The API is described in
[api.md](api.md#logo-on-air).

## L-shape

After graphics conversion, channel artwork and its picture window come from
the assigned L-shape model. Replacing only the artwork keeps that window,
including a freely positioned Composer window. External corner/scale moves
remain temporary overrides; resetting the position restores the model's
window. A clip with only an artwork override inherits the model window;
explicit clip corner/scale settings keep their legacy geometry.

In an L-shape the programme picture shrinks into its configured window and a
full-screen graphic fills the band around it, with news, a sponsor or what is
on next, over the picture of clips, lives and ingest alike; texts stay on
top. Going in, the logo goes off first (0.4 s) and then the picture shrinks
(0.8 s, easing in and out); coming out, the picture grows back first and then
the logo returns. A stretch marked in a video starts this at its first mark
and ends it at its second.

Set it up in **Graphics › Templates › Channel L-shape › Edit**, which opens
Composer. Add image, shape or text layers for the artwork and one **Video in
the corner** layer for the programme picture. The picture covers the artwork
inside its window. Select that video layer to choose a corner and size
(30–95%), or drag/resize it to author a free window. Ordinary artwork layers
retain the same editing controls as other templates.

Saving updates the model without restarting the channel. External/clip artwork
replacements change its controlled image while preserving other art layers and
the model window. Only an explicit corner/scale override changes that window.

It goes on air:

- **For stretches of a video**, marked in **Storage › Trim, logo, breaks and
  L-shape** on the L-shape tab, like the [stretches without logo](#where-the-logo-goes-off-by-itself).
  They are kept in the video's `<name>.marks.json` as `"lshape"` and apply
  wherever the video plays; a playlist that plays part of the video gets the
  stretches of that part. **This video's L-shape graphic** on that tab gives
  the video a graphic of its own (a sponsor of that programme), used for its
  stretches with the model's picture window. Explicit clip corner/scale settings
  override that window; empty artwork uses the channel model's image.
- **From NoAr**: select **Channel L-shape** in **Graphics › On air**, then
  **Enter L-shape** or **Return to full screen**; the library offers the same
  controls. **Edit** opens its layers in Composer. Change the artwork and video
  window there, or apply a scene from the Templates **Scenes** group.
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

## Logo and L-shape of one video

A video can show another logo or put it somewhere else, and its L-shape in
another corner or size, for itself alone, wherever it plays: open its marks in
the Storage (**Cut, logo, breaks and L-shape in this video**).

- **Logo** tab › **Logo of this video**: an image from the storage shown in
  place of the channel's logo while the video plays, at the channel's size,
  place and opacity (or the video's own place). It needs the channel's logo
  on; commercials still play without a logo.
- **Logo** tab › **Logo position in this video**: **This video's own**, then
  drag or type it. It applies while the video plays.
- **L-shape** tab › **This video's L-shape**: its graphic, the video's corner
  and its size, for its L-shape stretches; each left empty uses the channel's.
  Pick the corner where that graphic left the video's space.

They are saved in the video's `<name>.marks.json` (`logo_image`,
`logo_position`, `lshape_graphic`, `lshape_corner`, `lshape_scale`). A swap or
move from outside (a scene or the API) goes before the video's own
logo and position.

### Marks of one playlist item

In the **Playlist** page, **Edit** on an item › **Marks of this entry** opens
the same editor for that item alone: what is changed there applies only where
it sits in that day's playlist, and the video's marks in the Storage stay as
they are. The item shows **own** while it has them; **Back to the Storage's**
drops them. They are saved with the playlist, in the item's `marks` field,
which takes the same fields as `<name>.marks.json` (its trim becomes the
item's in and out points). Items without `marks` use the video's.

## Quick scenes

A scene is a named set of changes for a moment that repeats, like a
commercial break: hide the logo, swap it or move it, and put the L-shape in or
out with a given graphic, corner and size. Make them in **Graphics › Templates › Scenes › Manage scenes** (up to 24 per channel) and save. Each scene changes only what it
says; anything set to **Leave as it is** stays as it is on air.

A scene is applied with **Apply** in **Graphics › Templates › Scenes**,
or from an encoder with the graphics key, its name in the address in lower
case with dashes:

```text
http://<server>:8787/scene/<channel>/intervalo-comercial?key=<KEY>
```

**Graphics › External control (API)** lists each saved scene's address ready
to copy. A scene applies the L-shape graphic and place first, then the logo,
then the L-shape in or out, so the picture shrinks straight to its corner.

Existing scene-action schedules keep their compatible reversal behavior. New
[Scheduler](scheduler.md#scheduled-graphics) events select Composer models,
including L-shape artwork and its video window.

## Logo in the guide

The XMLTV guide uses the configured model's controlled image as the channel icon, unless
**Logo in the guide (XMLTV)** sets another image, such as a version in colour
instead of a white one. Set this independent guide path in **Graphics › XMLTV logo** and use **Save guide icon**. Transient on-air replacements do
not change the guide icon. See [xmltv.md](xmltv.md).
