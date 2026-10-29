# GStreamer integration checkpoint

Branch: `prototype/gstreamer-continuity`. Status: **WORK IN PROGRESS**.

This branch is for development and isolated tests. It is not a release or an
on-air readiness approval. No version bump, release, production deployment or
main-branch merge is part of this checkpoint.

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
| Rust workspace tests before the latest follow-up fixes | PASS | 567 tests passed; existing opt-in tests remained ignored. |
| Rust lint before the latest follow-up fixes | PASS | No-default-features Clippy and static all-features Clippy with warnings denied. |
| Latest file preload/seek regression tests | PASS | Two focused tests pass after opening local files in PAUSED. |
| Recording dimensions | PASS | Desktop CPU-feature test encodes 320x180 input into a 160x90 Matroska recording and probes its dimensions and audio. No display or sound hardware is exercised. |
| SVT-AV1 option mapping | PASS | Native property test verifies persisted quality and kbit/s units. |
| Formatting and embedded frontend build | PASS | Latest `cargo fmt --all -- --check` and no-default-features embedded frontend build. |
| Integrated normal transitions, earlier build | PASS | Three triggered Clock/Continue events; decoded A/B/live transitions; one publisher and one reader; zero reported SRT drops; no DTS regressions; maximum observed receiver progress gap 576 ms over a 60-second run. |
| Integrated source-loss continuity, earlier build | FAIL | Fault injection preserved SRT sessions and the source returned, but one run stalled about 10.5 seconds on the return to a trimmed file. Zero packet drops alone does not establish continuity. |
| Latest integrated source-loss smoke run | PASS | All 13 smoke checks passed on the checkpoint binary: three live entries, A/B/live decoded markers, one publisher and one reader, zero reported SRT drops, monotonic timestamps, captured audio duration 59.8 seconds and maximum receiver progress gap 410.5 ms. |
| Repeated source-loss stability | FAIL | An earlier run of the same follow-up changes observed an 811 ms receiver progress gap, above the 750 ms smoke threshold. The latest PASS does not establish repeatability or long-soak readiness. |
| Latest normal-run retry | NOT_AVAILABLE | The executor became unavailable during the run; its final result was not retrieved. Do not infer PASS. |
| Complete Rust suite/lints after the latest follow-up fixes | NOT_RUN | Repeat after the latest source-state, clock and option changes. |
| Dockerfile build checks | PASS | `docker build --check .` reports no warnings. |
| Complete Docker image | NOT_AVAILABLE | This executor's 32 GB filesystem and Docker `vfs` driver exhausted disk while copying backend layers, before the Rust image build. Task-local build cache was removed; do not treat the image as built. |
| Optional ONNX runtime | NOT_AVAILABLE | The configured network proxy denies the ONNX binary CDN with HTTP 403. `ORT_SKIP_DOWNLOAD=1` permits static Clippy only, not runtime/link verification. |
| Authenticated browser interaction and GPU playback | NOT_RUN | Frontend compilation and API tests do not replace browser or hardware evidence. |

## Next validation

1. Repeat the normal and source-loss tests against the latest actual NoAr
   binary, measuring receiver progress independently of API polling.
2. Verify the trimmed-file stall does not recur and investigate the occasional
   receiver gap above 750 ms before any readiness claim.
3. Repeat the Rust workspace tests, no-default-features Clippy, all-features
   Clippy where native dependencies permit it, and the embedded frontend build.
4. Build the existing root Dockerfile on an isolated machine with sufficient
   disk and a suitable Docker storage driver. Do not publish an image yet.
5. Exercise the changed frontend and scheduler controls in a disposable
   browser session, and extend HLS/SCTE-35/recording checks as appropriate.

Do not restore the removed input servers or add a second playback backend to
work around a failing test. Keep new findings and fixes on this development
branch until the relevant gates pass.

## Executor interruption after the checkpoint

The complete code and AI-reference checkpoint was pushed as `53999ae4` before
execution access was lost. The later reusable QA files
`qa/gstreamer/integrated_smoke.py` and `qa/gstreamer/INTEGRATED.md` were created
locally but were not committed or pushed before the executor became unavailable.
The recovery run using that local harness passed all 13 smoke checks; its
normal-run retry had no retrievable final result. The harness itself still needs
review and publication after workspace recovery.

Resume by fetching this branch and inspecting local changes. Preserve those QA
files while bringing the local branch forward: this status document was updated
through the GitHub connector after execution access failed. Re-run the remaining
checks instead of inferring their outcome from earlier builds.
