# Converted graphics compatibility checks

These runners are destructive **only to an owned synthetic staging fixture**.
They require both `NOAR_E2E_STAGING=1` and `NOAR_E2E_DISPOSABLE=1`, loopback
servers and a `.qa-graphics-owned` marker inside `--work-root`. Never point
them at an operator's database, storage or output destination.

`compatibility.py` compares HTTP statuses and complete JSON/text bodies between
the candidate on port 8787 and the official 2.2.1 image on port 18787. Both
must already contain a disposable channel/account. `--auth-file` contains its
`username` and `password`; tokens and external-control keys stay out of evidence.
The baseline database must be a separate pre-00028 copy. The runner seeds
synthetic playlists, changes channel settings, renews the graphics key and
checks XMLTV's explicit and configured-model fallback icons.

`output.py` uses the candidate and its local HLS files. It changes clip marks,
restarts playout, edits the bound L-shape window, creates/deletes a timed event
and edits a named scene after taking the event's action snapshot. It waits for
ten newly published HLS segments, decodes an actually completed `.ts` segment
with FFmpeg and checks pixel samples before saving each JPEG. It checks health
and verifies the frozen legacy logo/L-shape/scene columns did not change.
`--free-window-only` runs only the free-window captures; it still resets the
fixture's transient graphics state and marks.

Required synthetic storage fixtures, all without embedded graphics:

| File | Content |
| --- | --- |
| `compatibility.mp4` | 180 seconds, 640×360, 25 fps, solid `#505050`, synthetic AAC audio |
| `channel-logo.png` | 80×48, solid `#e0a020` |
| `replacement-logo.png` | 80×48, solid `#2288ff` |
| `clip-logo.png` | 80×48, solid `#b030d0` |
| `guide-logo.png` | 80×48, white |
| `channel-art.png` | 640×360, solid `#183878` |
| `replacement-art.png` | 640×360, solid `#702838` |
| `clip-art.png` | 640×360, solid `#285018` |

The candidate fixture contains a converted ticker binding; output checks hide
it to isolate logo pixels. The HLS playlist is `public/live/compat.m3u8` under
the work root. Python 3.10+ and `ffmpeg` are required.

```sh
export NOAR_E2E_STAGING=1 NOAR_E2E_DISPOSABLE=1
python3 qa/graphics/compatibility.py \
  --work-root /tmp/owned-graphics \
  --auth-file /tmp/owned-graphics/auth.json \
  --evidence /tmp/owned-graphics/api-comparison.json
python3 qa/graphics/output.py \
  --work-root /tmp/owned-graphics \
  --screenshots /tmp/owned-graphics/captures
```

Do not run the two runners concurrently: they intentionally control the same
candidate channel. HTTP equality and sampled settled HLS frames do not prove
aligned frame-by-frame migration parity, hardware output, A/V sync or a soak.
