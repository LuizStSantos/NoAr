# Project organization: verification handoff

Branch: `claude/branch-review-opz967`. Starting revision: `6315dc62`.

The organization pass now covers application configuration, scheduled playback,
XMLTV, native output policy/desktop recording, and frontend playlist, Scheduler,
Storage, reports and output discovery. The [maintenance guide](../maintenance.md)
records source ownership, sensitive flows and extension recipes.

Review **27 actual captures** in the [28-page PDF](review.pdf),
[HTML gallery](review.html) or [image index](screenshot-index.json).
All images are at most 1440 px wide; five complementary full-page phone shots
show the lower controls in addition to the original viewport captures.

## Source changes and boundaries

- Application configuration retains its facade. Output/recording DTOs and policy
  have dedicated modules. `apply_config_update` owns validation, the existing
  transaction and runtime reconciliation; the HTTP adapter retains authorization,
  extraction and response serialization.
- Scheduler policy, reservation/generation and resume coordination remain in the
  parent; `schedule/playback.rs` owns scheduled clip/live execution. XMLTV
  programme assembly and wire serialization now have separate owners.
- Engine output configuration retains existing imports/exports and delegates
  codecs, HLS parsing, metadata validation and recording settings. Desktop's
  bounded recording worker has a dedicated module with the existing queue,
  native frame handling and shutdown behavior.
- Playlist row/group projection and whole-day persistence have separate owners.
  Scheduler panels receive typed inputs and emit commands. Storage delegates
  image resources/viewer selection and file actions/dialogs; the page retains
  folder/channel/selection. Report requests/polling and device discovery moved
  to their domain composables.
- Architecture CI checks now also reject direct store/auth-fetch/network edges
  in the seven extracted presentation panels. Existing app/engine/SQL dependency
  guards remain. All nine guard tests passed.

The [source inventory](source-inventory.json) records paths, sizes and hashes.
All **34 migration files**, the public route registry, engine root exports and
existing generated playout types match the starting revision byte for byte.
The [verification ledger](verification.json) identifies the normal and desktop
binary hashes used for these proofs.

Authentication/session, DB handles, upload/access policy, audio DSP/native input
and deployment/release ownership were examined through the source inventory and
representative contracts. Those areas retain their existing implementations.
Coverage here is a cross-project organization pass and scoped review; an
independent line-by-line audit of every file was **NOT_RUN**. The earlier
Graphics organization has its own [handoff](../graphics-handoff/README.md).

## Executed checks

All runtime work used owned disposable SQLite copies, synthetic media and
GStreamer **1.26.2**. The normal backend has no pre-encode observation hook.

| Check | Result | Evidence |
| --- | --- | --- |
| `cargo fmt --all`; exact no-default/all-target Clippy with deny warnings | PASS | [Rust log](rust-verification-log.txt) |
| `cargo test --no-default-features` | **714 passed / 14 ignored** | [Rust log](rust-verification-log.txt) |
| Normal backend build; desktop-CPU all-target compilation | PASS | [Rust log](rust-verification-log.txt) |
| Desktop engine tests with `desktop-cpu,tokio` | **37 passed** | [Desktop test/build log](desktop-verification-log.txt) |
| Vue types, whole-frontend ESLint, changed-file Prettier, QA TypeScript | PASS | [Ledger](verification.json), [format log](frontend-prettier-log.txt) |
| Frontend production build | PASS | [Build log](frontend-production-log.txt) |
| Architecture guards and their tests | PASS / **9 tests** | [Findings](architecture-evidence.json), [test log](architecture-tests-log.txt) |
| Fresh-install authenticated browser suite | **36 passed** | [Browser log](browser-test-log.txt) |
| Actual pt-BR desktop/phone workflows | PASS | [UI actions/measurements](ui-maintenance-evidence.json) |
| Saved playlists/events across actual backend restart | PASS, exact definitions | [Restart evidence](restart-evidence.json) |
| Released 2.2.1 legacy graphics API/XMLTV comparison | PASS, **76 exact responses + 2 icon cases** | [Evidence](compatibility-evidence.json), [log](compatibility-test-log.txt) |
| Actual HLS scheduled interruption and programme return | PASS | [Metrics](hls-evidence.json), [health](hls-health.json), [channel log](hls-channel-log.txt) |
| Actual virtual desktop and dedicated AV recording | PASS | [Recording evidence](project-desktop-recording-evidence.json) |

