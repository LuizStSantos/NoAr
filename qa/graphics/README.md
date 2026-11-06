# Converted graphics compatibility checks

These runners are destructive **only to an owned synthetic staging fixture**.
They require both `NOAR_E2E_STAGING=1` and `NOAR_E2E_DISPOSABLE=1`, loopback
servers and a `.qa-graphics-owned` marker inside `--work-root`. Never point
them at an operator's database, storage or output destination.

`compatibility.py` compares HTTP statuses and complete JSON/text bodies between
the candidate on port 8787 and the official 2.2.1 image on port 18787. Both
must already contain a disposable channel/account. `--auth-file` contains its
`username` and `password`; tokens and external-control keys stay out of evidence.
The baseline database must be a separate pre-00028 copy. When its storage
lives in a different owned fixture, pass `--baseline-work-root` for that root. The runner seeds
synthetic playlists, changes channel settings, renews the graphics key and
checks XMLTV's explicit and configured-model fallback icons.

`output.py` uses the candidate and its local HLS files. It changes clip marks,
restarts playout, edits the bound L-shape window, creates/deletes a timed event
and edits a named scene after taking the event's action snapshot. Additional
logo/L-shape art layers prove image overrides preserve the composition; a logo
move also checks the translated secondary layer. It waits for
ten newly published HLS segments, decodes an actually completed `.ts` segment
with FFmpeg and checks pixel samples before saving each JPEG. It checks health
and verifies the frozen legacy logo/L-shape/scene columns did not change.
`--free-window-only` runs only the free-window captures; it still resets the
fixture's transient graphics state and marks. `--composite-only` isolates the
additional-layer cases with the same reset behavior.

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

Do not run browser/model writers or these runners concurrently: they intentionally control the same
candidate channel. HTTP equality and sampled settled HLS frames do not prove
aligned frame-by-frame migration parity, hardware output, A/V sync or a soak.

## Paired desktop and migration parity

`desktop.py` runs inside the GStreamer container with equivalent pre-created
`desktop-main/noar.db` and `desktop-branch/noar.db` fixtures. It starts fresh
Xvfb/PulseAudio processes for each 240-second run, uses the same media, fake
clock and action times, and saves logs, health and warning counts. Its ALSA
configuration path is the owned stage-c fixture from the original comparison.

For `parity.py`, read [the fixed criteria](parity-criteria.md) first. NumPy,
Pillow, FFmpeg and Docker are required on the controller. The runtime container
must mount the work root at the same absolute path and publish port 8787 to
loopback. No other process may own that test port.

Create isolated source worktrees at the reference/candidate commits and apply
`preencode.patch` to each. It adds only a writer at the encoder boundary;
never apply it to shipped source. Build each no-default binary in the same
GStreamer 1.26 environment. When reusing Cargo's target between worktrees,
**clean `noar` and `noar-engine` before switching**, then copy the binary to a
distinct path. Dependency artifacts may stay cached. Save commits, build logs,
patch hash and binary hashes.

```sh
export NOAR_E2E_STAGING=1 NOAR_E2E_DISPOSABLE=1
python3 qa/graphics/parity.py prepare \
  --work-root /tmp/owned-parity --source-database /tmp/disposable-221.db
python3 qa/graphics/parity.py run --work-root /tmp/owned-parity \
  --name main --binary /tmp/main-capture --auth-file /tmp/disposable-auth.json
python3 qa/graphics/parity.py run --work-root /tmp/owned-parity \
  --name branch --binary /tmp/branch-capture --auth-file /tmp/disposable-auth.json
python3 qa/graphics/parity.py compare --work-root /tmp/owned-parity \
  --screenshots /tmp/owned-parity/captures
```

`prepare` requires a new directory and creates the deterministic media, marks
and byte-identical database copies. `run` clears only that owned root's output
and logs, starts the actual application, captures the complete interval, then
stops and flushes output. `compare` requires all 1000 raw PTS files and all 1000
HLS timestamps; failures are reported without selecting matching subsets.
Archives retain all common raw frames and both HLS segment sets; JPEG/PNG pairs
illustrate selected frames. These runners must run sequentially.
