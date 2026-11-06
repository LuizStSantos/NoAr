# Stage d criteria, fixed before evaluating output

Reference: `origin/main` at `dd4529e2935b07d7a5bf8deb764421e4d201fc41`.
Its `backend/`, `Cargo.toml` and `Cargo.lock` are identical to release revision
`d4a1a0a8a41f39b90de23844910998431fd42510`; documentation/QA trees differ.
Candidate starts at `ee2ad7cbc4ea98d48c26da755a99302770623ad7`.

Both actual applications open independent copies of the same disposable 2.2.1
database at sqlx 00027. Candidate startup runs the real migrations/conversion.
The same storage files, output geometry (640×360, 25 fps), codec/options,
permissions and GStreamer 1.26.2 are used. Folder mode with one source starts
the deterministic 40-second clip at source PTS zero on both sides, independent
of database-conversion/startup wall time. There is no external transition
clock injection or best-fit frame shifting.

The source has a visible binary frame number and a grid to expose position,
scale, missing frames and timing differences. Clip marks drive these exact
source-time transitions, using the real application/engine marks path:

- Logo hide gap: 8–10 seconds; configured fade duration 1 second gives
  fade-out during 7–8 seconds and fade-in during 10–11 seconds.
- L-shape marked interval: 14–20 seconds, including the engine's entire
  logo-suppression/picture transition on entry and exit (`LSHAPE_SWITCH_SECONDS`).
- Remaining frames provide settled full-frame, hidden-logo and L-shape states.

**Pre-encode:** apply the identical `preencode.patch` only to isolated test
worktrees, never the shipped application. Capture each accepted frame at the
input to `packed_video_buffer`, after composition/PTS assignment, duplicate-PTS
filter and pacing, before encoding. Store tightly packed YUV420P (Y then U/V;
visible row widths only, excluding aligned stride padding), keyed by the
original engine PTS. At 640×360 each frame is 345600 bytes. Require exactly
PTS 0–999 on both sides and byte-for-byte equality for every plane of every
frame. Publish first divergence and per-region metrics on failure; do not
discard missing, duplicate or divergent test frames.

**HLS:** decode the actual completed `.ts` segments, including the flushed last
segment. Align by MPEG-TS frame PTS after subtracting each stream's constant
container origin; require all 1000 test timestamps on the 40 ms grid, without
duplicates, gaps, spatial translation, rescaling or per-transition time shifts.
Verify the source frame-number marker at the beginning/end. Post-roll for
encoder flushing is outside the first 40-second test interval.

Compare decoded 8-bit RGB in logo `(548,12,80,48)`, whole-frame L-shape and
ticker `(0,300,640,60)` regions on **every** test frame. Fixed compression
tolerance per region/frame: mean absolute error ≤0.75, 99.9th percentile ≤8,
maximum channel error ≤32 (range 0–255). This allows small lossy-codec rounding,
not movement: exact pre-encode equality is an additional mandatory gate that
forbids any geometry or transition-timing difference. Do not relax thresholds
after seeing results. Save matched frames, amplified difference images,
per-frame metrics, input/segment hashes, `/health` samples and channel logs.

**Ticker:** 2.2.1's migration 00008 removed persisted text configuration; the
reference application has no channel ticker to import. Verify that region
remains unchanged and that conversion creates an empty, disabled ticker.
Do not claim historical parity for a newly authored nonempty ticker.

Stage d remains pending until both exact and HLS gates pass. Stage e menu and
editor changes must wait for that result.
