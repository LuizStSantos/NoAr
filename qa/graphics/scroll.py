#!/usr/bin/env python3
"""Measure scrolling text in actual HLS from a runner-owned NoAr installation."""

import argparse
import datetime as dt
import hashlib
import json
import os
import re
from pathlib import Path
import subprocess
import time
import urllib.error
import urllib.parse
import urllib.request


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", default="http://127.0.0.1:8787")
    parser.add_argument("--work-root", type=Path, required=True)
    parser.add_argument("--session-file", type=Path, required=True)
    parser.add_argument("--artifacts", type=Path, required=True)
    parser.add_argument("--container", default="")
    args = parser.parse_args()
    root = args.work_root.resolve()
    host = urllib.parse.urlparse(args.base)
    if (os.getenv("NOAR_E2E_STAGING") != "1" or os.getenv("NOAR_E2E_DISPOSABLE") != "1"
            or host.hostname not in {"127.0.0.1", "localhost", "::1"}
            or host.scheme not in {"http", "https"} or not (root / ".qa-graphics-owned").is_file()):
        raise SystemExit("Explicit owned loopback disposable staging is required")
    session = json.loads(args.session_file.read_text())
    token = next(item["value"] for origin in session["origins"] for item in origin["localStorage"] if item["name"] == "token")
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))

    def api(method, path, body=None, expected=200):
        request = urllib.request.Request(args.base + path, method=method,
            headers={"Authorization": "Bearer " + token, "Content-Type": "application/json"},
            data=None if body is None else json.dumps(body).encode())
        try:
            response = opener.open(request, timeout=40)
        except urllib.error.HTTPError as error:
            response = error
        raw = response.read().decode()
        statuses = expected if isinstance(expected, tuple) else (expected,)
        assert response.code in statuses, f"{method} {path}: HTTP {response.code}"

        try:
            return json.loads(raw) if raw else None
        except json.JSONDecodeError:
            return raw

    channel = api("GET", "/api/channel/1")
    assert Path(channel["storage"]).resolve() == root / "media", "Storage is outside the owned fixture"
    args.artifacts.mkdir(parents=True, exist_ok=True)
    log_path = root / "backend.log"
    log_offset = log_path.stat().st_size

    def media(*command):
        prefix = ["docker", "exec", args.container] if args.container else []

        return subprocess.check_output([*prefix, *command], timeout=90)

    def until(check, description):
        deadline = time.monotonic() + 45

        while time.monotonic() < deadline:
            result = check()

            if result:
                return result

            time.sleep(0.3)

        raise AssertionError("Timed out: " + description)

    api("POST", "/api/control/1/process", {"command": "stop"})
    api("POST", "/api/graphics/1/out", {})
    api("POST", "/api/control/1/lshape", {"action": "off"})

    for model in api("GET", "/api/graphics/1/templates"):
        if model["name"] == "Scroll HLS model":
            api("DELETE", f"/api/graphics/1/templates/{model['id']}")

    for source in api("GET", "/api/graphics/1/sources"):
        if source["name"] == "Scroll HLS feed":
            api("DELETE", f"/api/graphics/1/sources/{source['id']}")

    for model in api("GET", "/api/graphics/1/channel"):
        if model["kind"] in {"logo", "ticker", "permanent"}:
            api("POST", f"/api/graphics/1/channel/{model['template']['id']}", {"on": False})

    for event in api("GET", "/api/schedule/1/events"):
        api("DELETE", f"/api/schedule/1/{event['id']}")

    for event in api("GET", "/api/schedule/1/graphics"):
        api("DELETE", f"/api/schedule/1/graphics/{event['id']}")

    movie = root / "media/scroll-blue.mp4"
    media("ffmpeg", "-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i", "color=c=blue:s=640x360:r=25",
          "-f", "lavfi", "-i", "anullsrc=r=48000:cl=stereo", "-t", "30", "-c:v", "libx264", "-preset", "ultrafast",
          "-threads", "2", "-pix_fmt", "yuv420p", "-g", "25", "-c:a", "aac", str(movie))
    channel["timezone"] = "UTC"
    api("PATCH", "/api/channel/1", channel)
    config = api("GET", "/api/playout/config/1")
    output = next(item for item in api("GET", "/api/playout/outputs/1") if item["name"] == "hls")
    config["output"].update(id=output["id"], mode="hls", width=640, height=360, fps=25,
        video_codec="libx264", audio_codec="aac", hls_playlist_name="scroll", hls_segment_duration=2, hls_list_size=12, hls_variants=[])
    config["output"]["video_options"]["preset"] = "ultrafast"
    config["processing"]["mode"] = "playlist"
    config["playlist"].update(day_start="00:00:00", length="00:00:30", infinit=True)
    config["recording"]["enable"] = False
    api("PUT", "/api/playout/config/1", config)
    today = dt.datetime.now(dt.timezone.utc).date().isoformat()
    api("POST", "/api/playlist/1", {"channel": "Scroll test", "date": today,
        "program": [{"source": str(movie), "in": 0, "out": 30, "duration": 30}]}, expected=(200, 409))
    saved = api("GET", f"/api/playlist/1?date={today}")
    assert len(saved["program"]) == 1 and saved["program"][0]["source"] == str(movie)
    # Remove only this runner's closed output before reusing its segment names.
    for path in (root / "public/live").glob("scroll*"):
        if path.suffix in {".ts", ".m3u8"}:
            path.unlink()
    source = api("POST", "/api/graphics/1/sources", {"name": "Scroll HLS feed", "kind": "csv", "interval": 30,
        "document": "headline\nMARKETS 11111111 - NoAr scrolling data - Sao Paulo - USD 5.10 - EUR 6.20 - Broadcast news"})
    layer = {"id": "crawl", "name": "Market crawl", "kind": "text", "visible": True,
        "x": 10, "y": 80, "width": 80, "height": 12, "text": "{headline}", "font_family": "DejaVu Sans Mono",
        "font_size": 6, "color": "#ffffff", "fill": "#101010", "animation_in": "fade", "animation_out": "fade",
        "animation_ms": 300, "opacity": 0.5, "scroll": {"direction": "left", "speed": 80, "gap": 80},
        "data_fields": {"headline": {"source_id": source["id"], "column": "headline", "row": 0}}}
    template = api("POST", "/api/graphics/1/templates", {"name": "Scroll HLS model", "group": "QA", "duration": 0, "layers": [layer]})
    api("POST", "/api/control/1/process", {"command": "start"})
    playlist = root / "public/live/scroll.m3u8"
    until(lambda: playlist.exists(), "HLS playlist")
    evidence = {"status": "RUNNING", "width": 640, "height": 360, "fps": 25, "speed": 80,
        "compression_tolerance_px": 2, "stages": [], "template_id": template["id"], "source_id": source["id"]}

    def capture(name, direction=None, threshold=190):
        initial = set(playlist.read_text().splitlines())

        def fresh():
            segments = [line for line in playlist.read_text().splitlines() if line.endswith(".ts")]

            return playlist.parent / segments[-2] if len(set(segments) - initial) >= 4 else None

        segment = until(fresh, "fresh HLS segments")
        retained = args.artifacts / f"{name}.ts"
        retained.write_bytes(segment.read_bytes())
        timestamps = json.loads(media("ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries",
            "frame=best_effort_timestamp_time", "-of", "json", str(retained)))["frames"]
        raw = media("ffmpeg", "-v", "error", "-i", str(retained), "-an", "-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1")
        frame_size = 640 * 360 * 3
        count = len(raw) // frame_size
        assert count > 25
        blue = list(raw[(10 * 640 + 10) * 3:(10 * 640 + 10) * 3 + 3])
        assert blue[2] > 200 and max(blue[:2]) < 30, f"Synthetic blue programme not on air: {blue}"
        image = args.artifacts / f"output-scroll-{name}.jpg"
        media("ffmpeg", "-v", "error", "-y", "-i", str(retained), "-frames:v", "1", "-q:v", "2", str(image))
        later = args.artifacts / f"output-scroll-{name}-later.jpg"
        media("ffmpeg", "-v", "error", "-y", "-i", str(retained), "-vf", "select=eq(n\\,25)",
            "-fps_mode", "passthrough", "-frames:v", "1", "-q:v", "2", str(later))

        def profile(frame_index):
            frame = raw[frame_index * frame_size:(frame_index + 1) * frame_size]

            return [sum(min(frame[(y * 640 + x) * 3:(y * 640 + x) * 3 + 3]) > threshold for y in range(288, 330)) for x in range(64, 576)]

        first, second = profile(0), profile(25)
        stage = {"name": name, "segment_sha256": hashlib.sha256(retained.read_bytes()).hexdigest(),
            "frame_pts": [timestamps[index]["best_effort_timestamp_time"] for index in [0, 25]],
            "screenshot": image.name, "later_screenshot": later.name, "text_pixels": sum(first), "text_threshold": threshold, "health": api("GET", "/health")}
        assert stage["health"]["status"] == "ok" and stage["health"]["channels"][0]["state"] == "on_air"

        if direction:
            scores = {}

            for shift in range(-100, 101):
                pairs = [(first[x], second[x + shift]) for x in range(512) if 0 <= x + shift < 512]
                scores[shift] = sum(abs(a - b) for a, b in pairs) / len(pairs)

            shift = min(scores, key=scores.get)
            delta = float(stage["frame_pts"][1]) - float(stage["frame_pts"][0])
            expected = (-1 if direction == "left" else 1) * 80 * delta
            assert sum(first) > 50, "No rendered text in HLS"
            assert abs(shift - expected) <= 2, f"{name}: displacement {shift}, expected {expected}"
            assert scores[shift] < 1, f"{name}: unstable glyph match {scores[shift]}"
            stage.update(direction=direction, displacement_px=shift, expected_px=expected, profile_error=scores[shift])
            if threshold == 80:
                colors = [list(raw[(y * 640 + x) * 3:(y * 640 + x) * 3 + 3]) for y in range(288, 330) for x in range(64, 576)]
                peak = max(colors, key=lambda pixel: pixel[0])
                assert 115 <= peak[0] <= 145 and peak[2] >= 232, f"Text/fill opacity was applied twice: {peak}"
                stage["half_opacity_peak_rgb"] = peak
        else:
            assert sum(first) == 0 and sum(second) == 0, "Text remained after take-out"

        evidence["stages"].append(stage)
        print("PASS:", name, flush=True)

    take = api("POST", "/api/graphics/1/take", {"template_id": template["id"], "duration": 0,
        "fields": {"headline": "MANUAL 11111111 - Continuous scrolling - NoAr broadcast headlines - Sao Paulo - Market report"}})
    capture("manual-left", "left", threshold=80)
    api("POST", "/api/graphics/1/out", {"take_id": take["take_id"]})
    layer["scroll"]["direction"] = "right"
    layer["opacity"] = 1
    template = api("PUT", f"/api/graphics/1/templates/{template['id']}", {**template, "layers": [layer]})
    take = api("POST", "/api/graphics/1/take", {"template_id": template["id"], "duration": 0})
    capture("data-right", "right")
    api("POST", "/api/graphics/1/out", {"take_id": take["take_id"]})
    layer["scroll"]["direction"] = "left"
    template = api("PUT", f"/api/graphics/1/templates/{template['id']}", {**template, "always_on": True, "layers": [layer]})
    api("POST", f"/api/graphics/1/channel/{template['id']}", {"on": True})
    capture("permanent-left", "left")
    source = api("PUT", f"/api/graphics/1/sources/{source['id']}", {
        "name": source["name"], "kind": "csv", "interval": 30, "revision": source["revision"],
        "document": "headline\nUPDATED 22222222 - NoAr scrolling data - Sao Paulo - USD 5.20 - EUR 6.30 - Broadcast news"})
    capture("data-updated", "left")
    evidence["source_after_refresh"] = {"revision": source["revision"], "rows": source["rows"]}
    api("POST", f"/api/graphics/1/channel/{template['id']}", {"on": False})
    capture("hidden")
    api("POST", f"/api/graphics/1/channel/{template['id']}", {"on": True})
    capture("restored", "left")
    log = re.sub(r"\x1b\[[0-9;]*m", "", log_path.read_bytes()[log_offset:].decode())
    assert "[ERROR]" not in log and "[ ERROR]" not in log, "Scoped backend log contains ERROR"
    (args.artifacts / "scroll-channel-log.txt").write_text(log)
    evidence["channel_errors"] = 0
    evidence["status"] = "PASS"
    (args.artifacts / "scroll-hls-evidence.json").write_text(json.dumps(evidence, indent=2) + "\n")


if __name__ == "__main__":
    main()
