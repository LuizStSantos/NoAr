# API

The API is served by the NoAr process, on the same address as the web
interface: `http://<server>:8787` with the Docker install. Outside Docker,
set the address with `--listen` / `-l`:

```bash
noar --listen 127.0.0.1:8787
```

All JSON endpoints return an error object with a `detail` field on failure.
Unless noted otherwise, `/api` endpoints require:

```http
Authorization: Bearer <ACCESS_TOKEN>
```

`{id}` always means a channel ID. Channel-scoped requests are available only
to users assigned to that channel, unless they are a global admin.

## Authentication and setup

These endpoints do not use a bearer token.

| Method | Endpoint | Request / response |
| --- | --- | --- |
| `POST` | `/auth/login` | `{ "username": "...", "password": "..." }`. Returns `{ "access": "...", "refresh": "..." }`, or a verification message when two-factor authentication is required. |
| `POST` | `/auth/verify` | `{ "username": "...", "code": "123456" }`. Returns access and refresh tokens. Verification codes expire after five minutes. |
| `POST` | `/auth/refresh` | `{ "refresh": "..." }`. Rotates it and returns a new `{ "access": "...", "refresh": "..." }` pair. |
| `POST` | `/auth/logout` | `{ "refresh": "..." }`. Revokes the complete refresh-token family for the session. |
| `GET` | `/api/setup` | Reports whether first-time setup is required. |
| `POST` | `/api/setup` | Completes first-time setup. It works only while no user exists; directory paths must be absolute and cannot be protected system paths. |

Access tokens are valid for 45 minutes and refresh tokens for 14 days. Refresh
tokens are single-use: each successful refresh replaces the submitted token.
Reusing an already rotated token revokes the complete token family and requires
a new login.

```bash
curl -X POST http://127.0.0.1:8787/auth/login \
  -H 'Content-Type: application/json' \
  --data '{"username":"admin","password":"<PASSWORD>"}'
```

Example first-time setup:

```bash
curl -X POST http://127.0.0.1:8787/api/setup \
  -H 'Content-Type: application/json' \
  --data '{
    "username":"admin", "mail":"admin@example.invalid", "password":"<PASSWORD>",
    "two_factor":false, "logs":"/data/logs",
    "playlists":"/data/playlists",
    "public":"/data/public", "storage":"/data/media",
    "shared":false, "smtp_server":"", "smtp_user":"", "smtp_password":"",
    "smtp_starttls":false, "smtp_port":465
  }'
```

The JWT `secret` is neither exposed nor accepted by the API.
Directory paths are accepted only by this first-time setup endpoint. Change
them later in Settings › General, or with `noar -i` on the server.

## Roles

`GA` means global admin, `CA` means channel admin, and `U` means user. All
listed authenticated endpoints also enforce the channel assignment where an
`{id}` is present.

| Access | Meaning |
| --- | --- |
| `GA` | Global admin only |
| `GA, CA` | Global or channel admin |
| `GA, CA, U` | Any authenticated role with access to the target channel |

## Channel and global settings

| Method | Endpoint | Access | Description |
| --- | --- | --- | --- |
| `GET` | `/api/channel/{id}` | `GA, CA, U` | Read one channel. |
| `PATCH` | `/api/channel/{id}` | `GA, CA` | Update a channel. Only a global admin may change its `public`, `playlists`, or `storage` paths. |
| `POST` | `/api/channel` | `GA` | Create a channel. |
| `DELETE` | `/api/channel/{id}` | `GA` | Delete a channel. |
| `GET` | `/api/channels` | `GA, CA, U` | List channels available to the current user. |
| `GET` | `/api/global` | `GA` | Read editable global SMTP and HTTP push settings. Secrets are represented only by `smtp_password_set` and `notification_token_set`. |
| `PUT` | `/api/global` | `GA` | Update SMTP and HTTP push settings. Omit a secret or send an empty value to retain its current value. |
| `POST` | `/api/global/test-mail` | `GA` | `{ "recipient": "..." }`. Sends a test e-mail with the saved SMTP settings. |

## Playout configuration and capabilities

