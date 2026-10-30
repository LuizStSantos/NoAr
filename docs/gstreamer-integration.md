# GStreamer integration checkpoint

Branch: `prototype/gstreamer-continuity`. Status: **WORK IN PROGRESS**.

This branch is for development and isolated tests. It is not a release or an
on-air readiness approval. No version bump, release, production deployment or
main-branch merge is part of this checkpoint.

To build this GitHub branch on a separate Docker test server and open the real
NoAr interface, follow [Test the GStreamer branch from GitHub](gstreamer-testing.md).

## Protocol matrix development checkpoint

The current follow-up adds RTMP/HLS/UDP protocol observers, an independent
SCTE-35 decoder, an original-byte SRT receiver, and selectable external live
inputs (RTMP, SRT, HLS, UDP) with optional automatic Scheduler admission.
The live editor now exposes UDP explicitly. Fixes cover native RTMP FLV tag
boundaries/header availability, HLS buffer-list health, SCTE-35 command length
and PMT stuffing, and initial real audio preservation.

Native HLS SCTE-35 remains **experimental and NOT_READY** until the latest
protocol and source-loss matrix finishes. Subsequent isolated runs passed its
in-band cues, independent decoding, cancellation, shared date-range IDs,
legacy tags and segment/IDR boundaries in both renditions. These bounded runs
do not establish ad-platform interoperability or long-run readiness.

### Current fixes and evidence

- **PASS (individual bounded runs before the latest HLS/RTMP fixes):** UDP
  normal with SRT input, UDP SCTE-35 with UDP input, SRT normal with RTMP input,
  SRT SCTE-35 with SRT input, and HLS SCTE-35 with SRT input. Each run observes
  three automatically admitted Scheduler events and decoded A/B/live markers.
- Incoming audio now passes through native `audiorate`. A regression test checks
  exact PCM preservation under FLV/PCR timestamp jitter; subsequent RTMP-input
  media runs passed the previously failing decoded audio gates.
- SRT capture now uses original payload bytes for both modes. Output shutdown
  drains AppSrc and bounds the wait for native SRT wire activity to settle;
  a native loopback regression test verifies final-message delivery.
- Playback uses `uridecodebin3`/`hlsdemux2` and converts sample timestamps through
  their segments. The Scheduler explicitly preserves live context for HTTP
  sources through preload handover and reopening; ordinary HTTP VOD retains
  file behavior. The first retest restored all three live audio/video transitions without a
  muxer restart; timestamp gaps remained, so its overall status was **FAIL**.
- The remaining HLS/RTMP timestamp gaps were traced to program loudness
  lookahead being discarded on source skips. Encoded outputs now drain this
  already-counted media; outputs that can reset retain their discard behavior.
  A regression test verifies both paths. A fresh automatic RTMP-input/RTMP-output
  run passes all gates: video gap 40 ms, audio gap 21.5 ms, receiver progress
  503.4 ms, three live transitions and one persistent publisher/reader.
- RTMP caches its complete FLV preamble at the first usable keyframe. A split
  metadata/config regression test passes. **FAIL:** the latest bounded RTMP
  output run before the lookahead fix had timestamp gaps above the harness
  limits; the subsequent retest above passes.
- **PASS:** 14 earlier Chromium checks cover live-source save/reload for four
  protocols, HLS SCTE-35 settings and navigation. They do not verify streaming.
- **PASS:** the isolated Docker test Compose configuration and Dockerfile static
  build check. **NOT_AVAILABLE:** full image build, due to executor disk limits;
  optional ONNX binary download, denied by the configured proxy.

The reusable `integrated_matrix.py` runs seven selected input/output pairings,
not all 28 combinations. Fresh complete-matrix and recovery results are pending.
See [the current harness instructions](../qa/gstreamer/INTEGRATED.md).
Earlier checkpoint evidence below applies only to those recorded runs.
The overall protocol expansion remains **NOT_READY**.

## Implementation

- GStreamer is the single media engine. Native input decoding, normalization,
  output encoding, muxing, transports, preview and recording replace the
  previous FFmpeg playback implementation.
- The Rust compositor and audio processing retain FFmpeg frame structures as a
  memory interface. FFmpeg tools remain useful for fixtures and independent
  packet/decoder inspection.
- Live events read external URLs, including MediaMTX SRT read URLs. Embedded
  RTMP/SRT input servers and their frontend settings have been removed.
- Historical database tables and rows remain; no released migration is changed.
- Existing Docker and validation workflows install GStreamer 1.26 libraries
  and plugins. The corrected SRT 1.5.5 runtime remains in use.
- Codex and Claude use the shared `noar-gstreamer` reference. Start with
  [GStreamer reference navigation](ai/tech-references/gstreamer.md),
  [developer setup](developer.md) and [external live sources](live_ingest.md).

