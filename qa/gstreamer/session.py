#!/usr/bin/env python3
"""Keep the demo and observer alive in one disposable container."""

import argparse
import os
from pathlib import Path
import signal
import subprocess
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cycles", type=int, choices=range(1, 6), default=3)
    args = parser.parse_args()
    for name in ("events.jsonl", "report.json", "fault.json"):
        Path("/artifacts", name).unlink(missing_ok=True)
    children = []

    def stop(*_):
        for child in children:
            if child.poll() is None:
                child.terminate()

    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGINT, stop)
    try:
        demo = subprocess.Popen([sys.executable, "playout.py", "demo", "--cycles", str(args.cycles)])
        children.append(demo)
        if os.environ.get("NOAR_GST_VERIFY", "1") == "1":
            command = [sys.executable, "verify.py"]
            if os.environ.get("NOAR_GST_RECOVERY") == "1":
                command.append("--recovery")
            observer = subprocess.Popen(command)
            children.append(observer)
            observed = observer.wait()
            if observed:
                stop()
            return int(bool(demo.wait() or observed))
        return demo.wait()
    finally:
        stop()
        for child in children:
            try:
                child.wait(timeout=5)
            except subprocess.TimeoutExpired:
                child.kill()
                child.wait()


if __name__ == "__main__":
    sys.exit(main())
