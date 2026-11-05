# Decoded-picture validation — NoAr 2.2.1

Source baseline: `d4a1a0a8a41f39b90de23844910998431fd42510`.
Changes are confined to test tooling, CI and documentation. No production
engine or server behavior was changed, and no production channel was used.

## Evidence

| Check | Result |
|---|---|
| Picture tracker, actual FFmpeg receiver and HLS progress tests | PASS — 12 tests, [receipt](evidence/picture-tests.txt) |
| Actual native NoAr SRT output, moving fixture | PASS — 46 decoded frames, 45 detected changes |
| Actual native NoAr SRT output, intentionally static fixture | PASS — freeze detected with 46 incoming frames and healthy `/health` |
| Actual native NoAr HLS output, moving fixture | PASS — 36 decoded frames, 35 detected changes |
| Actual native NoAr HLS output, intentionally static fixture | PASS — freeze detected with 36 incoming frames and healthy `/health` |
| Python syntax and `git diff --check` | PASS |
| Full SRT/HLS Docker-image soak with scheduled live faults | NOT_RUN locally — configured in the CI matrix |
| Full 90-minute endurance run | NOT_RUN |
| NDI / SDI | NOT_AVAILABLE — no runtime/receiver or output/capture hardware fixture supplied |

See the [machine-readable receipt](evidence/native-picture-verification.json).
Each real-output scenario observed the decoded stream for 18 seconds after
receiver acquisition, using the same 12-second freeze threshold as the soak.
The frozen cases are **negative controls**: passing means the intentionally
static picture was detected, despite continuing frame delivery and health.

The moving fixtures contain a changing test pattern, and the frozen fixtures
contain a constant blue picture. Both have encoded audio. This verifies
whole-picture freeze detection; it does not simulate every possible decoder
failure or establish audio continuity, lip sync or visual correctness.

## Reproduce

```sh
python3 -m unittest discover -s qa/soak -p 'test_*.py'
NOAR_SOAK=1 python3 qa/soak/verify_picture.py --binary target/debug/noar
```

This session used an explicitly scoped plugin path containing the additional
native GStreamer tools/libav plugin extracted under a temporary workspace
directory, without changing the host installation. The successful command was:

```sh
NOAR_SOAK=1 \
GST_PLUGIN_PATH=/workspace/work/noar-native/usr/lib/x86_64-linux-gnu/gstreamer-1.0 \
python3 qa/soak/verify_picture.py --binary target/debug/noar
```

Versions: Rust 1.99.1, gstreamer-rs 0.24.5, native GStreamer core/libav 1.26.2,
FFmpeg 7.1.5-0+deb13u1. The current server binary was built with
`--no-default-features`. The native fixtures were 640×360, avoiding an unrelated
known 1.26.2 decoder defect on non-aligned video widths.

The published NoAr image contains corrected GStreamer 1.26.11 components.
These short native checks do **not** replace testing that exact release image.
The committed verification script permits up to 4 seconds of final-frame age
for 2-second HLS segment delivery; the recorded run passed a stricter 2-second
assertion as well.

## CI and limits

The existing soak workflow now has separate SRT/HLS matrix jobs, installs the
independent receiver, runs the observer regressions, and keeps per-output
decoder logs and JSONL picture observations alongside the original artifacts.
This workflow was edited locally; it was not pushed or run on GitHub here.
The nightly default remains 90 minutes, and PR runs remain 15 minutes.

The observer compares known-moving, logo-free synthetic pictures and respects
bounded startup, restart and deliberately missing-signal intervals. Arbitrary
real programmes may intentionally show still images, and a small moving region
can conceal a partially frozen picture; this is not a production alarm.

Documentation impact: **UPDATED** (`qa/soak/README.md`, `docs/developer.md`,
`CHANGELOG.md`, this ledger). Release-publishing behavior and Docker runtime
configuration did not change. No new independent audit council was run.