## Validation at the checkpoint

All media tests use disposable databases, synthetic files and a local MediaMTX.
The standalone Python prototype is separate from integrated NoAr evidence.

| Check | Status | Evidence and limits |
| --- | --- | --- |
| AI toolkit | PASS | 58 unit tests; new canonical reference and Codex adapter resolve through the repository map. |
| Frontend build and lint | PASS | `npm run build`; ESLint for `frontend/src`. |
| Rust workspace tests | PASS | 573 tests passed with no default features; 13 existing opt-in tests remained ignored. Includes regression tests for missing live tracks and unchanged pacing rates. |
| Rust lint | PASS | No-default-features Clippy and static all-features Clippy with warnings denied. All-features analysis uses `ORT_SKIP_DOWNLOAD=1`; optional ONNX runtime is not validated. |
| Latest file preload/seek regression tests | PASS | Two focused tests pass after opening local files in PAUSED. |
| Recording dimensions | PASS | Desktop CPU-feature test encodes 320x180 input into a 160x90 Matroska recording and probes its dimensions and audio. No display or sound hardware is exercised. |
| SVT-AV1 option mapping | PASS | Native property test verifies persisted quality and kbit/s units. |
| Formatting and embedded frontend build | PASS | Latest `cargo fmt --all -- --check` and no-default-features embedded frontend build. |
| Integrated normal transitions | PASS | All 13 smoke gates pass in two 60-second runs after bounding live A/V separation. Three Clock/Continue admissions per run, decoded A/B/live transitions, one publisher and reader, zero reported SRT drops and no DTS regressions. Maximum receiver progress gaps 206.7 ms and 204.3 ms. |
| Integrated source-loss recovery | PASS | All 13 smoke gates pass after a four-second synthetic publisher interruption. Later live admissions return; output sessions persist. Maximum receiver progress gap 406.4 ms in the recorded 60-second run. |
| Recording decode | PASS | All six Matroska segments from those normal/recovery runs decode both audio and video with FFmpeg `-xerror`. Retention and power-loss recovery are separate checks. |
| Dockerfile build checks | PASS | `docker build --check .` reports no warnings. |
| Complete Docker image | NOT_AVAILABLE | This executor's 32 GB filesystem and Docker `vfs` driver exhausted disk while copying backend layers, before the Rust image build. Task-local build cache was removed; do not treat the image as built. |
| Optional ONNX runtime | NOT_AVAILABLE | The configured network proxy denies the ONNX binary CDN with HTTP 403. `ORT_SKIP_DOWNLOAD=1` permits static Clippy only, not runtime/link verification. |
| Scoped Chromium interaction | PASS | Nine checks cover anonymous redirect, password visibility, real browser login, authenticated output/audio/recording/schedule/player navigation and no page errors. The built frontend uses a test-only loopback proxy to the debug backend. Removed input-server settings links are absent. |
| Extended soak, GPU and complete scheduler UI flow | NOT_RUN | Bounded smoke runs and page navigation do not establish long-run stability, hardware behavior, schedule form persistence or automatic wall-time admission. |

The previous checkpoint exposed a trimmed-file stall and receiver gaps above
750 ms. Follow-up fixes open local files in PAUSED, retain the pacing anchor
when the rate is unchanged, and bound live track separation when only audio or
video continues arriving. Missing video holds the last composited picture;
missing audio receives silence after a 100 ms grace. Late source samples are
trimmed against the timeline already emitted. Three new regression tests cover
the interleaving behavior. Do not infer a 24/7 guarantee from the bounded runs.

The reusable [actual-NoAr harness](../qa/gstreamer/INTEGRATED.md) and a
[sanitized evidence summary](../qa/gstreamer/evidence/integration-summary.json)
are stored on this branch. Raw captures, credentials and temporary databases
remain excluded from Git. The summary records source/binary hashes and the
limits of each check.

## Next validation

1. Repeat the normal and source-loss tests against the latest actual NoAr
   binary, measuring receiver progress independently of API polling.
2. Run an extended soak and repeat the strict 750 ms receiver gate under
   representative channel load. Investigate any regression before a readiness
   claim.
3. Re-run the recorded Rust and frontend checks after further code changes.
4. Build the existing root Dockerfile on an isolated machine with sufficient
   disk and a suitable Docker storage driver. Do not publish an image yet.
5. Exercise the changed frontend and scheduler controls in a disposable
   browser session, and extend HLS/SCTE-35/recording checks as appropriate.

Do not restore the removed input servers or add a second playback backend to
work around a failing test. Keep new findings and fixes on this development
branch until the relevant gates pass.

The original code checkpoint is `53999ae4`; `18e4af51` preserved its status when
the executor became unavailable. The workspace was subsequently recovered and
the follow-up evidence above supersedes those earlier pending results.