| Method | Endpoint | Access | Description |
| --- | --- | --- | --- |
| `GET` | `/api/playout/config/{id}` | `GA, CA, U` | Read the complete `PlayoutConfig`. |
| `PUT` | `/api/playout/config/{id}` | `GA, CA` | Replace the complete `PlayoutConfig`. Use the response from `GET` as the request shape. The response contains `requires_restart`; mail, notification, and volume changes are runtime-safe and return `false`. |
| `GET` | `/api/playout/outputs/{id}` | `GA, CA, U` | List configured outputs for the channel. |
| `GET` | `/api/playout/codecs/{id}` | `GA, CA, U` | List supported codecs for HLS, RTMP, SRT, UDP, and custom outputs, including compatible hardware encoders and their curated encoder-setting schema. |
| `POST` | `/api/playout/config/{id}/keys/{kind}` | `GA, CA` | `kind` is `logo` (logo and L-shape links) or `scte35`; `{ "action": "renew" }` makes and saves a new key, `{ "action": "clear" }` turns it off. Answers `{ "key": "..." }`. `PUT /api/playout/config/{id}` keeps the stored keys whatever it is sent. |
| `POST` | `/api/playout/config/{id}/test-alert` | `GA, CA` | `{ "kind": "mail", "recipient": "...", "subject": "..." }` or `{ "kind": "push", "topic": "...", "tags": "..." }`. Sends one test alert with these values, saved or not. `409` when no mail or push server is configured. |

`PUT /api/playout/config/{id}` validates output mode, codecs, HLS subtitle
settings, and volume before persisting the change.
Video settings are submitted as `output.video_options`: a string map whose
allowed keys, values, numeric bounds, defaults, and visibility conditions are
provided with the selected video codec by `GET /api/playout/codecs/{id}`.
Audio encoder settings use `output.audio_options` and, for dedicated recording
encoding, `recording.audio_options`. Both are string maps and remain empty by
default so FFmpeg retains its codec defaults. FFmpeg audio-encoding AVOptions
can be submitted through the advanced editor. Options managed by NoAr,
including bitrate, sample rate, channel layout and time base, cannot be
overridden. FFmpeg validates every remaining name and value
before the configuration is stored, so unsupported options cannot cause a
delayed failure during the next output restart.

Network stream transports use the separate `output.protocol_options` string
map. NoAr validates its names and values for the RTMP, SRT, UDP, or custom
URL transport and merges accepted values with its enforced I/O timeout when the
output is opened. The map is rejected for HLS, desktop, and local-file outputs.
Values such as SRT passphrases are stored and returned unchanged by these
authenticated endpoints.

`output.metadata_options` is a separate string map of global container tags,
such as `title`, `copyright`, `service_name`, or `service_provider`. Tag names
and values are validated for safe length and characters before saving. They
are applied to the FFmpeg output context before its header is written, not as
muxer or transport options. Whether a tag is retained or visible depends on
the selected output format. Desktop output does not accept container metadata.

For `output.mode: "stream"`, `output.stream_type` also accepts `"custom"`.
Set `output.stream_format` to an FFmpeg output format such as `"decklink"` and
`output.stream_url` to its target, for example a DeckLink device name. The
format must be available in the FFmpeg libraries linked by NoAr; FFmpeg
performs the final device and codec compatibility check when the playout opens.
Uncompressed video codecs such as `rawvideo` and lossless audio codecs such as
`pcm_s16le` do not use bitrate settings.

## Server

| Method | Endpoint | Access | Description |
| --- | --- | --- | --- |
| `GET` | `/api/system/server` | `GA` | `{ "started_at", "version", "channels", "channels_running" }`; `started_at` is the Unix time this run started. |
| `POST` | `/api/system/restart` | `GA` | Answers, then stops every channel and starts NoAr again in the same process. |
| `GET` | `/api/system/update` | `GA` | Installed version and commit, the newest published image (`latest`: commit, publish time, changes), `available`, `checked_at`, `error` and `auto_check`, as of the last check. |
| `POST` | `/api/system/update/check` | `GA` | Asks GitHub now (at most every 20 s) and answers like `GET /api/system/update`. |
| `PUT` | `/api/system/update/settings` | `GA` | Any of `{ "auto_check": bool, "auto_update": bool, "update_time": "HH:MM" }`; the status also has `scheduled_at` and `updater` (`configured`, `connected`, `address`, `error`). |
| `POST` | `/api/system/update/install` | `GA` | Asks the updater to install the newest image now; NoAr stops moments later and starts on the new version. `409` without the updater. |
| `POST` | `/api/system/update/schedule` | `GA` | `{ "time": "HH:MM" }`: install at the next such time of the server clock. `DELETE` cancels. |

