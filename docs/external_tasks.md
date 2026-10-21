**External tasks**
================

An external task runs a script or binary when a clip starts (not when it is
skipped). Turn it on and set the program's path in **Settings › Advanced**.

NoAr passes the current playout data as a JSON object in the first
command-line argument. It includes the current media, playout mode, elapsed
time, time shift, live-ingest state, and available audio-level data.

For example, a shell script can read the JSON argument as its first parameter:

```sh
#!/bin/sh
data="$1"
printf '%s\n' "$data" >> /data/logs/task.log
```

The file must be executable, and it runs inside the NoAr container: keep it
under `./data` (seen as `/data` in the container), and use only programs the
image has (`/bin/sh` is there).

## Limits

Only one external task can run per channel. Starting the next clip task first
terminates a still-running task from the previous clip. Stopping the channel
also terminates its task. A task that runs longer than 30 seconds is terminated
automatically.

External tasks should finish quickly. Use a separate service, queue, or job
runner for work that needs to continue beyond the clip boundary.
