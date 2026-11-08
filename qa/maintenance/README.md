# Disposable maintenance regression checks

The full [browser harness](../e2e/README.md) covers playlist editing, Scheduler
CRUD, Storage download/mutation dialogs and report loading-error recovery.
`runtime.py` adds an actual HLS scheduled-interruption/return proof.

Prepare and start a separate test installation with synthetic media, a private
auth JSON file (`username`, `password`), and a channel-1 storage directory under
an owned work root. Create `.qa-maintenance-owned` in that root. The backend
must use `logs/noar_1.log` and its channel public directory must be accessible
at the configured filesystem path. The script accepts only loopback API URLs.

```sh
NOAR_E2E_STAGING=1 NOAR_E2E_DISPOSABLE=1 \
python3 qa/maintenance/runtime.py \
  --base http://127.0.0.1:8787 \
  --work-root /absolute/disposable/root \
  --auth-file /private/disposable-auth.json \
  --artifacts /absolute/evidence
```

With `--container noar-dev`, media generation/probing/decoding runs inside that
local container; it must see the same absolute work/artifact paths. Without it,
local `ffmpeg`/`ffprobe` commands are used as independent test tools. The backend
continues to use GStreamer.

This test **changes its disposable channel configuration**, disables existing
synthetic material schedules in the copy, writes two owned MP4 files, selects
640×360/25 fps HLS with a 30-second playlist starting just before the test, and schedules a three-second red clip to interrupt a blue
programme and continue that interrupted clip. The analysis freezes the stopped
HLS segment list with an `ENDLIST` marker; the emitted TS files remain unchanged. It stops the playout at the end. The completed event
is retained for restart-persistence checks. Use a fresh fixture for each run.

Evidence includes the actual HLS frames before/during/after the interruption,
decoded source-color counts, video/audio stream metadata, segment SHA-256,
active/completed schedule observations, health samples and the scoped channel
log. This is a scoped execution/return test; exact renderer parity uses the
separate [graphics criteria](../graphics/parity-criteria.md).
