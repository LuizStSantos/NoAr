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

| Method | Endpoint        | Request / response                                                                                                                                                 |
| ------ | --------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `POST` | `/auth/login`   | `{ "username": "...", "password": "..." }`. Returns `{ "access": "...", "refresh": "..." }`, or a verification message when two-factor authentication is required. |
| `POST` | `/auth/verify`  | `{ "username": "...", "code": "123456" }`. Returns access and refresh tokens. Verification codes expire after five minutes.                                        |
| `POST` | `/auth/refresh` | `{ "refresh": "..." }`. Rotates it and returns a new `{ "access": "...", "refresh": "..." }` pair.                                                                 |
| `POST` | `/auth/logout`  | `{ "refresh": "..." }`. Revokes the complete refresh-token family for the session.                                                                                 |
| `GET`  | `/api/setup`    | Reports whether first-time setup is required.                                                                                                                      |
| `POST` | `/api/setup`    | Completes first-time setup. It works only while no user exists; directory paths must be absolute and cannot be protected system paths.                             |

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

| Access      | Meaning                                                  |
| ----------- | -------------------------------------------------------- |
| `GA`        | Global admin only                                        |
| `GA, CA`    | Global or channel admin                                  |
| `GA, CA, U` | Any authenticated role with access to the target channel |

## Channel and global settings

| Method   | Endpoint                | Access      | Description                                                                                                                         |
| -------- | ----------------------- | ----------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| `GET`    | `/api/channel/{id}`     | `GA, CA, U` | Read one channel.                                                                                                                   |
| `PATCH`  | `/api/channel/{id}`     | `GA, CA`    | Update a channel. Only a global admin may change its `public`, `playlists`, or `storage` paths.                                     |
| `POST`   | `/api/channel`          | `GA`        | Create a channel.                                                                                                                   |
| `DELETE` | `/api/channel/{id}`     | `GA`        | Delete a channel.                                                                                                                   |
| `GET`    | `/api/channels`         | `GA, CA, U` | List channels available to the current user.                                                                                        |
| `GET`    | `/api/global`           | `GA`        | Read editable global SMTP and HTTP push settings. Secrets are represented only by `smtp_password_set` and `notification_token_set`. |
| `PUT`    | `/api/global`           | `GA`        | Update SMTP and HTTP push settings. Omit a secret or send an empty value to retain its current value.                               |
| `POST`   | `/api/global/test-mail` | `GA`        | `{ "recipient": "..." }`. Sends a test e-mail with the saved SMTP settings.                                                         |

## Playout configuration and capabilities

| Method | Endpoint                               | Access      | Description                                                                                                                                                                                                                                           |
| ------ | -------------------------------------- | ----------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `GET`  | `/api/playout/config/{id}`             | `GA, CA, U` | Read the complete `PlayoutConfig`.                                                                                                                                                                                                                    |
| `PUT`  | `/api/playout/config/{id}`             | `GA, CA`    | Replace the complete `PlayoutConfig`. Use the response from `GET` as the request shape. The response contains `requires_restart`; mail, notification, and volume changes are runtime-safe and return `false`.                                         |
| `GET`  | `/api/playout/outputs/{id}`            | `GA, CA, U` | List configured outputs for the channel.                                                                                                                                                                                                              |
| `GET`  | `/api/playout/codecs/{id}`             | `GA, CA, U` | List supported codecs for HLS, RTMP, SRT, UDP, and custom outputs, including compatible hardware encoders and their curated encoder-setting schema.                                                                                                   |
| `POST` | `/api/playout/config/{id}/keys/{kind}` | `GA, CA`    | `kind` is `logo` (logo and L-shape links) or `scte35`; `{ "action": "renew" }` makes and saves a new key, `{ "action": "clear" }` turns it off. Answers `{ "key": "..." }`. `PUT /api/playout/config/{id}` keeps the stored keys whatever it is sent. |
| `POST` | `/api/playout/config/{id}/test-alert`  | `GA, CA`    | `{ "kind": "mail", "recipient": "...", "subject": "..." }` or `{ "kind": "push", "topic": "...", "tags": "..." }`. Sends one test alert with these values, saved or not. `409` when no mail or push server is configured.                             |

`logging.media_level` is the level of the media engine's (GStreamer's)
messages in the log. Until 2.0 it was named `ffmpeg_level`: requests may still
send that name, and responses still carry it as a deprecated copy of
`media_level` for older clients. When a request has both, `media_level` is
used.

`PUT /api/playout/config/{id}` validates output mode, codecs, HLS subtitle
settings, and volume before persisting the change.
Video settings are submitted as `output.video_options`: a string map whose
allowed keys, values, numeric bounds, defaults, and visibility conditions are
provided with the selected video codec by `GET /api/playout/codecs/{id}`.
Audio encoder settings use `output.audio_options` and, for dedicated recording
encoding, `recording.audio_options`. Both are string maps and remain empty by
default so the encoder keeps its own defaults. Further encoder settings can be
submitted through the advanced editor. Options managed by NoAr, including
bitrate, sample rate, channel layout and time base, cannot be overridden.
NoAr builds the GStreamer encoder with every remaining name and value before
the configuration is stored, so unsupported options cannot cause a delayed
failure during the next output restart. Codec names keep the values stored
before GStreamer (`libx264`, `aac`, `libopus` and so on).

