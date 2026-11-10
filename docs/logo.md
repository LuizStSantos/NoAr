**Logo**
================

The channel logo, the ticker, L-shapes and lower thirds are all graphics
templates made in the Composer, put on air and taken off with the same two
commands. Everything about them is in the **Graphics** tab of the main menu:

- **On air**: template preview, the real program monitor, put on air / take
  off, and the logo and ticker switches (every user);
- **Templates**: the channel's templates by group. **Edit** opens a template's
  layers in the Composer; **+ New template** starts one;
- **Data sources**: CSV/public Sheets CSV, RSS/Atom or HTTP JSON mapped to fields;
- **XMLTV logo**: the independent guide icon, with storage selection and preview;
- **Subtitles**: the WebVTT subtitles of the HLS output;
- **External control (API)**: the graphics key and the take/out commands ready
  to copy.

Old `/graphics/logo` and `/graphics/lshape` links open the channel logo or
L-shape in the Composer; `/graphics/scenes` opens Templates.

The Player reports what is on air below its preview, with **Open Graphics**
linking to On air. The On air **Preview** is a browser illustration; **Program**
shows the actual decoded playout output. The logo and ticker switches report
the requested state: commercials and an L-shape can still keep the logo off in
the output.

A fresh installation or a new channel starts with one removable **Logo**
template, whose image is in the storage's `Graphics` folder (a storage set up
by an earlier version keeps `00-assets`). Images uploaded in the Composer go
to `Graphics` too. Deleting a template takes it off air and keeps its uploaded
images.

## Composer canvas guides

Composer uses a neutral checkerboard for transparency, with grid lines every
10% of the output to help position layers. Enable **Safety margins** above the
canvas to show two guides: white at 5% from each edge for action, yellow at 10%
for text. These insets leave a 90% action area and an 80% text area.
The guides scale with the canvas on desktop and phone, and pointer gestures
pass through them. They are local editor aids: they do not add layers, modify
the model, appear in On air or change the output. The checkbox starts off when
the editor is opened/reloaded. Opaque full-frame artwork covers the checkerboard;
enabled safety guides remain visible above the artwork.

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
- **During an L-shape**: the logo goes off before the picture shrinks and
  comes back after it grew back.
- **At times inside a video**: mark it in the video's graphics (below).

## Putting graphics on and off air

Every graphic is a template made in the Composer: a GC (lower third), the
channel logo, the ticker, an L-shape. Two commands drive them all:

- **Put on air**, with an optional time in seconds. 0, or no time, keeps it on
  air until it is taken off.
- **Take off air**.

From NoAr, use **Graphics › On air** or the template cards in
**Graphics › Templates** (the logo and the ticker have an On air / Off
switch). From an encoder, vMix, OBS, a Stream Deck or a script, create the
channel's **graphics key** in **Graphics › External control (API)** and call
these addresses (GET or POST), naming the template by its name in lower case
with dashes, or by its number:

```text
http://<server>:8787/graphics/<channel>/take/comercial-l-shape?key=<KEY>&duration=50
http://<server>:8787/graphics/<channel>/take/entrevistado?key=<KEY>&nome=Maria
http://<server>:8787/graphics/<channel>/out/logo?key=<KEY>
```

Other query parameters fill the template's fields. **External control (API)**
shows these addresses, and the same commands with a login token, ready to copy
for the template and time you pick. The API is described in
[api.md](api.md#graphics-commands).

The routes of earlier versions (`/logo/...`, `/lshape/...`, `/scene/...` and
`/api/control/{id}/logo|lshape|scene`) were removed in 3.0: update encoders
and buttons that called them.

## L-shape

An L-shape is a template with a **Video in the corner** layer: the programme
picture shrinks into that window and the template's other layers (images,
shapes, texts) fill the band around it, over clips, lives and ingest alike.
Going in, the logo goes off first (0.4 s) and then the picture shrinks
(0.8 s); coming out, the picture grows back first and then the logo returns.

Make it in the Composer: add the artwork layers and one **Video in the
corner** layer, then pick its corner and size (30–95%) or drag it to any
place. Put it on and off air like any other template. A template that goes
over the picture (a GC) follows the picture into the corner, waits for the
L-shape to end or does not go on air, as set in its **During an L-shape**
option.

## Graphics inside a video

Open the video's marks in Storage (**Trim, graphics and breaks in this
video**, a row action, or the same button in the Player's clip editor) and use
the **Graphics** tab. Each line is a cue: at a time of the video, a template
goes on air (for a time, or until taken off) or goes off. **Template on at
…** and **Template off at …** add a cue at the play position; type the time,
pick the action and the template, and **Go** jumps to it.

Cues are saved in the video's `<name>.marks.json` as `"graphics"` and run
wherever the video plays, counted from the start of the file: a playlist that
plays part of the video skips the cues before its in point. Logo and L-shape
stretches marked by earlier versions are no longer read.

### Marks of one playlist item

In the **Playlist** page, **Edit** on an item › **Marks of this entry** opens
the same editor for that item alone: what is changed there applies only where
it sits in that day's playlist, and the video's marks in the Storage stay as
they are. The item shows **own** while it has them; **Back to the Storage's**
drops them. They are saved with the playlist, in the item's `marks` field,
which takes the same fields as `<name>.marks.json` (its trim becomes the
item's in and out points). Items without `marks` use the video's.

## Scheduled graphics

The [Scheduler](scheduler.md#scheduled-graphics) puts a template on air at a
time and takes it off at the end, once or on chosen weekdays. Quick scenes and
scheduled logo/L-shape changes of earlier versions were removed in 3.0.

## Logo in the guide

The XMLTV guide uses the configured model's controlled image as the channel icon, unless
**Logo in the guide (XMLTV)** sets another image, such as a version in colour
instead of a white one. Set this independent guide path in **Graphics › XMLTV logo** and use **Save guide icon**. Transient on-air replacements do
not change the guide icon. See [xmltv.md](xmltv.md).
