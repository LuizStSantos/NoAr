# GStreamer integration checkpoint

Branch: `prototype/gstreamer-continuity`. Status: **WORK IN PROGRESS**.

This branch is for development and isolated tests. It is not a release or an
on-air readiness approval. No version bump, release, production deployment or
main-branch merge is part of this checkpoint.

To build this GitHub branch on a separate Docker test server and open the real
NoAr interface, follow [Test the GStreamer branch from GitHub](gstreamer-testing.md).

## Open audio-quality investigation

An operator reports background hiss on checkout `08186625`, including playlist
files with loudness processing disabled, using AAC at 128 kbit/s. This report
is **unresolved**; the continuity matrix below is not an audio-quality verdict.

A focused native-to-Rust regression exposed a separate sample-clock defect:
flooring a nanosecond timestamp mapped sample 49,024 to 49,023. Conversion now
rounds to the nearest sample. The regression fails before this change and
passes afterward. The input still uses `audioconvert`, `audioresample` and
explicit stereo 48 kHz caps; the managed `ar` restriction remains intentional.

Controlled 44.1/48 kHz AAC stereo files with silence and distinct left/right
tones produce 48 kHz output. Their measured middle windows match the decoded
PCM from an independent native GStreamer reference exactly, and their silence
windows contain no added noise. These static-file measurements did not show
an audible-noise improvement from the clock fix. The real noisy source/output
pair is needed to identify the reported hiss; do not call it fixed yet.

The follow-up passes 590 Rust tests, formatting, both Clippy configurations,
the embedded frontend build and a fresh actual-NoAr SRT/SRT Scheduler smoke.
The seven-output matrix below belongs to its recorded source/binary, not this
new rounding change. See the [audio diagnostic evidence](../qa/gstreamer/evidence/audio-quality-summary.json).

## Latest protocol validation

**PASS:** the complete bounded matrix at source commit `c64932e8` passes all
seven selected pairings in normal operation and again with a four-second
publisher interruption: **14/14 cases**. All runs use the same compiled binary,
three automatically admitted Scheduler events, decoded synthetic A/B/live
markers, Clock/Continue return modes and independent receiver observations.

| Output | Scheduler live input | Normal | Publisher interruption |
| --- | --- | --- | --- |
| UDP normal | SRT | PASS | PASS |
| UDP SCTE-35 | UDP | PASS | PASS |
| SRT normal | RTMP | PASS | PASS |
| SRT SCTE-35 | SRT | PASS | PASS |
| HLS normal | HLS | PASS | PASS |
| HLS SCTE-35 | SRT | PASS | PASS |
| RTMP normal | RTMP | PASS | PASS |

See the [sanitized protocol evidence](../qa/gstreamer/evidence/protocol-matrix-summary.json)
for source/binary hashes, all gate statuses, timing metrics and fault times,
and [the harness instructions](../qa/gstreamer/INTEGRATED.md) to reproduce it.
This covers seven selected pairings, not all 28 combinations. The publisher
pause can be partly absorbed by input buffering; it does not prove every
Scheduler retry/waiting policy. HLS receiver progress is evaluated against its
segment target duration plus one second; UDP/SRT/RTMP retain the 750 ms gate.

SCTE-35 checks independently decode original MPEG-TS for UDP/direct SRT and
both native HLS renditions. They verify CRCs, repeated out/in cues, pre-roll,
splice/IDR boundaries, cancellation and HLS date ranges/legacy markers. Stock
MediaMTX strips SCTE-35, so direct SRT capture deliberately avoids that relay.
Downstream ad insertion and third-party cue interoperability remain untested;
native HLS SCTE-35 remains experimental.

### Fixes included in the tested binary

- Incoming audio uses native `audiorate` to preserve samples through FLV/PCR
  timestamp jitter. The decoder read loop waits within its existing timeout
  for separately decoded tracks before choosing timestamp order.
- Encoded output drains program loudness lookahead before source skips, keeping
  already-counted media on the persistent output timeline.
- RTMP reads complete cached FLV/codec headers at its first usable keyframe.
  SRT shutdown drains AppSrc and bounds its wait for final native wire activity.
- The Scheduler explicitly preserves live context for HTTP events through
  preloading and reopening. Ordinary HTTP VOD retains file behavior.
- Playback uses `uridecodebin3`/`hlsdemux2` and segment-to-stream-time conversion.
  A scoped guard on adaptive HLS's internal compressed TS pads unwraps an
  observed extra 33-bit timestamp cycle that otherwise inserted a 26-hour gap.

The first matrix at `bf278673` failed HLS admission/audio coverage; those
failures drove the HLS guard and read-order fixes. The fresh complete results
above supersede those earlier failures for this binary.

### Current local checks and limits

- **PASS:** 590 Rust tests with no default features; 13 existing opt-in tests
  ignored. Formatting, no-default-features Clippy, static all-features Clippy
  and the embedded frontend build pass. All-features analysis uses
  `ORT_SKIP_DOWNLOAD=1`, without optional ONNX linking or runtime verification.
- **PASS:** 58 AI toolkit tests, 13 Python QA tests, required native factories,
  Dockerfile static checks and the isolated Compose configuration.
- **PASS (earlier scoped browser evidence):** 14 Chromium checks cover saving
  and reloading four live protocols, HLS SCTE-35 settings and navigation.
- **NOT_AVAILABLE:** complete Docker image build in this executor, due to its
  32 GB filesystem and Docker `vfs` copy-layer exhaustion; optional ONNX binary
  download, denied by the proxy. No full image build is claimed.
- **NOT_RUN:** extended soak, GPU/multicast, browser signal-test validation in
  this matrix and downstream ad insertion. Recording presence is checked;
  current recording decode/retention needs separate evidence.

These are bounded development results, not release or on-air readiness
approval. Earlier baseline evidence below applies only to its recorded runs.

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

## Historical baseline before the protocol expansion

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

1. Reproduce the validated normal and source-interruption matrix on the
   isolated company test server; record its commit, image and native versions.
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
