Trimming videos
===============

A video often has something at the start or the end that should not air: a
slate, colour bars, a countdown, seconds of black. Trim it once in
**Storage** and it comes in trimmed whenever it goes into a playlist, and in
the Scheduler, folder mode and the playlist generator, instead of setting the
in and out points again each time.

## Trim a video in Storage

In **Storage**, open the folder and choose the scissors on the video's row
(**Trim the video**). The panel **Trim, logo and breaks in this video** opens
on the **Trim** tab:

- **Drag the handles** of the bar to where the video starts and ends on air.
  The parts cut off are striped. A click on the bar moves the play position.
- Or play the video and press **In here** and **Out here** at the right
  frames.
- Or type the times, as `h:mm:ss.d`, `mm:ss.d` or seconds.
- With a handle selected, the arrow keys move it a tenth of a second at a
  time, and a second with Shift.
- **Watch the trim** plays from the in point and stops at the out point;
  **Hear the out point** plays the last 3 seconds before it.
- The undo arrow removes the trim, so the whole video airs again.

Press **Save marks**. Trimmed videos show a "trimmed" tag in Storage; its
tooltip gives the in and out points, and a click opens the panel again.

The trim is kept with the video's other marks, in `<name>.logo.json` next to
it. Renaming, moving or deleting the video in Storage takes that file along;
outside NoAr, move it with the video. Videos of the same name with other
extensions (`Programa.mp4` and `Programa.mov`) share it.

```json
{ "trim": { "from": 8.0, "to": 1790.5 } }
```

Times are seconds of the video. A trim that ends after the end of the file
ends with the file.

## Where the trim applies

| Where | What happens |
| --- | --- |
| Player › Build the playlist | A video added from the library (with **+**, by dragging it or a whole folder, or **Add program**) comes in with the trim as its in and out. The library shows its trimmed length with a scissors icon. |
| Scheduler | Scheduled clips play the trimmed part, and the event's length counts only that part. |
| [Playlist generation](playlist_gen.md) | Generated days use the trimmed part and count its length when filling the day. |
| [Folder mode](folder_mode.md) | Each video plays its trimmed part. |
| Report | Planned lengths are the trimmed ones. |

Clips already in a playlist keep the in and out points they have: the trim
only sets where a video starts and ends when it goes into a playlist. A
playlist clip can always have its own in and out, which win over the trim.
Fillers play whole.

## In the Player's clip editor

The clip editor of the playlist has the same bar: drag the handles, use the
arrows, or type the in and out points. Below it:

- **Trim saved with the video** shows the saved trim, and **Use the saved
  trim** puts it back on the clip.
- **Save this trim with the video** saves the clip's in and out as the video's
  trim, for every playlist it goes into from now on. With the whole video
  selected, the button removes the saved trim instead.
- **Trim watching the video** opens the Storage panel on the **Trim** tab.
  When the clip played the whole video or the saved trim, it takes the new
  trim when you save.

## API

The trim is part of the video's marks: `trim` in
[`/api/file/{id}/logo-marks`](api.md#files), next to `hide` and `breaks`.
`null` or no `trim` plays the whole video. The browse response and the
program episodes list give each video's `trim` when it has one.
