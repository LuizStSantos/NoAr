# Stage c compatibility matrix

All checks in the matrix below completed successfully.

Evidence: [HTTP comparison](compat-c-api-evidence.json),
[real HLS frames and pixels](compat-c-output-evidence.json),
[desktop visual checks and limitations](compat-c-desktop-evidence.json),
[backend restart and channel-file log](compat-c-restart-evidence.json).

All integration fixtures are disposable, use synthetic media and UTC, and
remain on `claude/branch-review-opz967`. HTTP reference: the official 2.2.1
image used in stage b. Candidate: GStreamer 1.26.2 and Rust 1.99.1.

| Contract | Executed check | Evidence / scope |
| --- | --- | --- |
| Keyed `/logo`, `/lshape`, `/scene` | GET and POST, header and query key; valid actions and invalid keys | `compat-c-api-evidence.json`: exact status and body equality with 2.2.1 |
| Authenticated `/api/control/{id}/logo\|lshape\|scene` | Actions, state DTOs, unauthenticated access, missing channel/scene/assets, invalid geometry | Same 76-case comparison, including replacement and placement fields |
| Clip `logo_marks` | Clip image and position, external replacement/move priority, show/default restores clip, hide span suppresses logo | Actual HLS frames `clip-logo`, `external-logo`, `clip-logo-restored`, `clip-logo-hidden` |
| Clip L-shape | Own artwork, explicit bottom-left/60% picture, clear marks restores channel logo | Actual HLS `clip-lshape`, `clip-cleared` |
| Free model window | Artwork replacement and image-only clip inherit x=10%, y=10%, w=70%, h=50%; explicit SDK geometry remains unchanged | HLS `free-window-replacement`, `free-window-clip`; engine regression `model_art_replacement_and_clip_art_inherit_free_windows_without_changing_sdk_placement` |
| Scheduled `graphics_events` | Real future event applies its `GraphicsScene` snapshot, survives editing the named scene, reverses at end | Actual HLS `scheduled-scene`, `scheduled-reverted`; event fetched after scene edit |
| Logo transitions | Cut, fade out/in, image crossfade, channel switch duration | Engine tests `a_cut_hides_and_shows_the_logo_at_once`, `hiding_the_logo_fades_it_out_and_showing_fades_it_back`, `a_new_image_cross_fades_with_the_one_on_air`, `the_channel_sets_how_long_a_switch_takes`; aligned integration transition parity remains stage d |
| Shared model source | API, marks and scheduler operate on the same native handles; old columns stay frozen | HLS single-logo color samples plus unchanged `config_processing.logo`, `lshape`, `graphics_scenes` before/after runner; conversion/reload persistence tests in the full suite |
| Desktop output | Actual Xvfb window: show, hide, external bitmap replacement and move, reset to configured logo | `compat-c-desktop-evidence.json`; `desktop-cpu` only, not GPU/hardware or audio stability |
| XMLTV | Explicit guide logo survives on-air replacement; empty guide setting falls back to canonical model asset | Both cases on candidate and official 2.2.1; full XMLTV unit/API regressions passed |

The HTTP runner compares 76 complete responses. Its JSON lists each method,
path, expected status and body without credentials or keys. HLS evidence
records the source `.ts` hash, healthy channel sample and decoded RGB probes
for every capture. These probes verify settled graphics and single-logo
opacity; they are not aligned frame-by-frame before/after migration comparisons.

Commands executed:

```sh
cargo fmt --all
cargo clippy --no-default-features --all-targets -- --deny warnings
cargo test --no-default-features
cargo clippy -p noar --no-default-features --features desktop-cpu --all-targets -- --deny warnings
cargo test -p noar-engine --no-default-features --features desktop-cpu output::desktop
```

The main suite passed 681 tests, with zero failures and 14 ignored. The selected
desktop tests passed 33, with zero failures. Both server and desktop-cpu binaries
were built and exercised. Frontend source did not change in stage c; the four
browser regressions and pt-BR/mobile checks belong to the preceding library fix.

Desktop visual samples were healthy, but the virtual PulseAudio/ALSA device
later logged underrun/overrun warnings, an I/O error and a clip-boundary sync
error. This run does not establish desktop audio stability. GPU, physical
outputs, recording, window resizing, A/V sync and a long soak were not tested.
No menu entry was removed. Clock support remains outside this delivery.