Browser coverage includes playlist edit/undo/save/conflict/load-failure protection,
Scheduler CRUD/focus/graphics selection, Graphics Composer/models/fonts/sources,
Storage original-byte downloads and create/cancel/rename/move/delete dialogs,
and report loading-error recovery. The first run passed 35 tests and failed the
new folder test's assumption that raw URL reload retained folder selection.
Source verification confirmed the existing `?path=` bookmark contract; the test
was corrected, then all 36 passed in a fresh installation.

The additional manual browser proof covers native material/event drag with
pointer-derived times, zoom 24/6/1 h, horizontal wheel scrolling, playlist
programme/flat views, day-map keyboard navigation, confirmation cancellation,
successful copy and confirmed discard. The current UI artifact includes 15
workflow actions plus five read-only full-page phone captures: **20 recorded
actions**, **22 screenshots**, no page errors or document horizontal overflow.

## Actual output evidence

HLS contains H.264 **640×360/25 fps** and AAC **48 kHz stereo**. The completed
synthetic interruption is visible in **512 decoded frames**: 239 blue programme
frames before, **75 red event frames** for the three-second source, and 198 blue
programme frames after return. Active/done schedule state and healthy output
were sampled during playback; the scoped channel log has **zero ERROR entries**.
[Before](output-hls-before.jpg), [event](output-hls-event.jpg),
[after](output-hls-after.jpg), [test log](hls-test-log.txt).
The [analysis manifest](hls-analysis.m3u8) freezes the stopped writer's segment
list with `ENDLIST`; TS bytes remain unchanged and their SHA-256 values are in
the metrics. Relative TS URLs belong to the private disposable fixture.

The reusable [runtime recipe](../../qa/maintenance/README.md) uses a complete
30-second playlist with a fresh playback clock. During harness preparation,
finite HLS analysis, playlist duration/Continue semantics and fixture navigation
state were corrected; no application source change was needed for those setup
issues. These output checks demonstrate actual execution and return. The full
pre-encode/main-versus-candidate parity fixture was **NOT_RUN** in this pass;
its previous Graphics result retains its original scope.

Desktop CPU ran with Xvfb and PulseAudio virtual audio. Its actual Matroska
recording is **13.76 s**, H.264 **640×360/25 fps** plus AAC **48 kHz stereo**;
measured audio RMS is **0.12476**, tone **440.018 Hz**. All 13 health samples were
healthy; there were zero NoAr channel errors or audio/sync/media warnings.
[Desktop](project-desktop-recording.jpg),
[recorded frame](project-desktop-recorded-frame.jpg),
[health](project-desktop-health.json),
[channel log](project-desktop-channel-log.txt),
[server log](project-desktop-server-log.txt).
The [virtual PulseAudio log](project-desktop-pulse-log.txt) retains optional
system/session D-Bus startup diagnostics. Disposable login's 2FA warning is also
retained; it is separate from media-stream warnings. The recording proof's
processes, port and sockets were closed and checked.

## Remaining verification limits

**NOT_RUN:** Firefox/Safari, physical audio/video devices, GPU output/encoders,
long-duration soak, same-run desktop comparison with 2.2.1, the full raw-frame
parity repeat and independent full-source audit. Native live-source drag was
NOT_RUN because the fixture contains no live inputs. Continuous scrolling
text/feed authoring remains outside this refactor and pending in the Graphics
roadmap. No merge, release or main-branch push belongs to this handoff.
