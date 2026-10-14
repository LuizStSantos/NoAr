**XMLTV Program Guide**
================

ffplayout builds an XMLTV guide (EPG) from the channel's playlists and the
`.nfo` files placed in each program folder. Images from those folders (poster,
fanart, episode thumbnail) are linked from the guide and served by ffplayout
itself.

The guide is public, like the HLS files, so IPTV players (Kodi, Jellyfin,
TiviMate, Plex, etc.) can load it without logging in.

```text
http://<server>:8787/xmltv/<channel-id>/guide.xml
```

## Folder layout

Each program has its own folder inside the channel storage, with a Kodi-style
`.nfo` file:

```text
storage/
  Morning News/
    tvshow.nfo               <- program metadata
    poster.jpg               <- program poster
    fanart.jpg               <- wide image (backdrop)
    part 1.mp4
    part 2.mp4
  Summer Love/
    tvshow.nfo
    poster.png
    Season 1/                <- subfolders inherit the program .nfo
      Episode 01.mp4
      Episode 01.nfo         <- optional: episode metadata
      Episode 01-thumb.jpg   <- optional: episode image
  Movies/
    The Big Movie (2019)/
      movie.nfo
      poster.jpg
      movie.mp4
```

### Program `.nfo`

The program `.nfo` is searched for in the clip's folder and then in each parent
folder, up to (but not including) the storage root. Preferred names, in this
order: `tvshow.nfo`, `programa.nfo`, `program.nfo`, `show.nfo`, `movie.nfo`,
`folder.nfo`. If none of them exists, any other `.nfo` in the folder is used,
as long as it does not share its name with a video.

```xml
<tvshow>
  <title>Morning News</title>
  <plot>The day's top stories.</plot>
  <genre>News</genre>
  <mpaa>US:TV-G</mpaa>
  <premiered>2019-03-04</premiered>
  <actor><name>Jane Doe</name><role>Host</role></actor>
  <thumb aspect="poster">poster.jpg</thumb>
</tvshow>
```

### Episode `.nfo` (optional)

An `.nfo` with the same name as a video describes only that video. Its title
becomes the sub-title in the guide, and its plot, date, season and episode take
priority over the program's.

```xml
<episodedetails>
  <title>The Return</title>
  <season>1</season>
  <episode>1</episode>
  <aired>2026-10-14</aired>
  <plot>Helen comes back to town after ten years.</plot>
</episodedetails>
```

When there is no program `.nfo`, the video's own `.nfo` is used as the program
itself (useful for a movie next to `movie-name.nfo`), unless it is an
`<episodedetails>` file.

### Fields read

| `.nfo` field | XMLTV element |
| --- | --- |
| `title` | `title` (program) or `sub-title` (episode) |
| `showtitle` | `title`, when only the episode `.nfo` exists |
| `plot`, `outline` | `desc` |
| `genre` | `category` (program and episode genres combined) |
| `premiered`, `aired`, `releasedate`, `year` | `date` |
| `season`, `episode` | `episode-num` (`xmltv_ns` and `onscreen`, e.g. `S01E02`) |
| `mpaa`, `certification` | `rating`; `US:TV-G` becomes system `US`, value `TV-G` |
| `director`, `actor`, `credits` | `credits` |
| `country` | `country` |
| `thumb`, `fanart` | images, when the folder has no matching image file |

Files encoded as UTF-8, UTF-16 and Windows-1252 are all read correctly. A broken
`.nfo`, or one that only contains a link, does not break the guide: whatever can
be read is used.

### Images

Supported extensions: `jpg`, `jpeg`, `png`, `webp`, `gif`.

| Type | File names looked for |
| --- | --- |
| Program poster | `poster`, `folder`, `cover`, `capa`, `show` |
| Program wide image | `landscape`, `thumb`, `fanart`, `backdrop`, `background` |
| Episode image | `<video>-thumb`, `<video>` |
| Poster of a standalone movie | `<video>-poster`, `<video>` |

The `<icon>` of each guide entry uses the episode image, then the poster, then
the wide image. All of them are also written as
`<image type="poster|backdrop|still">`. A `<thumb>` with an `http(s)://` address
is used as it is.

The channel logo (from the output logo setting) becomes the channel icon in the
guide.

## How the guide is built

- Times follow the playlist, the same way as the program view in ffplayout.
- Consecutive videos of the same program become a single guide entry (e.g. the
  parts of a news show). Episodes with their own `.nfo` stay separate.
- Ads (`"ad": true` in the playlist) and the configured filler are not listed:
  their time is added to the program they interrupt.
- Without an `.nfo`, the title comes from the playlist item title or the file
  name.

## Query parameters

All optional:

| Parameter | Default | Purpose |
| --- | --- | --- |
| `days` | `7` | Days from the start of today (1 to 14). |
| `lang` | — | Language of titles and descriptions, e.g. `en` or `pt-BR`. |
| `tvg_id` | `ffplayout.<id>` | Channel id in the guide; must match the `tvg-id` in the M3U list. |
| `base_url` | request address | Public address for image links, when a reverse proxy serves ffplayout under a sub path. |

Example:

```text
https://tv.example.org/xmltv/1/guide.xml?days=3&lang=en&tvg_id=ExampleTV.us
```

And in the player's M3U list:

```text
#EXTM3U url-tvg="https://tv.example.org/xmltv/1/guide.xml?tvg_id=ExampleTV.us"
#EXTINF:-1 tvg-id="ExampleTV.us",Example TV
https://tv.example.org/public/1/live/stream.m3u8
```

Behind nginx or another proxy, send `X-Forwarded-Proto` and `X-Forwarded-Host`
so image links use the public address.

## Image route

```text
/xmltv/<channel-id>/images/<path-inside-storage>
```

Only image files inside the channel storage are served; videos, `.nfo` files
and paths that leave the storage (including through symbolic links) are
rejected.