Network stream transports use the separate `output.protocol_options` string
map. NoAr validates its names and values for the RTMP, SRT, UDP, or custom
URL transport and merges accepted values with its enforced I/O timeout when the
output is opened. The map is rejected for HLS, desktop, and local-file outputs.
Values such as SRT passphrases are stored and returned unchanged by these
authenticated endpoints.

`output.metadata_options` is a separate string map of global container tags,
such as `title`, `copyright`, `service_name`, or `service_provider`. Tag names
and values are validated for safe length and characters before saving. They
are set as tags on the GStreamer muxer (on MPEG-TS, as the service name and
provider of the SDT), not as muxer or transport options. Whether a tag is retained or visible depends on
the selected output format. Desktop output does not accept container metadata.

For `output.mode: "stream"`, `output.stream_type` also accepts `"custom"`.
Set `output.stream_format` to one of the container formats NoAr can write with
GStreamer (`mpegts`, `flv`, `matroska`, `webm`, `mp4` or `mov`) and
`output.stream_url` to its target. DeckLink cards and NDI have output modes of
their own (`sdi`, `ndi`).
Uncompressed video codecs such as `rawvideo` and lossless audio codecs such as
`pcm_s16le` do not use bitrate settings.

## Server

| Method | Endpoint                      | Access | Description                                                                                                                                                                     |
| ------ | ----------------------------- | ------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `GET`  | `/api/system/server`          | `GA`   | `{ "started_at", "version", "channels", "channels_running" }`; `started_at` is the Unix time this run started.                                                                  |
| `POST` | `/api/system/restart`         | `GA`   | Answers, then stops every channel and starts NoAr again in the same process.                                                                                                    |
| `GET`  | `/api/system/update`          | `GA`   | Installed version and commit, the newest published image (`latest`: commit, publish time, changes), `available`, `checked_at`, `error` and `auto_check`, as of the last check.  |
| `POST` | `/api/system/update/check`    | `GA`   | Asks GitHub now (at most every 20 s) and answers like `GET /api/system/update`.                                                                                                 |
| `PUT`  | `/api/system/update/settings` | `GA`   | Any of `{ "auto_check": bool, "auto_update": bool, "update_time": "HH:MM" }`; the status also has `scheduled_at` and `updater` (`configured`, `connected`, `address`, `error`). |
| `POST` | `/api/system/update/install`  | `GA`   | Asks the updater to install the newest image now; NoAr stops moments later and starts on the new version. `409` without the updater.                                            |
| `POST` | `/api/system/update/schedule` | `GA`   | `{ "time": "HH:MM" }`: install at the next such time of the server clock. `DELETE` cancels.                                                                                     |

After a restart the channels that were running start again by themselves. A
client can wait for `started_at` to change to know the new run is up.

### Health check

`GET /health` needs no login, for uptime monitors (Uptime Kuma, Zabbix,
Nagios…). It answers `200` when every channel set to run is on air and `503`
when one is not:

```json
{
    "status": "ok",
    "version": "2.2.0",
    "channels": [{ "id": 1, "state": "on_air", "uptime": 3600, "output_age": 0.2 }]
}
```

| `state`    | Meaning                                                     | Healthy |
| ---------- | ----------------------------------------------------------- | ------- |
| `on_air`   | Running, and the output carried something in the last 10 s. | yes     |
| `starting` | Running for less than 30 s, nothing output yet.             | yes     |
| `stopped`  | Stopped on purpose.                                         | yes     |
| `frozen`   | Running, but the output carried nothing for over 10 s.      | no      |
| `failed`   | Off air after an error, being retried.                      | no      |
| `down`     | Set to run, not running.                                    | no      |

`output_age` is the seconds since the output last carried sound (it carries
silence too, so black or a filler counts as on air); `uptime` the seconds
since the playout started. Only channel ids and states are shown. A manual
restart reads `down` or `starting` for a moment, so let the monitor retry
before it alerts.

## Playout control

| Method | Endpoint                          | Access      | Request body                                                                                                                                                                                                                                                                   |
| ------ | --------------------------------- | ----------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `POST` | `/api/control/{id}/playout`       | `GA, CA, U` | `{ "control": "back" \| "next" \| "reset" }`                                                                                                                                                                                                                                   |
| `PUT`  | `/api/control/{id}/audio`         | `GA, CA`    | `{ "volume": 0.0 }`, from `0.0` through `1.5`.                                                                                                                                                                                                                                 |
| `GET`  | `/api/control/{id}/media/current` | `GA, CA, U` | Read the current media and playout state. `index` is the entry of the saved playlist on air (the clips of an assembled program share their entry, and `elapsed` counts from the start of the entry); during a scheduled event it is `null` and `media` covers the whole event. |
| `POST` | `/api/control/{id}/process`       | `GA, CA, U` | `{ "command": "status" \| "start" \| "stop" \| "restart" }`                                                                                                                                                                                                                    |

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

