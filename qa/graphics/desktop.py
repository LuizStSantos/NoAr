#!/usr/bin/env python3
"""Run paired desktop graphics scenarios inside the owned GStreamer container."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import time

from compatibility import Client


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--work-root", type=Path, required=True)
    parser.add_argument("--main-binary", required=True)
    parser.add_argument("--candidate-binary", required=True)
    parser.add_argument("--auth-file", type=Path, required=True)
    parser.add_argument("--duration", type=int, default=240)
    args = parser.parse_args()
    root = args.work_root.resolve()
    if os.getenv("NOAR_E2E_STAGING") != "1" or os.getenv("NOAR_E2E_DISPOSABLE") != "1":
        raise SystemExit("Explicit disposable staging opt-in is required")
    if not (root / ".qa-graphics-owned").is_file() or args.duration < 220:
        raise SystemExit("Owned marker and duration covering two clip boundaries are required")
    credentials = json.loads(args.auth_file.read_text())
    runtime = root / "desktop-runtime"
    runtime.mkdir(mode=0o700, exist_ok=True)
    environment = dict(os.environ, DISPLAY=":99", XDG_RUNTIME_DIR=str(runtime),
                       PULSE_SERVER=f"unix:{runtime}/pulse.sock",
                       ALSA_CONFIG_PATH="/workspace/noar-cloud/stage-c/alsa.conf")
    results = []
    for variant, binary in [("desktop-main", args.main_binary), ("desktop-branch", args.candidate_binary)]:
        directory = root / variant
        processes = []
        streams = []

        def launch(command, log, **extra):
            stream = (directory / log).open("w")
            streams.append(stream)
            process = subprocess.Popen(command, stdout=stream, stderr=subprocess.STDOUT,
                                       env=dict(environment, **extra))
            processes.append(process)
            return process

        try:
            launch(["Xvfb", ":99", "-screen", "0", "1280x720x24", "-ac", "-nolisten", "tcp"], "xvfb.log")
            launch(["pulseaudio", "--daemonize=no", "--exit-idle-time=-1", "--disable-shm=true", "-n",
                    f"--load=module-native-protocol-unix auth-anonymous=1 socket={runtime}/pulse.sock",
                    "--load=module-null-sink sink_name=graphics rate=48000 channels=2"], "pulse.log")
            time.sleep(2)
            process = launch([binary, "-l", "127.0.0.1:18788", "--log-to-console",
                              "--fake-time", "2026-11-06T00:02:27.074415072+00:00"],
                             "server.log", DB=str(directory / "noar.db"))
            started = time.monotonic()
            client = None
            for _ in range(30):
                try:
                    client = Client("http://127.0.0.1:18788", credentials)
                    if client.ok("GET", "/health")["status"] == "ok":
                        break
                except (OSError, RuntimeError):
                    pass
                time.sleep(1)
            else:
                raise AssertionError(f"{variant} did not become healthy")
            actions = [(124, "show", {}), (124, "move", {"position": "default"}),
                       (126, "hide", {}), (129, "replace", {"logo": "replacement-logo.png"}),
                       (129, "move", {"position": "top-left"}), (129, "show", {}),
                       (129, "replace", {"logo": "replacement-logo.png"}),
                       (132, "show", {}), (132, "move", {"position": "default"})]
            health = []
            while time.monotonic() - started < args.duration:
                elapsed = time.monotonic() - started
                assert process.poll() is None, f"{variant} exited during the scenario"
                while actions and actions[0][0] <= elapsed:
                    _, action, data = actions.pop(0)
                    client.ok("POST", "/api/control/1/logo", {"action": action, **data})
                health.append({"elapsed": round(elapsed, 3), "health": client.ok("GET", "/health")})
                time.sleep(2)
            subprocess.run(["ffmpeg", "-hide_banner", "-loglevel", "error", "-f", "x11grab",
                            "-video_size", "640x360", "-i", ":99.0", "-frames:v", "1",
                            "-q:v", "2", "-update", "1", "-y", str(directory / "desktop.jpg")],
                           env=environment, check=True)
        finally:
            for process in reversed(processes):
                if process.poll() is None:
                    process.send_signal(signal.SIGTERM)
                    try:
                        process.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait()
            for stream in streams:
                stream.close()
        text = re.sub(r"\x1b\[[0-9;]*m", "", (directory / "server.log").read_text())
        warnings = [line for line in text.splitlines() if any(term in line for term in
                    ["underrun/overrun", "desktop audio stream error", "Clip begin out of sync"])]
        results.append({"variant": variant, "duration_seconds": args.duration, "health": health,
                        "audio_underrun_overrun_count": text.count("desktop audio device underrun/overrun"),
                        "audio_io_error_count": text.count("snd_pcm_avail_delay"),
                        "clip_sync_seconds": re.findall(r"Clip begin out of sync for\s+([-\d.]+)", text),
                        "warnings": warnings, "binary_sha256": hashlib.sha256(Path(binary).read_bytes()).hexdigest()})
        (root / "desktop-pair.json").write_text(json.dumps({"results": results}, indent=2) + "\n")
        print("Completed", variant, flush=True)


if __name__ == "__main__":
    main()
