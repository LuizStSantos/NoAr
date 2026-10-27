**Converting uploaded videos**
================

With **Settings › Playlist and storage › Convert uploaded videos to the
output format** on, every video uploaded in Storage is converted once, when
its upload ends, to what the channel sends:

- the output's picture size and frame rate (for example 1280×720 at 25
  frames per second), the aspect ratio kept with black bars, as the playout
  fits a picture;
- H.264 at a high quality (CRF 18), since NoAr encodes it again on air;
- AAC stereo sound at 48 kHz, the playout's sample rate.

On air, NoAr then has no picture to resize and no frame rate to convert, so
the playout uses less CPU. A file with damaged spots is rewritten from what
decodes, so the damage stays out of the air and the log.

## How it works

- One video at a time per channel, in the background, on at most half the
  CPU cores, so the playout keeps its share. It can be used meanwhile, as
  uploaded: put it in a playlist for another day straight away, and that day
  plays the converted file.
- A video whose picture is already the output's (H.264 4:2:0, progressive,
  of that size and at that constant frame rate) but whose sound is not, is
  not encoded again: its picture is copied and only the sound is converted,
  which takes seconds. Should that fail, the video is encoded after all.
- Otherwise the picture is encoded with x264's `veryfast` preset at CRF 18.
  As a rough guide, on 2 cores of a recent CPU a test pattern of an hour of
  1080p at 30 fps took about 11 minutes to 720p at 25 fps and about 17
  minutes to 1080p at 25 fps; busy real footage takes longer, and more cores
  make it faster.
- The converted file is written next to the original under a hidden name
  (`.Name.converting.mp4`) and takes its place in one rename. It keeps the
  original's name, and so its `.nfo` sheet, logo marks and place in
  playlists. A playout that has the original open goes on reading it.
- MP4, MOV, MKV and TS files keep their extension. WebM, AVI and others
  become an `.mp4` of the same name, and the original is removed; a playlist
  made before with the original plays the `.mp4` in its place.
- A video already in the output format (H.264 of that size and frame rate,
  AAC stereo at 48 kHz, or no sound) is left as it is.
- If the conversion fails, the original stays as it was and Storage says why.
- Images and sound files are never converted, nor files copied into the
  storage folder some other way.

The uploads panel, in the corner of every page, follows each video after its
upload with a blue bar while it waits for or goes through conversion, and
then says whether it was converted, was already in the output format or
failed. Storage shows, next to each file, whether it is queued, how far the
conversion is, and whether it was converted, left as it was or failed. The
list is also at `GET /api/file/{id}/conversions` ([API](api.md#files)).

Changing the output size or frame rate later does not convert the videos
again; those uploaded afterwards follow the new settings.
