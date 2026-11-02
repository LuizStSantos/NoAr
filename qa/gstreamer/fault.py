#!/usr/bin/env python3
"""Stop only the synthetic source service, then restore it for the next cycle."""

import json
from pathlib import Path
import subprocess
import time


def compose(*args):
    subprocess.run(["docker", "compose", "--project-name", "noar-gstreamer-prototype",
                    "--file", str(Path(__file__).with_name("compose.yaml")), *args], check=True)


def main():
    artifacts = Path(__file__).with_name("artifacts")
    deadline = time.monotonic() + 40
    while time.monotonic() < deadline:
        path = artifacts / "events.jsonl"
        lines = path.read_text().splitlines() if path.exists() else []
        events = []
        for line in lines:
            try:
                events.append(json.loads(line))
            except json.JSONDecodeError:
                pass
        if any(item["event"] == "error" for item in events):
            raise RuntimeError("Demo failed before source-loss injection")
        if any(item["event"] == "switch" and item.get("source") == "live" for item in events):
            time.sleep(1)
            record = {"action": "stop synthetic source", "observed_elapsed": events[-1]["elapsed"]}
            compose("stop", "--timeout", "2", "source")
            (artifacts / "fault.json").write_text(json.dumps(record, indent=2) + "\n")
            time.sleep(3)
            compose("start", "source")
            return
        time.sleep(0.2)
    raise RuntimeError("No live entry observed within 40 s; fault was not injected")


if __name__ == "__main__":
    main()