After a restart the channels that were running start again by themselves. A
client can wait for `started_at` to change to know the new run is up.

## Playout control

| Method | Endpoint | Access | Request body |
| --- | --- | --- | --- |
| `POST` | `/api/control/{id}/playout` | `GA, CA, U` | `{ "control": "back" \| "next" \| "reset" }` |
| `PUT` | `/api/control/{id}/audio` | `GA, CA` | `{ "volume": 0.0 }`, from `0.0` through `1.5`. |
| `GET` | `/api/control/{id}/media/current` | `GA, CA, U` | Read the current media and playout state. |
| `POST` | `/api/control/{id}/process` | `GA, CA, U` | `{ "command": "status" \| "start" \| "stop" \| "restart" }` |

During an active live ingest, `back`, `next`, and `reset` return HTTP 200 with
`{ "operation": "ignored", "reason": "live_ingest_active" }`. They do not change
the playlist position or time shift and are not queued for later execution.
Process stop and restart remain available. The `ingest` field in current-media
responses and playout events indicates an actual live takeover, not merely a
listener waiting for a connection.

Current-media responses and playout events also carry `output_link`: how an
SRT, RTMP or UDP output reaches its destination
([docs](output.md#when-the-destination-is-down)).

```json
"output_link": {
  "state": "retrying",
  "destination": "rtmp://live.example.org/…",
  "error": "Connection refused",
  "seconds": 12,
  "connections": 0
}
```

`state` is `idle` (no network output: HLS, desktop, a file), `connecting`,
`listening` (an SRT listener waiting for a receiver), `connected` or
`retrying`; `seconds` is the time in that state and `connections` how many
times the destination was reached since the playout started. The address
leaves out credentials, the query string and the RTMP stream key.

```bash
curl -X POST http://127.0.0.1:8787/api/control/1/process \
  -H 'Authorization: Bearer <ACCESS_TOKEN>' \
  -H 'Content-Type: application/json' \
  --data '{"command":"restart"}'
```

### Logo on air

The logo can be hidden or swapped for another image while the channel plays,
for example during the commercial break of a live. It changes with the
channel's [logo animation](logo.md#how-the-logo-comes-up-and-goes-off) (a
0.4 s fade unless set; a swap cross-fades), applies to clips, lives and live
ingest alike, and lasts until the next
command or a server restart. A commercial in the playlist still keeps any logo
off.

| Method | Endpoint | Access | Request body |
| --- | --- | --- | --- |
| `GET` | `/api/control/{id}/logo` | `GA, CA, U` | Read the state: `{ "hidden": false, "replacement": null, "position": null }`. |
| `POST` | `/api/control/{id}/logo` | `GA, CA, U` | `{ "action": "hide" \| "show" \| "replace" \| "move", "logo": "<storage path>", "position": "bottom-left" }`, or `"x": 90, "y": 5` instead of `position`; `logo` only for `replace`, `position` (or `x`/`y`) only for `move`. |
| `GET`, `POST` | `/logo/{id}/{hide\|show\|replace\|move}` | Logo key | Query `key` (or header `X-Logo-Key`), `logo` for `replace`, `position` or `x` and `y` for `move`. |

`show` brings back the configured logo and drops a replacement. A replacement
takes the size, position and opacity of the configured logo. `move` puts the
logo anywhere:

- `x` and `y`, from 0 to 100: shares of the room around the logo, so 0 is
  the left (or top) edge, 100 the right (or bottom) one and 50 the middle.
  It fits any output size and any logo. `position` takes the same as
  `X%:Y%`, such as `90%:5%`;
- `position` as a place, 12 px from the edges: `top-left`, `top`,
  `top-right`, `left`, `center`, `right`, `bottom-left`, `bottom`,
  `bottom-right`;
- `position` as an `X:Y` expression in pixels, such as `W-w-40:H-h-40`;
- `position` `default`, back to the channel's place.

The state then has `"position"`, which a swapped logo uses too.

The `/logo/...` route is for encoders and scripts that cannot log in (vMix,
OBS, Bitfocus Companion). It needs the channel's logo key, created in
Graphics › External control (API), where a tutorial lists the addresses ready
to copy. Without
a key, or with a wrong one, it answers 403. A key stays the same until it is
renewed or turned off with `POST /api/playout/config/{id}/keys/{logo|scte35}`;
saving the configuration never changes it.

```bash
curl -X POST 'http://127.0.0.1:8787/logo/1/hide?key=<KEY>'
curl -X POST 'http://127.0.0.1:8787/logo/1/replace?key=<KEY>&logo=00-assets/logo-ao-vivo.png'
curl -X POST 'http://127.0.0.1:8787/logo/1/show?key=<KEY>'
curl -X POST 'http://127.0.0.1:8787/logo/1/move?key=<KEY>&x=90&y=5'
curl -X POST 'http://127.0.0.1:8787/logo/1/move?key=<KEY>&position=default'
```

### L-shape

The picture shrinks into a corner over the channel's L-shape graphic
([docs](logo.md#l-shape)). Switched on here, it stays until switched off or
the server restarts; stretches marked in a video go into it by themselves.
The keyed route takes the channel’s graphics key (the logo key).

| Method | Endpoint | Access | Request body |
| --- | --- | --- | --- |
| `GET` | `/api/control/{id}/lshape` | `GA, CA, U` | Read the state: `{ "on": false, "ready": true, "replacement": null, "position": null, "scale": null }`; `ready` is false without a graphic, `replacement` names a graphic swapped in, `position` and `scale` the corner and size the picture was moved to. |
| `POST` | `/api/control/{id}/lshape` | `GA, CA, U` | `{ "action": "on" \| "off" \| "replace" \| "reset" \| "move", "graphic": "<storage path>", "position": "bottom-left", "scale": 70 }`. `replace` swaps the graphic (needs `graphic`); `on` with `graphic` swaps and goes in; `reset` goes back to the channel's graphic; `move` puts the picture elsewhere (below). `on` without any graphic answers 409. |
| `GET`, `POST` | `/lshape/{id}/{on\|off\|replace\|reset\|move}` | Logo key | Query `key` (or header `X-Logo-Key`), `graphic`, `position` and `scale`. |

```bash
curl -X POST 'http://127.0.0.1:8787/lshape/1/on?key=<KEY>'
curl -X POST 'http://127.0.0.1:8787/lshape/1/off?key=<KEY>'
curl -X POST 'http://127.0.0.1:8787/lshape/1/replace?key=<KEY>&graphic=00-assets/sponsor.png'
curl -X POST 'http://127.0.0.1:8787/lshape/1/reset?key=<KEY>'
curl -X POST 'http://127.0.0.1:8787/lshape/1/move?key=<KEY>&position=bottom-left&scale=70'
curl -X POST 'http://127.0.0.1:8787/lshape/1/on?key=<KEY>&position=top-left'
```

`move` (or `on` with a position) puts the picture in another corner, on air
or not, for a graphic whose space for the video is elsewhere: `position` is
`top-left`, `top-right`, `bottom-left` or `bottom-right`. `scale` is its size
in percent of the screen (30 to 95); without it the graphic's size stays. It
applies to the channel's graphic and to a swapped one, not to a video's own
graphic for its marked stretches, until moved again, `position` `default`,
or a server restart.

The logo goes off while the picture is in the L-shape and comes back after.

### Quick scenes

A scene is a named set of logo and L-shape changes saved in the channel
configuration (`processing.graphics_scenes`, at most 24, unique names) and
applied in one go ([docs](logo.md#quick-scenes)).

| Method | Endpoint | Access | Request body |
| --- | --- | --- | --- |
| `POST` | `/api/control/{id}/scene` | `GA, CA, U` | `{ "name": "Intervalo comercial" }`. Answers `{ "scene", "logo", "lshape" }` with the state after it; 404 for an unknown scene, 409 when it goes to the L-shape without a graphic. |
| `GET`, `POST` | `/scene/{id}/{name}` | Logo key | `name` in lower case with dashes (`intervalo-comercial`); query `key` or header `X-Logo-Key`. |

A scene in `graphics_scenes` is
`{ "name", "logo": "keep" | "hide" | "show" | "replace", "logo_image", "logo_position", "lshape": "keep" | "on" | "off", "lshape_graphic", "lshape_corner", "lshape_scale" }`;
`null` values leave that part as it is on air, `logo_position` and
`lshape_corner` also take `default`.

```bash
curl -X POST 'http://127.0.0.1:8787/scene/1/intervalo-comercial?key=<KEY>'
```

### Ad breaks (SCTE-35)

Breaks marked in the SRT or UDP output; see [Ad breaks](ad_breaks.md). The
`/scte35/{id}/{break|cancel}` route takes the channel's SCTE-35 key instead
of a login, for encoders and automation
([docs](ad_breaks.md#from-an-encoder-or-an-automation-with-the-scte-35-key)).

| Method | Endpoint | Access | Request body |
| --- | --- | --- | --- |
| `GET` | `/api/control/{id}/scte35` | `GA, CA, U` | Read the state, the breaks to come and on air, and the last 20. |
| `POST` | `/api/control/{id}/scte35/break` | `GA, CA, U` | `{ "duration": 120 }`: mark a break of that many seconds, from now plus the pre-roll. |
| `DELETE` | `/api/control/{id}/scte35/break/{event_id}` | `GA, CA, U` | Call off a break that has not started. |
| `GET`, `POST` | `/scte35/{id}/{break\|cancel}` | SCTE-35 key | Query `key` (or header `X-Scte35-Key`), `duration` for `break`, `event` for `cancel` (else the next break). |

## Live preview

| Method | Endpoint | Access | Description |
| --- | --- | --- | --- |
| `GET` | `/api/preview/{id}` | `GA, CA, U` | The channel output as a live MPEG-TS stream (`video/mp2t`). |
| `GET` | `/api/preview/{id}?quality=light` | `GA, CA, U` | A lighter copy: 640×360, 15 frames per second, about 0.6 Mb/s. |

The Player shows this stream. It is a copy of the packets the output
encoders already produced, so it costs no extra encoding, works the same for
HLS, RTMP and SRT outputs, and plays about a second behind the air. The
response never ends while the client reads it; nothing is sent while the
playout is stopped, and a client that falls behind skips ahead. The light copy
is encoded from it only while someone reads it
([docs](output.md#light-and-high-quality)).

```bash
curl -N http://127.0.0.1:8787/api/preview/1 \
  -H 'Authorization: Bearer <ACCESS_TOKEN>' | ffplay -
```

## Playlists and programme data

| Method | Endpoint | Access | Description |
| --- | --- | --- | --- |
| `GET` | `/api/playlist/{id}?date=YYYY-MM-DD` | `GA, CA, U` | Read a playlist. |
| `POST` | `/api/playlist/{id}` | `GA, CA, U` | Save a complete playlist JSON document. |
| `POST` | `/api/playlist/{id}/generate/{date}` | `GA, CA, U` | Generate and save a playlist. Body is optional: `{ "paths": ["..."], "shuffle": true, "template": { ... } }`. |
| `POST` | `/api/playlist/{id}/programs` | `GA, CA, U` | Body: `[{ "source": "...", "title": null, "ad": false }]`. Returns, per clip, the guide programme it belongs to: `{ "key", "title", "sub_title", "is_break" }`. Consecutive clips with the same `key` form one guide entry; breaks extend the entry before them. |
| `POST` | `/api/playlist/{id}/episodes` | `GA, CA, U` | Body: `{ "folder": "Show", "date": "YYYY-MM-DD", "after": null }`. The videos of a program folder (with season subfolders) in episode order: by the date in the file name when there is one (`show-16.10.26.mp4`, day first, or `2026-10-16`), else by natural name order. Returns `{ "episodes": [{ "source", "name", "duration" }], "last": { "source", "date" } | null, "next": 0 }`: `last` is `after` (the folder's last clip in the day being edited) or the folder's last clip in the saved playlists of the 60 days before `date`, and `next` is the index of the episode after it, starting over after the last one. |
| `DELETE` | `/api/playlist/{id}/{date}` | `GA, CA, U` | Delete a playlist. |
| `GET` | `/api/program/{id}` | `GA, CA, U` | Read programme items. Optional `start_after` and `start_before` query parameters accept local ISO date-times. |
| `GET` | `/api/log/{id}` | `GA, CA, U` | Read a log. Optional query parameters: `date`, `timezone`, `download`. |
| `GET` | `/api/system/{id}` | `GA, CA, U` | Read system statistics for the channel. |

Playlist items identify advertisements with `"ad": true`. The field is omitted
for regular content and defaults to `false` when absent. For compatibility,
playlist input may still contain `"category": "advertisement"`; the server
reads it as `"ad": true` and only serializes the new field. Programme responses
expose the resolved state as an `ad` boolean.

An item can carry its own `marks` (the same fields as `logo-marks` below,
without `trim`): they apply to that item only, instead of the video's marks
in the storage. They are checked when the playlist is saved; an empty `marks`
is dropped ([docs](logo.md#marks-of-one-playlist-item)).

Scheduled graphics (logo and L-shape changes at a time) are under
`/api/schedule/{id}/graphics`; see [Scheduler](scheduler.md#api).

## Files

All file-management endpoints require `GA, CA, U` access to the channel.
Paths are resolved below the channel storage root; parent-directory traversal is
rejected.

| Method | Endpoint | Request body or query |
| --- | --- | --- |
| `POST` | `/api/file/{id}/browse` | `{ "source": "", "folders_only": false }` |
| `POST` | `/api/file/{id}/create-folder` | `{ "source": "folder" }` |
| `POST` | `/api/file/{id}/rename` | Rename or move a file/folder: `{ "source": "old", "target": "new" }` |
| `POST` | `/api/file/{id}/remove` | `{ "source": "file.mp4", "recursive": false }` |
| `GET` | `/api/file/{id}/upload?path=folder&file_name=clip.mp4&size=1234&batch_id=<ID>` | Returns already received byte ranges for a resumable upload |
| `PUT` | `/api/file/{id}/upload?path=folder` | Chunked `multipart/form-data` upload |
| `GET` | `/api/file/{id}/conversions` | Uploaded videos queued, being or recently converted to the output format: `[{ "path", "converted", "state", "progress", "error", "updated" }]`; `state` is `queued`, `converting`, `done`, `skipped` or `failed`. See [Converting uploaded videos](uploads.md). |
| `PUT` | `/api/file/{id}/import?file=list.m3u&date=YYYY-MM-DD` | `multipart/form-data` with a file field |
| `POST` | `/api/file/{id}/access-token` | `{ "filename": "folder/file.mp4" }` |
| `POST` | `/api/file/{id}/sheet` | `{ "path": "Folder or folder/clip.mp4" }`: the `.nfo` sheet of a program folder or clip |
| `PUT` | `/api/file/{id}/sheet` | `{ "path", "kind": "program", "fields": { ... } }`, or `{ "path", "raw": "<tvshow>...</tvshow>" }` |
| `POST` | `/api/file/{id}/logo-marks` | `{ "path": "folder/clip.mp4" }`: the marks of the video, `{ "hide": [{ "from": 750, "to": 990.5 }], "breaks": [...], "trim": { "from": 8, "to": 1790.5 } }` |
| `PUT` | `/api/file/{id}/logo-marks` | `{ "path", "hide": [...], "breaks": [...] }`: seconds of the video; `hide` keeps the logo off, `breaks` are ad breaks signalled with SCTE-35, `lshape` the stretches in the [L-shape](logo.md#l-shape), with this video's own `lshape_graphic`, `lshape_corner` and `lshape_scale` for them (each optional), and `logo_position` (`X%:Y%`) where the logo goes while it plays ([docs](logo.md#logo-and-l-shape-of-one-video)). At most 16 of each, not overlapping; empty lists remove them. `trim` is the part that airs by default ([trimming videos](trim.md)); `null` plays the whole video. See [commercials](commercials.md#a-break-inside-a-video) and [ad breaks](ad_breaks.md#breaks-marked-inside-a-video). |
| `PUT` | `/api/file/{id}/artwork?path=Folder&kind=poster` | `multipart/form-data` with a `file` field; `kind` is `poster`, `backdrop` or `still` (clips) |
| `POST` | `/api/file/{id}/artwork/remove` | `{ "path": "Folder", "kind": "poster" }` |
| `PUT` | `/api/file/{id}/photo?path=Folder&name=Ana%20Souza` | `multipart/form-data` with a `file` field: the photo of a person in the credits |

The browse response lists `folders`, `files` (with `duration`, `size`,
`modified`, whether a video has its own `sheet`, `logo_marks` when its marks
keep the logo off or mark SCTE-35 breaks, and its `trim` when it has one), `sheet_folders` (the
subfolders with a program sheet) and `movie_folders` (the films among them).

A sheet holds the form `fields` (`title`, `plot`, `genres`, `rating`, `year`,
`premiered`, `countries`, `season`, `episode` and `credits`), the `.nfo` text
as `raw`, its images, the people's `photos` (photo key, the name with `_` for
spaces in lower case, to storage path) and, for a clip, the `program` folder
and its `program_credits`. A credit is
`{ "name": "Ana Souza", "role": "actor", "character": "Rose", "guest": false }`;
`role` is one of `director`, `actor`, `writer`, `adapter`, `producer`,
`composer`, `editor`, `presenter`, `commentator` or `guest`. Photos are stored
in `.actors/` of the program folder as `Ana_Souza.jpg`, replacing an earlier
photo of the same person. See [XMLTV](xmltv.md#credits) for how the guide
lists them.

The upload status response is `{ "received_ranges": [[start, end], ...] }`.
Each `PUT` request contains `fileName`, `start`, `end`, `size`, `batch_id`, and
`chunk` fields. A batch ID identifies concurrent attempts in the running
process. Incomplete data is stored as `<filename>.uploading` with resumable
metadata next to it; the final filename only becomes visible after all byte
ranges have been written successfully.

`POST /api/file/{id}/access-token` returns `{ "access": "...",
"expires_in_seconds": 900 }`. It creates a single-file token bound to the
request IP address and channel. It can be used for browser media previews:

```bash
curl 'http://127.0.0.1:8787/file/1/folder/file.mp4?access=<ACCESS_TOKEN>'
```

`GET /file/{id}/{filename}` also accepts a normal bearer token and supports a
single HTTP `Range` request for seeking.

## Users

| Method | Endpoint | Access | Description |
| --- | --- | --- | --- |
| `GET` | `/api/user` | `GA, CA, U` | Read the current user. |
| `PUT` | `/api/user/{id}` | Self or `GA` | Update a user. Only a global admin can change channel assignments, two-factor settings or the `role_id` (1 global admin, 2 channel admin, 3 operator) of another user. |
| `GET` | `/api/user/{id}` | `GA` | Read a user by ID. |
| `DELETE` | `/api/user/{id}` | `GA` | Delete a user. |
| `POST` | `/api/user` | `GA` | Create a user. |
| `GET` | `/api/users` | `GA` | List users with mail, role, channels and two-factor setting. |

## Server-sent events

SSE connections use a short-lived UUID instead of sending a bearer token in a
browser `EventSource` URL.

1. `POST /api/generate-uuid` with a bearer token. It returns `{ "uuid": "..." }`.
2. Optionally validate it with `GET /data/validate?uuid=<UUID>`.
3. Connect to `GET /data/event/{id}?endpoint=playout&uuid=<UUID>` or
   `GET /data/event/{id}?endpoint=system&uuid=<UUID>`.

Playout events carry the same data as `media/current`, `output_link`
included (see [Playout control](#playout-control)).

The UUID expires after 30 minutes and is bound to the request IP address.

## Public HLS files

HLS playlists, segments, and WebVTT files are public and do not use bearer
authentication:

```text
/public/{channel-id}/live/{playlist-or-segment}
```

For example, the default HLS master playlist of channel 1 is normally:

```text
http://127.0.0.1:8787/public/1/live/master.m3u8
```

Use nginx or another web server for production HLS delivery. The built-in route
is appropriate for previewing and simple deployments.

## Scheduler

Scheduled events and lives: see [Scheduler](/docs/scheduler.md#api) for the
routes under `/api/schedule/{id}` and the event format.

## Public XMLTV guide

The program guide and its artwork are public, so IPTV players can load them
without authentication. See [XMLTV / EPG](/docs/xmltv.md) for the folder layout
and `.nfo` fields.

| Method | Endpoint | Description |
| --- | --- | --- |
| `GET` | `/xmltv/{id}/guide.xml` | XMLTV guide. Optional query parameters: `days` (1-14, default 7), `lang`, `tvg_id`, `base_url`. |
| `GET` | `/xmltv/{id}/images/{path}` | Image file below the channel storage root. Other file types return `404`. |
