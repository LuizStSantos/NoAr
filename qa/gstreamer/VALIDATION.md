# Prototype validation — 2026-10-29 UTC

These results apply only to the isolated `qa/gstreamer` experiment, based on
NoAr main `6e8d7c60` (v1.11.0). No NoAr application code, manifest dependency,
release workflow or production configuration was modified. The prototype branch
is published for isolated test delivery. No releases were published or
production services contacted.

## Runtime

- Linux amd64, disposable Docker Compose project `noar-gstreamer-prototype`.
- Debian trixie; GStreamer 1.26.2; Python 3.13.5.
- `gstreamer1.0-plugins-bad` 1.26.2-3+deb13u3.
- `libsrt1.5-gnutls` 1.5.4-1+deb13u1.
- FFmpeg 7:7.1.5-0+deb13u1 for fixture generation, capture and independent decoding.
- MediaMTX 1.15.3, internal paths `input` and `output`.
- 640×360 at 25 fps, H.264/AAC, mono 48 kHz. All media is synthetic.

GStreamer manages pipeline timing and SRT lifecycle in this experiment. Its
`libav` codec plugin still uses FFmpeg libraries. A successful experiment does
not establish that NoAr can remove all FFmpeg dependencies.

## Executed checks

| Check | Status | Evidence |
| --- | --- | --- |
| Container build | PASS | Local prototype image built successfully |
| Dockerfile build checks | PASS | `docker build --check qa/gstreamer`: no warnings |
| Compose configuration | PASS | Prototype `docker compose config --quiet` |
| Python compilation / shell syntax | PASS | `compileall`; `sh -n qa/gstreamer/run.sh` |
| Observer adversarial unit tests | PASS | 9 tests, including timestamp reversal, lost live audio, incomplete cycles, output reconnection and missing/drop statistics |
| Selected AI toolkit unit suite | PASS | Existing `verify_changes.py --files ... --run`; toolkit was not modified |
| Three normal file/live/file cycles | PASS | Decoded video markers and tones follow all three scheduled sequences |
| Live source loss and recovery | PASS | Synthetic publisher stopped during first live phase; fallback to file B; live returned in cycles two and three |
| Output connection continuity | PASS | One output publisher ID and one observer ID throughout each capture; output automatic reconnection disabled |
| Received timestamps and decoding | PASS | Zero DTS regressions; maximum video gap 40 ms; maximum audio gap 21.356 ms; error-free full A/V decode |
| Reported SRT packet drops | PASS | Output publisher/reader reported zero send and receive drops; this is not a claim of zero network loss under all conditions |
| Manual VLC observation | NOT_RUN | Needs operator observation of this new prototype |
| NoAr Rust/backend/scheduler integration | NOT_RUN | Independent prototype; no application integration implemented |
| Continue mode / playlist looping | NOT_RUN | Files advance on the clock; bounded synthetic fixtures |
| Output/MediaMTX failure recovery | NOT_RUN | Only loss of the synthetic input publisher was injected |
| Production load / hardware encoding / long soak | NOT_RUN | No broadcast readiness claim |

Both principal scenarios ran three cycles, each with a nominal 77-second
controller timeline. The observer joined shortly after startup and captured
approximately 75 seconds. It independently decoded each source's marker and
tone rather than trusting switch commands alone. Exact run measurements and
session IDs are recorded in generated `artifacts/report.json` and
`artifacts/connections.json`; the runner replaces these files on each invocation.

| Measured capture property | Final normal run | Recovery run |
| --- | --- | --- |
| Video packets / maximum DTS gap | 1853 / 40 ms | 1853 / 40 ms |
| Audio packets / maximum DTS gap | 3511 / 21.356 ms | 3511 / 21.356 ms |
| End-of-capture A/V timestamp difference | 8.000 ms | 8.022 ms |
| Maximum receiver progress interval | 600.652 ms | 602.164 ms |

Receiver progress measures FFmpeg's reporting cadence after capture startup,
not per-packet delivery or display timing in VLC. The gate is <750 ms.

The first source-loss experiment revealed multiple asynchronous errors from the
same disconnected input, including messages queued after its bin was detached.
The controller now retains ownership of retired live bins and logs their
teardown messages without shutting down the output pipeline. The completed
recovery run verified this path with an actual source interruption.

## Scope of the result

The experiment demonstrates that a persistent GStreamer compositor, audio mixer,
encoder/muxer and SRT output can survive scheduled live-ingest lifecycle changes
and controlled source loss in this synthetic setup. Timestamp gates are coarse;
they do not certify lip sync, end-device playback, production resource usage or
24/7 operation.

The next engineering step is to define a Rust media-backend interface and verify
NoAr's scheduling and configuration contracts before adding an optional
GStreamer backend. Preserve the current FFmpeg backend for comparison and return.
