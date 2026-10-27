**XMLTV Program Guide**
================

NoAr builds an XMLTV guide (EPG) from the channel's playlists and the
`.nfo` files placed in each program folder. Images from those folders (poster,
fanart, episode thumbnail) are linked from the guide and served by NoAr
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
    .actors/Jane_Doe.jpg     <- optional: photo of a person in the credits
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

A series needs only its program `.nfo`: every video in the folder and its
subfolders gets the program's title, synopsis and poster in the guide, and
an episode `.nfo` only adds what is particular to one episode (its title,
season and number, image). Storage shows such folders as *series* (or
*film* for a `movie.nfo`), and its file table shows how each video appears in
the guide and whether it uses the series sheet or one of its own. Dates in
file names (`show-16.10.26.mp4`) only order the episodes; they never show up
in the guide.

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
| `director`, `credits`, `writer`, `actor` | `credits` (see below) |
| `country` | `country` |
| `thumb`, `fanart` | images, when the folder has no matching image file |

### Credits

The guide lists the people in the order of the XMLTV DTD: `director`, `actor`
(with the character in `role` and `guest="yes"` or `"no"`), `writer`,
`adapter`, `producer`, `composer`, `editor`, `presenter`, `commentator` and
`guest`. The program's people come first and the episode's are added to them;
someone in both, in the same job, is listed once.

In the `.nfo`, directors and writers are written as Kodi does (`<director>`,
`<credits>`); everyone else is an `<actor>`, and its `<type>` (as Jellyfin
writes it) gives the job:

```xml
<director>Carlos Lima</director>
<credits>Julia Prado</credits>
<actor><name>Ana Souza</name><role>Rose</role></actor>
<actor><name>Peter Alves</name><role>Joe</role><type>GuestStar</type></actor>
<actor><name>Mark Ribeiro</name><type>Presenter</type></actor>
<actor><name>Paul Mendes</name><type>Guest</type></actor>
```

| `<type>` | XMLTV element |
| --- | --- |
| none, `Actor` | `actor guest="no"` |
| `GuestStar` | `actor guest="yes"` |
| `Director`, `Writer`, `Adapter`, `Producer`, `Composer`, `Editor`, `Presenter` (or `Host`), `Commentator`, `Guest` | `director`, `writer`, `adapter`, ... |

A person's photo is an image in the `.actors` folder of the program folder,
named after them as Kodi does: spaces become `_` (`.actors/Ana_Souza.jpg`),
in any letter case and image type. Without one, an `<actor>`'s own `<thumb>`
is used. The guide links the photo as `<image type="person">` inside the
person's element:

```xml
<credits>
  <director>Carlos Lima<image type="person">http://server:8787/xmltv/1/images/Novela/.actors/Carlos_Lima.jpg</image></director>
  <actor role="Rose" guest="no">Ana Souza</actor>
</credits>
```

The program and episode sheets in Storage edit the credits and upload the
photos.

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

The channel icon in the guide is the *Logo in the guide (XMLTV)* image from
Graphics › Logo when one is set, otherwise the logo on the video. A guide logo
that goes missing falls back to the video logo.

## How the guide is built

- Times follow the playlist, the same way as the program view in NoAr.
- Consecutive videos of the same program become a single guide entry (e.g. the
  parts of a news show). Episodes with their own `.nfo` stay separate.
- Ads (`"ad": true` in the playlist), clips from a
  [commercials folder](commercials.md) and the configured filler are not
  listed: their time is added to the program they interrupt.
- Without an `.nfo`, the title comes from the playlist item title or the file
  name.

## Query parameters

All optional:

| Parameter | Default | Purpose |
| --- | --- | --- |
| `days` | `7` | Days from the start of today (1 to 14). |
| `lang` | — | Language of titles and descriptions, e.g. `en` or `pt-BR`. |
| `tvg_id` | `noar.<id>` | Channel id in the guide; must match the `tvg-id` in the M3U list. |
| `base_url` | request address | Public address for image links, when a reverse proxy serves NoAr under a sub path. |

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
