**NoAr documentation**
================

The [README](../README.md) and [Docker](docker.md) are all the install needs.
The pages below describe each part of NoAr. The interface is in Brazilian
Portuguese and English; menus are named here in English.

## Install and run

- **[Docker](docker.md)**: install, data folders, ports and updates. Docker
  is the only supported install.
- **[Versions and releases](releasing.md)**: what a version number means and
  which image tag to pin.

## Programming

- **[Scheduler](scheduler.md)**: breaks, clips and lives at a set time over
  the running programme.
- **[Playlist generation](playlist_gen.md)**: fill days from folders or a
  template.
- **[Folder mode](folder_mode.md)**: play the videos of a folder instead of
  daily playlists.
- **[Videos from a URL](remote_source.md)**: playlist entries that point to a
  remote file or stream.
- **[Commercials folders](commercials.md)**: clips that play as commercials
  everywhere, out of the guide and without the logo, and breaks inside a
  video.

## Picture and output

- **[Output modes](output.md)**: HLS, and RTMP, SRT or UDP streams.
- **[Logo](logo.md)**: placing the logo, where it goes off by itself, and
  hiding or swapping it on air by API.
- **[Closed captions](closed_captions.md)**: WebVTT files as HLS subtitles.

## Lives

- **[Live ingest](live_ingest.md)**: encoders that publish to NoAr and take
  over the channel.
- **[Live ingest on an unstable network](ingest_error.md)**: what happens
  when a publisher drops, and how to test it.

## Guide and report

- **[XMLTV guide](xmltv.md)**: the programme guide built from the playlists
  and the `.nfo` sheets, with artwork.
- **[Report](report.md)**: what actually aired and what is still to come.

## Integration

- **[API](api.md)**: everything the web interface does, for scripts and
  other systems.
- **[HTTP push notifications](notifications.md)**: log alerts to ntfy and
  similar services.
- **[External tasks](external_tasks.md)**: run a script when a clip starts.

## Development

- **[Development](developer.md)**: build and run NoAr from source.
- **[SCTE-35 study](scte35.md)**: what SCTE-35 is, and how NoAr could signal
  and read ad breaks with it.