They also carry `failure`: `null` while the playout is on air, or why it is
off air while NoAr starts it again (an output card missing, an output that
does not open). The clip in `media` is then the last one that aired, not one
airing now.

```json
"failure": {
  "message": "Conflict: opening DeckLink output 1 in 1080p25 (driver, device number and mode): Element failed to change its state",
  "since": 1793650133,
  "retry_in": 6,
  "attempts": 4
}
```

`since` is when the failures began (Unix seconds), `retry_in` the seconds to
the next attempt and `attempts` how many tries failed in a row. Addresses in
`message` keep only their host. The Player and the header show **OFF AIR**
with this message instead of **ON AIR**.

```bash
curl -X POST http://127.0.0.1:8787/api/control/1/process \
  -H 'Authorization: Bearer <ACCESS_TOKEN>' \
  -H 'Content-Type: application/json' \
  --data '{"command":"restart"}'
```

### Graphics commands

Two commands drive every graphics template: a GC (lower third), an L-shape
(a template with a video window) and an always-on logo or ticker. They replace
the removed `/logo`, `/lshape`, `/scene` and `/api/control/{id}/logo|lshape|scene`
routes (see the [changelog](../CHANGELOG.md)).

| Method | Endpoint | Access | Request |
| --- | --- | --- | --- |
| `POST` | `/api/graphics/{id}/take` | `GA, CA, U` | `{ "template": "comercial-l-shape", "duration": 50, "fields": { ... } }`. `template` is the template id or its name in lower case with dashes; `template_id` also works. `duration` is in seconds, 0 or none keeps it on air until it is taken off. |
| `POST` | `/api/graphics/{id}/out` | `GA, CA, U` | `{ "template": "logo" }` takes that template off (its GCs, or the logo, ticker or L-shape it is); `{ "take_id" }` one GC; `{}` every GC, leaving always-on templates on. |
| `GET`, `POST` | `/graphics/{id}/take/{template}` | Graphics key | Query `key` (or header `X-Graphics-Key`), optional `duration`; every other parameter fills the template field of the same name. |
| `GET`, `POST` | `/graphics/{id}/out/{template}` | Graphics key | Query `key` or header `X-Graphics-Key`. |
| `GET`, `POST` | `/graphics/{id}/out` | Graphics key | Takes everything off air: GCs, GCs waiting for the L-shape, and the logo, ticker, L-shape and always-on templates. `except=logo,ticker` (names or ids, separated by commas) keeps those on; an unknown name answers 404 and changes nothing. Answers the on-air list below. |
| `GET`, `POST` | `/graphics/{id}/on-air` | Graphics key | Everything on air: `[{ "template_id", "template", "name", "kind" }]`, where `template` is the name to use in commands and `kind` is `logo`, `ticker`, `lshape`, `permanent` or `temporary`; a GC also has `since` (seconds) and `take_id`. |

`take` answers `{ "state": "on_air", "template_id", "take_id" }` (`take_id`
only for a GC) or `{ "state": "waiting", "template_id" }` when a GC waits for
the L-shape on air to end; the keyed `out` answers
`{ "state": "off", "template_id" }`. The graphics key is the channel key that
`/api/playout/config/{id}/keys/logo` renews.

```bash
curl -X POST 'http://127.0.0.1:8787/graphics/1/take/comercial-l-shape?key=<KEY>&duration=50'
curl -X POST 'http://127.0.0.1:8787/graphics/1/out/logo?key=<KEY>'
curl -X POST 'http://127.0.0.1:8787/graphics/1/out?key=<KEY>&except=logo'
curl 'http://127.0.0.1:8787/graphics/1/on-air?key=<KEY>'
```

### Graphics templates

The Graphics → External control (API) page explains channel versus temporary
models, shows examples with saved model IDs, and distinguishes login tokens
from the legacy graphics key. Its examples do not execute on page load.

A template is a graphic a channel designs once and puts on air many times:
layers placed in percent of the frame, with `{field}` names filled in on
air ([roadmap](graphics-roadmap.md)).

