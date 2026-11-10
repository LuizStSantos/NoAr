#!/usr/bin/env python3
"""Exercise converted graphics through real HLS in an owned disposable fixture."""
import argparse
import datetime as dt
import hashlib
import json
import os
from pathlib import Path
import sqlite3
import statistics
import subprocess
import time

from compatibility import Client


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--work-root", type=Path, required=True)
    parser.add_argument("--screenshots", type=Path, required=True)
    parser.add_argument("--free-window-only", action="store_true")
    parser.add_argument("--composite-only", action="store_true")
    args = parser.parse_args()
    root = args.work_root.resolve()
    if os.getenv("NOAR_E2E_STAGING") != "1" or os.getenv("NOAR_E2E_DISPOSABLE") != "1":
        raise SystemExit("Explicit disposable staging opt-in is required")
    if not (root / ".qa-graphics-owned").is_file():
        raise SystemExit("Owned fixture marker is required")
    args.screenshots.mkdir(parents=True, exist_ok=True)

    client = Client("http://127.0.0.1:8787", json.loads((root / "auth.json").read_text()))
    storage = Path(client.ok("GET", "/api/channel/1")["storage"]).resolve()
    if not storage.is_relative_to(root):
        raise SystemExit("Server storage must be inside the owned fixture")
    captures = []
    database = sqlite3.connect(root / "db/noar.db")
    frozen_sql = ("SELECT add_logo,logo,logo_scale,logo_position,logo_opacity,logo_transition_in,"
                  "logo_transition_out,logo_transition_seconds,lshape,lshape_scale,lshape_corner,"
                  "graphics_scenes FROM config_processing WHERE config_id=1")
    frozen = database.execute(frozen_sql).fetchone()

    def control(kind, action, **data):
        return client.ok("POST", f"/api/control/1/{kind}", {"action": action, **data})

    def marks(**data):
        client.ok("PUT", "/api/file/1/logo-marks", {"path": "compatibility.mp4", "hide": [], **data})
        client.ok("POST", "/api/control/1/process", {"command": "restart"})

    def capture(name, samples):
        if args.free_window_only and not name.startswith("free-window-"):
            return

        if args.composite_only and not name.startswith("layer-"):
            return

        # Published HLS segments can still contain queued frames from the previous
        # action. Wait for ten newly published segments before decoding a frame.
        playlist = root / "public/live/compat.m3u8"
        initial = set(playlist.read_text().splitlines()) if playlist.exists() else set()
        deadline = time.monotonic() + 75
        while time.monotonic() < deadline:
            lines = playlist.read_text().splitlines() if playlist.exists() else []
            segments = [line for line in lines if line.endswith(".ts")]
            if len(set(segments) - initial) >= 10:
                segment = playlist.parent / segments[-2]
                break
            time.sleep(1)
        else:
            raise AssertionError("No fresh completed HLS segments")
        command = ["ffmpeg", "-hide_banner", "-loglevel", "error", "-i", str(segment), "-frames:v", "1"]
        raw = subprocess.check_output(command + ["-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1"])
        assert len(raw) == 640 * 360 * 3
        measurements = []
        for x, y, expected in samples:
            rgb = [statistics.median(raw[((row * 640 + col) * 3) + channel]
                                     for row in range(y, y + 8) for col in range(x, x + 8))
                   for channel in range(3)]
            if max(abs(a - b) for a, b in zip(rgb, expected)) > 12:
                raise AssertionError(f"{name} pixel {x},{y}: {rgb}, expected {expected}")
            measurements.append({"x": x, "y": y, "rgb": rgb, "expected": expected, "tolerance": 12})
        destination = args.screenshots / f"output-compat-c-{name}.jpg"
        subprocess.run(command + ["-q:v", "2", "-update", "1", "-y", str(destination)], check=True)
        health = client.ok("GET", "/health")
        assert health["status"] == "ok" and health["channels"][0]["state"] == "on_air"
        captures.append({"stage": name, "screenshot": destination.name,
                         "segment_sha256": hashlib.sha256(segment.read_bytes()).hexdigest(),
                         "health": health, "pixels": measurements})
        print("PASS:", name, flush=True)

    for event in client.ok("GET", "/api/schedule/1/graphics"):
        client.ok("DELETE", f"/api/schedule/1/graphics/{event['id']}")
    roles = client.ok("GET", "/api/graphics/1/channel")
    ticker = next(role for role in roles if role["kind"] == "ticker")
    client.ok("POST", f"/api/graphics/1/channel/{ticker['template']['id']}", {"on": False})
    control("lshape", "off")
    control("lshape", "reset")
    control("lshape", "move", position="default")
    control("logo", "show")
    control("logo", "move", position="default")
    marks()
    gray = (79, 79, 79)
    amber = (166, 128, 51)
    blue = (52, 113, 184)
    purple = (136, 61, 157)
    capture("canonical-logo", [(570, 25, amber), (25, 25, gray)])
    marks(logo_image="clip-logo.png", logo_position="0%:100%")
    capture("clip-logo", [(20, 325, purple), (570, 25, gray)])
    control("logo", "replace", logo="replacement-logo.png")
    control("logo", "move", position="top-left")
    capture("external-logo", [(25, 25, blue), (20, 325, gray)])
    control("logo", "show")
    control("logo", "move", position="default")
    capture("clip-logo-restored", [(20, 325, purple), (25, 25, gray)])
    marks(hide=[{"from": 0, "to": 180}], logo_image="clip-logo.png")
    control("logo", "show")
    capture("clip-logo-hidden", [(570, 25, gray), (25, 25, gray)])
    marks(lshape=[{"from": 0, "to": 180}], lshape_graphic="clip-art.png",
          lshape_corner="bottom-left", lshape_scale=60)
    capture("clip-lshape", [(25, 25, (40, 80, 24)), (25, 200, gray), (500, 300, (40, 80, 24))])
    marks()
    capture("clip-cleared", [(570, 25, amber), (25, 25, gray)])
    role = next(item for item in roles if item["kind"] == "lshape")
    template = role["template"]
    original = json.loads(json.dumps(template))
    picture = next(layer for layer in template["layers"] if layer["kind"] == "video")
    picture.update({"x": 10, "y": 10, "width": 70, "height": 50})
    client.ok("PUT", f"/api/graphics/1/templates/{template['id']}", template)
    control("lshape", "on")
    control("lshape", "replace", graphic="replacement-art.png")
    capture("free-window-replacement", [(25, 25, (112, 40, 56)), (100, 100, gray),
                                         (100, 250, (112, 40, 56))])
    control("lshape", "off")
    control("lshape", "reset")
    marks(lshape=[{"from": 0, "to": 180}], lshape_graphic="clip-art.png")
    capture("free-window-clip", [(25, 25, (40, 80, 24)), (100, 100, gray),
                                 (100, 250, (40, 80, 24))])
    marks()
    client.ok("PUT", f"/api/graphics/1/templates/{template['id']}", original)
    if args.free_window_only:
        assert database.execute(frozen_sql).fetchone() == frozen
        (root / "free-window-evidence.json").write_text(json.dumps({
            "status": "PASS", "captures": captures, "frozen_legacy_columns_unchanged": True,
        }, indent=2) + "\n")
        return

    # These extra layers have no equivalent in 2.2.1. Prove that legacy image
    # controls change only the stable target and preserve the composed artwork.
    roles = client.ok("GET", "/api/graphics/1/channel")
    logo = next(item["template"] for item in roles if item["kind"] == "logo")
    logo_original = json.loads(json.dumps(logo))
    badge = {"id": "proof-badge", "kind": "box", "name": "Proof badge", "visible": True,
             "x": 62.5, "y": 3.3333333333333335, "width": 6.25, "height": 5.555555555555555,
             "opacity": 1, "fill": "#ffffff", "animation_in": "none", "animation_out": "none",
             "animation_ms": 0, "delay_ms": 0}
    logo["layers"].append(badge)
    client.ok("PUT", f"/api/graphics/1/templates/{logo['id']}", logo)
    control("logo", "replace", logo="replacement-logo.png")
    capture("layer-logo-replacement", [(570, 25, blue), (405, 18, (255, 255, 255))])
    control("logo", "move", position="bottom-right")
    capture("layer-logo-move", [(570, 325, blue), (405, 305, (255, 255, 255)), (405, 18, gray)])
    control("logo", "show")
    control("logo", "move", position="default")
    client.ok("PUT", f"/api/graphics/1/templates/{logo['id']}", logo_original)

    artwork = json.loads(json.dumps(original))
    badge.update({"x": 0, "y": 0})
    artwork["layers"].insert(1, badge)
    client.ok("PUT", f"/api/graphics/1/templates/{artwork['id']}", artwork)
    control("lshape", "on")
    control("lshape", "replace", graphic="replacement-art.png")
    capture("layer-lshape-replacement", [(5, 5, (255, 255, 255)), (25, 50, (112, 40, 56)), (250, 100, gray)])
    control("lshape", "off")
    control("lshape", "reset")
    marks(lshape=[{"from": 0, "to": 180}], lshape_graphic="clip-art.png")
    capture("layer-lshape-clip", [(5, 5, (255, 255, 255)), (25, 50, (40, 80, 24)), (250, 100, gray)])
    marks()
    client.ok("PUT", f"/api/graphics/1/templates/{artwork['id']}", original)

    if args.composite_only:
        assert database.execute(frozen_sql).fetchone() == frozen
        (root / "composite-evidence.json").write_text(json.dumps({
            "status": "PASS", "captures": captures, "frozen_legacy_columns_unchanged": True,
        }, indent=2) + "\n")
        return

    now = dt.datetime.now(dt.timezone.utc)
    start = now + dt.timedelta(seconds=10)
    end = now + dt.timedelta(seconds=70)
    actions = {"name": "Snapshot", "logo": "hide", "lshape": "on"}
    event = client.ok("POST", "/api/schedule/1/graphics", {
        "title": "Compatibility scheduled snapshot", "actions": actions,
        "start_date": str(start.date()), "start_time": start.strftime("%H:%M:%S"),
        "end_time": end.strftime("%H:%M:%S"), "repeat_days": 0, "enabled": True,
    })
    config = client.ok("GET", "/api/playout/config/1")
    config["processing"]["graphics_scenes"][0]["logo"] = "show"
    config["processing"]["graphics_scenes"][0]["lshape"] = "off"
    client.ok("PUT", "/api/playout/config/1", config)
    stored = client.ok("GET", "/api/schedule/1/graphics")
    snapshot = next(item for item in stored if item["id"] == event["id"])
    assert snapshot["actions"]["logo"] == "hide" and snapshot["actions"]["lshape"] == "on"
    while dt.datetime.now(dt.timezone.utc) < start + dt.timedelta(seconds=3):
        time.sleep(1)
    capture("scheduled-scene", [(25, 25, (24, 56, 120)), (250, 100, gray)])
    while dt.datetime.now(dt.timezone.utc) < end + dt.timedelta(seconds=3):
        time.sleep(1)
    capture("scheduled-reverted", [(570, 25, amber), (25, 25, gray)])
    client.ok("DELETE", f"/api/schedule/1/graphics/{event['id']}")
    assert database.execute(frozen_sql).fetchone() == frozen
    result = {"status": "PASS", "captures": captures, "frozen_legacy_columns_unchanged": True,
              "scheduled_action_snapshot_and_reversal": "PASS"}
    (root / "output-evidence.json").write_text(json.dumps(result, indent=2) + "\n")


if __name__ == "__main__":
    main()