Every template is put on air and taken off with the
[graphics commands](#graphics-commands).

Channel `on` is the requested runtime control state; clip-logo gaps and L-shape
suppression can still hide a logo on a particular frame. Scene `on` is always
false because Apply is a one-shot action, not a toggle or undo. For roles,
`ready` checks saved content availability, not successful output decoding.
Scenes are offered as saved actions; applying one still validates its assets. Thumbnails show
saved models rather than the program monitor or all transient replacements.
Showing a previously disabled logo/ticker enables its binding; hiding changes
runtime visibility while retaining its definition. A full server restart resets
these transient hide/show overrides and loads enabled definitions again, as for
the legacy controls. A playout restart retains the shared control handles.
Templates opens Composer for channel logo/ticker/L-shape models. Scenes retain
their action editor at `/graphics/channel/scene`. Old channel-model and legacy
UI URLs resolve their binding and redirect with the channel preserved; the old
logo `#ticker` link opens the ticker model. These redirects do not change legacy
HTTP control endpoints. Text layers support optional `scroll` configuration:
`{"direction":"left","speed":80,"gap":80}` (or `"right"`). Speed is output
pixels per second, from 1 to 2000; gap is 0–4096 output pixels. Omitted/null
means static text. Non-text layers reject scrolling. The prepared line is
clipped to the layer and repeats; fields/data mappings retain their existing
semantics and refresh without restarting the take clock. See the
[scrolling guide](graphics-data-sources.md#scroll-text-and-live-information).
Clock layers remain a separate kind. The separate
`/graphics/xmltv` tab edits `processing.guide_logo`, the independent guide icon,
through the existing configuration API; it does not change on-air model layers.

Templates expose nullable `always_on` and `control_layer_id`. `always_on:true`
loads a compatible model at startup, including unbound user-authored templates;
false/null is temporary unless it has a compatibility role. Omitting it on PUT
preserves the saved selection. Video-window templates cannot be permanent. At
most 16 permanent models are allowed per channel, enforced transactionally;
capacity conflicts return 409 before persisting the requested change. `control_layer_id` identifies the image addressed by
legacy replacement/placement controls on logo/L-shape models. Omission preserves
the saved ID across layer reordering; deletion chooses another image once.
Bound models require literal content, and only L-shape models contain a visible
video window. Ordinary templates retain operator fields.

Layers can additionally contain `clip_animation_ms`: the imported legacy clip
fade duration when it differs from external control timing. Composer clears this
override when Duration is edited. A zero duration produces a cut. Normal layer
geometry, opacity and animations are authoritative; compatibility configuration
reads project these values and writes update layers, not layout `settings`.

| Method   | Endpoint                                                | Access      | Request body                                                                                                                                                                                                                                                                                                                                                                               |
| -------- | ------------------------------------------------------- | ----------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `GET`    | `/api/graphics/{id}/channel`                            | `GA, CA, U` | Assigned and permanent models with `kind` (logo/ticker/lshape/scene/permanent), `template` (saved preview), requested `on`, content-availability `ready`, and nullable `scene_name`.                                                                                                                                                                                                       |
| `POST`   | `/api/graphics/{id}/channel/{template_id}`              | `GA, CA, U` | `{ "on": true/false }` controls the assigned model on this channel; returns the refreshed list above. 404 for an unassigned/other-channel model; 409 when content must first be configured or L-shape artwork cannot be loaded. For scenes, `on:true` applies the action; `on:false` is 400.                                                                                               |
| `GET`    | `/api/graphics/{id}/fonts`                              | `GA, CA, U` | Available font family names for text layers.                                                                                                                                                                                                                                                                                                                                               |
| `GET`    | `/api/graphics/{id}/templates`                          | `GA, CA, U` | Every template of the channel, sorted by group and name.                                                                                                                                                                                                                                                                                                                                   |
| `GET`    | `/api/graphics/{id}/templates/{template_id}`            | `GA, CA, U` | One template; 404 when the channel has no such template.                                                                                                                                                                                                                                                                                                                                   |
| `POST`   | `/api/graphics/{id}/templates`                          | `GA, CA`    | A template (below). 409 when the channel already has that name, ignoring case.                                                                                                                                                                                                                                                                                                             |
| `PUT`    | `/api/graphics/{id}/templates/{template_id}`            | `GA, CA`    | The whole template again.                                                                                                                                                                                                                                                                                                                                                                  |
| `DELETE` | `/api/graphics/{id}/templates/{template_id}`            | `GA, CA`    | `"removed"`. Detaches channel/scene assignments, takes active temporary instances out and reloads model state. Removed logo/L-shape bindings also clear transient external/clip artwork and placement; L-shape returns to full screen. Uploaded media/fonts and scheduled event records remain. Legacy action snapshots may still execute; a deleted model reference cannot air until edited. A reload failure after commit is reported; restart restores the saved state. Intentional removal is never re-imported.                                                                                          |
| `GET`    | `/api/graphics/{id}/templates/{template_id}/usage`      | `GA, CA, U` | Assignment, current/startup visibility, temporary instance count and retained scheduled actions potentially affected by removing the assignment. Legacy events retain action snapshots; new events retain model references and are skipped if their template is deleted.                                                                                                                                                                                         |
| `PUT`    | `/api/graphics/{id}/templates/{template_id}/assignment` | `GA, CA`    | `{ "role": "logo"/"ticker"/"lshape"/null, "control_layer_id": "image-id" }`. Optional role and image target; does not rename or regroup. Omitted target retains the saved image or chooses the first image initially. Invalid/foreign targets are 400. Replacing a role makes the old model temporary to avoid a second permanent copy. Removing/changing this model's previous role clears that role's transient external/clip artwork and placement. Unassigning retains the model and its independent permanent setting; ordinary saves and replacement assignments preserve shared transient controls.                                                    |
| `POST`   | `/api/graphics/{id}/take` | `GA, CA, U` | See [graphics commands](#graphics-commands). |
| `POST`   | `/api/graphics/{id}/out` | `GA, CA, U` | See [graphics commands](#graphics-commands). |
| `GET`    | `/api/graphics/{id}/on-air`                             | `GA, CA, U` | `[{ "take_id", "template_id", "name", "since", "leaving" }]`, `since` in seconds.                                                                                                                                                                                                                                                                                                          |

Template `POST` and `PUT` optionally accept an `assignment` object in the same
request: `{"role":"logo","control_layer_id":"image-id"}`. The role may be
`logo`, `ticker`, `lshape`, or `null` (unassigned). Content, binding, and the
replaced model's permanent state commit together; invalid content or an invalid
role leaves them unchanged. Omitting `assignment` preserves the established
request/response contract. Within this combined request, an omitted image
target preserves a valid existing target; explicit `null` selects the first
image. The separate assignment endpoint remains available for existing clients.

A template is
`{ "name", "group", "duration", "during_lshape": "follow" | "wait" | "skip", "layers": [...] }`.
`duration` is in seconds (default 8; 0 stays until taken out).
`during_lshape` says what happens when the template is due while an L-shape is
on air: drawn inside the shrunk picture, going on air when the L-shape ends,
or not going on air. Answers add `id`, `channel_id` and `fields`, the `{field}`
names in the order they first appear.

A layer is
`{ "id", "name", "kind": "text" | "image" | "box" | "video", "visible", "x", "y", "width", "height", "opacity", "text", "font_family", "font_size", "bold", "color", "align": "left" | "center" | "right", "image", "fill", "radius", "animation_in", "animation_out", "animation_ms", "delay_ms" }`.
Position and size are percent of the frame and must stay inside it;
`font_size` and `radius` are percent of the frame height; colors are `#rgb`,
`#rrggbb` or `#rrggbbaa`. `image` is a storage path, or a whole `{field}` to
choose the image on air. Animations are `none`, `fade` (default),
`slide_left`, `slide_right`, `slide_up`, `slide_down`, `wipe` or `scale`.
Layer ids are unique within the template; a template has at most 64 layers.
At most one layer can be `video`: it is the program picture window, not a
video file. A template with a visible video layer becomes an L-shape: its
other layers form the artwork around that window. It uses the existing
L-shape transition; per-layer animation controls do not change that transition.
Admins can author these templates at `/graphics/composer?channel={id}`;
adding `&template={template_id}` edits an existing template.

On air, a template is drawn once at the output size and blended onto the
picture before the L-shape, so with `follow` it shrinks with the picture into
the L-shape's corner. Graphics on air are not drawn over filler frames between
clips. At most 16 are on air at once; taking another takes the oldest out.
`scale` animations are drawn as a fade for now. A waiting take gives up
after an hour of L-shape. At most 16 takes may wait per channel; another returns
409. Out-all (`{}`), authored edits/deletion, and output stop/restart cancel
pending takes. Ordinary visibility controls preserve the queue, so leaving the
L-shape releases waiting graphics. Manual fields are limited to 64 values of
1000 characters each, 16 record selectors, and 256 KiB of expanded text across
visible text layers before rasterization.

```bash
curl -X POST http://127.0.0.1:8787/api/graphics/1/templates -H 'Content-Type: application/json' \
-d '{"name": "Guest", "group": "Lower thirds", "layers": [
      {"id": "bar", "kind": "box", "x": 5, "y": 75, "width": 45, "height": 12, "fill": "#f39223"},
      {"id": "name", "kind": "text", "x": 7, "y": 76, "width": 41, "height": 6, "text": "{name}"}]}' \
-H 'Authorization: Bearer <TOKEN>'
curl -X POST http://127.0.0.1:8787/api/graphics/1/take -H 'Content-Type: application/json' \
-d '{"template_id": 1, "fields": {"name": "Maria da Silva"}}' -H 'Authorization: Bearer <TOKEN>'
curl -X POST http://127.0.0.1:8787/api/graphics/1/out -H 'Content-Type: application/json' \
-d '{}' -H 'Authorization: Bearer <TOKEN>'
```

### Ad breaks (SCTE-35)

Breaks marked in the SRT or UDP output; see [Ad breaks](ad_breaks.md). The
`/scte35/{id}/{break|cancel}` route takes the channel's SCTE-35 key instead
of a login, for encoders and automation
([docs](ad_breaks.md#from-an-encoder-or-an-automation-with-the-scte-35-key)).

| Method        | Endpoint                                    | Access      | Request body                                                                                                |
| ------------- | ------------------------------------------- | ----------- | ----------------------------------------------------------------------------------------------------------- |
| `GET`         | `/api/control/{id}/scte35`                  | `GA, CA, U` | Read the state, the breaks to come and on air, and the last 20.                                             |
| `POST`        | `/api/control/{id}/scte35/break`            | `GA, CA, U` | `{ "duration": 120 }`: mark a break of that many seconds, from now plus the pre-roll.                       |
| `DELETE`      | `/api/control/{id}/scte35/break/{event_id}` | `GA, CA, U` | Call off a break that has not started.                                                                      |
| `GET`, `POST` | `/scte35/{id}/{break\|cancel}`              | SCTE-35 key | Query `key` (or header `X-Scte35-Key`), `duration` for `break`, `event` for `cancel` (else the next break). |

## Live preview

| Method | Endpoint                          | Access      | Description                                                    |
| ------ | --------------------------------- | ----------- | -------------------------------------------------------------- |
| `GET`  | `/api/preview/{id}`               | `GA, CA, U` | The channel output as a live MPEG-TS stream (`video/mp2t`).    |
| `GET`  | `/api/preview/{id}?quality=light` | `GA, CA, U` | A lighter copy: 640×360, 15 frames per second, about 0.6 Mb/s. |

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

| Method   | Endpoint                             | Access      | Description                                                                                                                                                                                                                                                                                                                                                        |
| -------- | ------------------------------------ | ----------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `GET`    | `/api/playlist/{id}?date=YYYY-MM-DD` | `GA, CA, U` | Read a playlist.                                                                                                                                                                                                                                                                                                                                                   |
| `POST`   | `/api/playlist/{id}`                 | `GA, CA, U` | Save a complete playlist JSON document.                                                                                                                                                                                                                                                                                                                            |
| `POST`   | `/api/playlist/{id}/generate/{date}` | `GA, CA, U` | Generate and save a playlist. Body is optional: `{ "paths": ["..."], "shuffle": true, "template": { ... } }`.                                                                                                                                                                                                                                                      |
| `POST`   | `/api/playlist/{id}/programs`        | `GA, CA, U` | Body: `[{ "source": "...", "title": null, "ad": false }]`. Returns, per clip, the guide programme it belongs to: `{ "key", "title", "sub_title", "is_break" }`. Consecutive clips with the same `key` form one guide entry; breaks extend the entry before them.                                                                                                   |
| `POST`   | `/api/playlist/{id}/episodes`        | `GA, CA, U` | Body: `{ "folder": "Show", "date": "YYYY-MM-DD", "after": null }`. The videos of a program folder (with season subfolders) in episode order: by the date in the file name when there is one (`show-16.10.26.mp4`, day first, or `2026-10-16`), else by natural name order. Returns `{ "episodes": [{ "source", "name", "duration" }], "last": { "source", "date" } | null, "next": 0 }`: `last`is`after`(the folder's last clip in the day being edited) or the folder's last clip in the saved playlists of the 60 days before`date`, and `next` is the index of the episode after it, starting over after the last one. |
| `DELETE` | `/api/playlist/{id}/{date}`          | `GA, CA, U` | Delete a playlist.                                                                                                                                                                                                                                                                                                                                                 |
| `GET`    | `/api/program/{id}`                  | `GA, CA, U` | Read programme items. Optional `start_after` and `start_before` query parameters accept local ISO date-times.                                                                                                                                                                                                                                                      |
| `GET`    | `/api/log/{id}`                      | `GA, CA, U` | Read a log. Optional query parameters: `date`, `timezone`, `download`.                                                                                                                                                                                                                                                                                             |
| `GET`    | `/api/system/{id}`                   | `GA, CA, U` | Read system statistics for the channel.                                                                                                                                                                                                                                                                                                                            |

Playlist items identify advertisements with `"ad": true`. The field is omitted
for regular content and defaults to `false` when absent. For compatibility,
playlist input may still contain `"category": "advertisement"`; the server
reads it as `"ad": true` and only serializes the new field. Programme responses
expose the resolved state as an `ad` boolean.

An item can carry its own `marks` (the same fields as `logo-marks` below,
without `trim`): they apply to that item only, instead of the video's marks
in the storage. They are checked when the playlist is saved; an empty `marks`
is dropped ([docs](logo.md#marks-of-one-playlist-item)).

Scheduled graphics (Composer models with fields/records; legacy logo/L-shape
action events remain supported) are under
`/api/schedule/{id}/graphics`; see [Scheduler](scheduler.md#api).

## Files

All file-management endpoints require `GA, CA, U` access to the channel.
Paths are resolved below the channel storage root; parent-directory traversal is
rejected.

| Method | Endpoint                                                                       | Request body or query                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| ------ | ------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `POST` | `/api/file/{id}/browse`                                                        | `{ "source": "", "folders_only": false }`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| `POST` | `/api/file/{id}/create-folder`                                                 | `{ "source": "folder" }`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| `POST` | `/api/file/{id}/rename`                                                        | Rename or move a file/folder: `{ "source": "old", "target": "new" }`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| `POST` | `/api/file/{id}/remove`                                                        | `{ "source": "file.mp4", "recursive": false }`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| `GET`  | `/api/file/{id}/upload?path=folder&file_name=clip.mp4&size=1234&batch_id=<ID>` | Returns already received byte ranges for a resumable upload                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| `PUT`  | `/api/file/{id}/upload?path=folder`                                            | Chunked `multipart/form-data` upload                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| `GET`  | `/api/file/{id}/conversions`                                                   | Uploaded videos queued, being or recently converted to the output format: `[{ "path", "converted", "state", "progress", "error", "updated" }]`; `state` is `queued`, `converting`, `done`, `skipped` or `failed`. See [Converting uploaded videos](uploads.md).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| `PUT`  | `/api/file/{id}/import?file=list.m3u&date=YYYY-MM-DD`                          | `multipart/form-data` with a file field                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| `POST` | `/api/file/{id}/access-token`                                                  | `{ "filename": "folder/file.mp4" }`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| `POST` | `/api/file/{id}/sheet`                                                         | `{ "path": "Folder or folder/clip.mp4" }`: the `.nfo` sheet of a program folder or clip                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| `PUT`  | `/api/file/{id}/sheet`                                                         | `{ "path", "kind": "program", "fields": { ... } }`, or `{ "path", "raw": "<tvshow>...</tvshow>" }`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| `POST` | `/api/file/{id}/logo-marks` | `{ "path": "folder/clip.mp4" }`: the marks of the video, `{ "breaks": [...], "trim": { "from": 8, "to": 1790.5 }, "graphics": [...] }`. |
| `PUT`  | `/api/file/{id}/logo-marks` | `{ "path", "breaks": [...], "graphics": [...], "trim", "insert" }`: seconds of the video. `breaks` are ad breaks signalled with SCTE-35 ([docs](ad_breaks.md#breaks-marked-inside-a-video)), at most 16, not overlapping. `graphics` are cues `{ "at", "action": "take" \| "out", "template_id", "duration" }`: at `at` seconds the template goes on air (for `duration` seconds, 0 until taken off) or off, at most 64. `trim` is the part that airs by default ([trimming videos](trim.md)). `insert: true` makes the video an ident or promo ([guide](xmltv.md#how-the-guide-is-built)). Logo and L-shape stretches of older versions are no longer read. |
| `PUT`  | `/api/file/{id}/artwork?path=Folder&kind=poster`                               | `multipart/form-data` with a `file` field; `kind` is `poster`, `backdrop` or `still` (clips)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| `POST` | `/api/file/{id}/artwork/remove`                                                | `{ "path": "Folder", "kind": "poster" }`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| `PUT`  | `/api/file/{id}/photo?path=Folder&name=Ana%20Souza`                            | `multipart/form-data` with a `file` field: the photo of a person in the credits                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |

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

| Method   | Endpoint         | Access       | Description                                                                                                                                                            |
| -------- | ---------------- | ------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `GET`    | `/api/user`      | `GA, CA, U`  | Read the current user.                                                                                                                                                 |
| `PUT`    | `/api/user/{id}` | Self or `GA` | Update a user. Only a global admin can change channel assignments, two-factor settings or the `role_id` (1 global admin, 2 channel admin, 3 operator) of another user. |
| `GET`    | `/api/user/{id}` | `GA`         | Read a user by ID.                                                                                                                                                     |
| `DELETE` | `/api/user/{id}` | `GA`         | Delete a user.                                                                                                                                                         |
| `POST`   | `/api/user`      | `GA`         | Create a user.                                                                                                                                                         |
| `GET`    | `/api/users`     | `GA`         | List users with mail, role, channels and two-factor setting.                                                                                                           |

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
routes under `/api/schedule/{id}` and the event format. Assembled programs
(`/api/schedule/{id}/programs`) are described in
[Assembled programs](/docs/programs.md#api).

## Public XMLTV guide

The program guide and its artwork are public, so IPTV players can load them
without authentication. See [XMLTV / EPG](/docs/xmltv.md) for the folder layout
and `.nfo` fields.

| Method | Endpoint                    | Description                                                                                     |
| ------ | --------------------------- | ----------------------------------------------------------------------------------------------- |
| `GET`  | `/xmltv/{id}/guide.xml`     | XMLTV guide. Optional query parameters: `days` (1-14, default 7), `lang`, `tvg_id`, `base_url`. |
| `GET`  | `/xmltv/{id}/images/{path}` | Image file below the channel storage root. Other file types return `404`.                       |

### Shape and clock layers

A `box` layer accepts `shape`: `rectangle` (default), `rounded`, `ellipse` or
`line`, optional `fill`/`stroke` colors and `stroke_width` in percent of output
height. Rounded rectangles use the existing `radius`, also percent of height.
Lines use the stroke color and width; fill is ignored. The original rectangle
without stroke retains its original renderer. Geometry and opacity remain layer
properties; circles require equal pixel width and height.

A `clock` layer shares text font, color, alignment and animation properties.
`clock_format` accepts `HH:mm:ss` (default), `HH:mm`, `hh:mm:ss A`, `DD/MM/YYYY`
or `DD/MM/YYYY HH:mm:ss`. `clock_timezone` is an optional IANA timezone; omitted,
null or empty follows the channel timezone, then the server local timezone.
Unsupported formats/timezones return HTTP 400. Text exceeding its layer box is
clipped rather than changing its position or scaling other layers. No clock is
created automatically.

`GET /api/graphics/{id}/clock` is available to GA, CA and U on the channel. It
returns `{ "unix_ms": 1794000000000, "timezone": "America/Sao_Paulo",
"offset_seconds": -10800 }`: UTC Unix milliseconds, nullable channel IANA name
and current effective UTC offset in seconds. The browser extrapolates this
reference with a monotonic clock, sampling again every 30 seconds; it does not
use the operator computer's wall clock. Authentication/channel failures retain
the usual 401/403/404 responses.

Clock artwork refreshes nominally every 250 ms outside the playout frame thread.
It follows civil time, not clip position, and does not reset take identity,
duration, visibility or entry/exit animation. This is not a frame-precision time
signal. Render errors retain the last frame and emit a throttled channel warning;
that clock may be stale until rendering recovers. The browser initially shows
`—` without a server sample and retains its last time reference on read failure.
Stopping playout stops its background refresh; starting it renders current time.

### Uploaded graphics fonts

- `GET /api/graphics/{id}/font-assets`: channel-owned metadata (`id`, `name`,
  `filename`, `family`, `size` in bytes); GA/CA/U on that channel.
- `POST /api/graphics/{id}/font-assets?filename=Review.ttf`: raw font bytes,
  `Content-Type: application/octet-stream`; GA/CA. Returns that metadata.
- `GET /api/graphics/{id}/font-assets/{font_id}`: authorized raw bytes,
  `application/octet-stream`, immutable private cache; GA/CA/U on that channel.

Accepted uploads are single-face TTF/OTF files up to 10 MiB; malformed/unsupported
fonts return 400, oversized request bodies 413. The maximum is 32 uploads per
channel (preflight 409, database limit also protects concurrent writers). Reads
retain standard 401/403/channel-not-found behavior and unknown assets return 404.

Migration `00032_graphics_fonts.sql` stores immutable font bytes and metadata in
SQLite. Normal database backup/restore includes them; no Docker image rebuild,
manual container installation or Storage file is required. The display name
comes from the font, while `family` is a unique internal `NoArFont_*` alias used
in text/clock `font_family`. Every upload, including identical bytes/filenames,
gets a new identity and counts toward the limit. It never replaces an older
composition's font. There is no font deletion endpoint in this stage; deleting
a template preserves its fonts and does not free that quota.

The engine registers these faces on startup and the browser loads the exact
same bytes for preview. Missing glyphs can still use the usual system fallback;
this is distinct from an unavailable uploaded face, which the preview reports.
A template cannot save an unknown/other-channel uploaded alias. The existing
`GET /api/graphics/{id}/fonts` continues returning installed system families;
it excludes uploaded aliases so another channel's private identities do not
appear there. Uploaded assets are listed through `font-assets`.

### Graphics data source API

GA/CA/U can `GET /api/graphics/{id}/sources` on their channel. Regular users
receive a blank `url`; GA/CA see the saved provider URL. Metadata includes `id`,
`name`, `kind`, `url`, `interval` (seconds), `records_path`, `revision`, `rows`,
`last_success`/`last_attempt` (nullable UTC Unix milliseconds) and `error`.
Uploaded CSV text and internal fetch sequence are not exposed.

GA/CA can write:

| Method | Endpoint | Body/response |
| --- | --- | --- |
| POST | `/api/graphics/{id}/sources` | Source configuration; returns the saved source and current status. |
| PUT | `/api/graphics/{id}/sources/{source_id}` | Source configuration including the current `revision`; stale edits return 409. |
| POST | `/api/graphics/{id}/sources/{source_id}/refresh` | Fetch/parse immediately; returns source status. |
| DELETE | `/api/graphics/{id}/sources/{source_id}` | 409 with dependent template names until mappings are removed; unknown source 404. |

A configuration is `{ "name": "Rates", "kind": "csv", "url": "",
"document": "symbol,price\nUSD,5.10", "interval": 30, "records_path": "" }`.
`kind` accepts `csv`, `rss` or `json`. Public sources use `url` instead of
`document`. JSON `records_path` is a JSON Pointer. Empty CSV text on update
retains the previous uploaded document. Source names need not be unique; IDs
are stable and never reused. No sources are created by installation/conversion.

A 200 save/refresh can include `error` and empty rows when a provider has no
successful result yet. It means configuration was saved, not that data arrived.
Later errors keep `rows` and `last_success` unchanged. Revisions/fetch sequences
prevent old responses overwriting newer configuration/results. Mapped column,
record and image-path validation occurs before publishing new cached rows.

A layer's optional `data_fields` maps names to
`{ "price": { "source_id": 4, "column": "price", "row": 0 } }`.
Mappings must refer to a source on the template's channel. Records are zero-based;
Composer/On air display them from 1. Image field values must name existing Storage
files, not remote URLs. Removing a placeholder prunes its mapping. A take accepts
optional `records`, for example `"records": { "4": 1 }`, to override the source's
record for that take. Nonempty `fields` override mapped values; omitted/empty
manual fields remain automatic. An active take retains that choice until taken
out/replaced; editing browser preview inputs does not rewrite an existing take.

Permanent templates require literal or fully mapped fields. Without initial data,
a new automatic take/permanent save returns a conflict before publication.
Existing successful rows survive provider failures and backend restart. Limits,
public-address rules, last-good status and operating details are in
[Data sources](graphics-data-sources.md). Earlier graphics and keyed API routes
retain their contracts; these are additive endpoints.
