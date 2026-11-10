#!/usr/bin/env python3
"""Compare legacy graphics HTTP contracts against a disposable 2.2.1 baseline.

Both servers must use synthetic media under their owned disposable work roots. Credentials and keys
are kept in memory and are never included in the evidence file.
"""
import argparse
import datetime as dt
import json
import os
from pathlib import Path
import time
import urllib.error
import urllib.parse
import urllib.request


class Client:
    def __init__(self, base, credentials):
        parsed = urllib.parse.urlparse(base)
        if parsed.hostname not in {"127.0.0.1", "localhost", "::1"}:
            raise ValueError("Only loopback staging servers are supported")
        self.base = base.rstrip("/")
        self.token = None
        self.key = None
        status, body = self.call("POST", "/auth/login", {
            "username": credentials["username"], "password": credentials["password"]
        }, auth=False)
        if status != 200:
            raise RuntimeError(f"Disposable login failed: HTTP {status}")
        self.token = body["access"]

    def call(self, method, path, data=None, auth=True, header_key=False, query=None):
        headers = {}
        if auth and self.token:
            headers["Authorization"] = "Bearer " + self.token
        if header_key:
            headers["X-Logo-Key"] = self.key
        if data is not None:
            headers["Content-Type"] = "application/json"
        if query:
            path += "?" + urllib.parse.urlencode(query)
        request = urllib.request.Request(
            self.base + path, method=method, headers=headers,
            data=None if data is None else json.dumps(data).encode(),
        )
        time.sleep(0.04)
        try:
            response = urllib.request.urlopen(request, timeout=30)
        except urllib.error.HTTPError as error:
            response = error
        raw = response.read()
        try:
            body = json.loads(raw)
        except ValueError:
            body = raw.decode()
        return response.code, body

    def ok(self, method, path, data=None, **kwargs):
        status, body = self.call(method, path, data, **kwargs)
        if status != 200:
            raise RuntimeError(f"{method} {path}: HTTP {status}")
        return body


def seed(client, root, baseline):
    config = client.ok("GET", "/api/playout/config/1")
    channel = client.ok("GET", "/api/channel/1")
    storage = Path(channel["storage"]).resolve()
    if not storage.is_relative_to(root) or not (root / ".qa-graphics-owned").is_file():
        raise RuntimeError("Server storage is not inside the owned disposable fixture")
    client.ok("POST", "/api/control/1/process", {"command": "stop"})
    channel = client.ok("GET", "/api/channel/1")
    channel["timezone"] = "UTC"
    client.ok("PATCH", "/api/channel/1", channel)
    output = next(item for item in client.ok("GET", "/api/playout/outputs/1") if item["name"] == "hls")
    config["output"].update({
        "id": output["id"], "mode": "hls", "width": 640, "height": 360, "fps": 25,
        "video_codec": "libx264", "audio_codec": "aac", "audio_bitrate": 128,
        "hls_playlist_name": "baseline" if baseline else "compat",
        "hls_segment_duration": 2, "hls_list_size": 12, "hls_variants": [],
    })
    config["processing"].update({
        "add_logo": True, "logo": "channel-logo.png", "logo_scale": "80:48",
        "logo_position": "W-w-12:12", "logo_opacity": 0.6,
        "logo_transition_in": "cut", "logo_transition_out": "cut", "logo_transition_seconds": 0.4,
        "lshape": "channel-art.png", "lshape_scale": 70, "lshape_corner": "top-right",
        "guide_logo": "guide-logo.png",
        "graphics_scenes": [
            {"name": "Compatibility break", "logo": "hide", "lshape": "on"},
            {"name": "Compatibility move", "logo": "replace", "logo_image": "replacement-logo.png",
             "logo_position": "bottom-left", "lshape": "on", "lshape_graphic": "replacement-art.png",
             "lshape_corner": "bottom-left", "lshape_scale": 60},
            {"name": "Compatibility restore", "logo": "show", "logo_position": "default",
             "lshape": "off", "lshape_corner": "default"},
        ],
    })
    config["recording"]["enable"] = False
    config["playlist"].update({"day_start": "00:00:00", "length": "24:00:00", "infinit": True})
    client.ok("PUT", "/api/playout/config/1", config)
    today = dt.datetime.now(dt.timezone.utc).date()
    for day in [today, today + dt.timedelta(days=1)]:
        status, body = client.call("POST", "/api/playlist/1", {"channel": "Compatibility staging", "date": str(day), "program": [
            {"source": str(root / "media/compatibility.mp4"), "in": 0, "out": 180, "duration": 180}
            for _ in range(481)
        ]})
        if status != 200 and not (status == 409 and "already exists" in str(body)):
            raise RuntimeError(f"Synthetic playlist save failed: HTTP {status}")
    client.ok("POST", "/api/control/1/process", {"command": "start"})
    for _ in range(60):
        status, health = client.call("GET", "/health")
        if status == 200 and health["status"] == "ok" and health["channels"][0]["state"] == "on_air":
            break
        time.sleep(0.5)
    else:
        raise RuntimeError("Staging channel did not reach on_air")
    client.key = client.ok("POST", "/api/playout/config/1/keys/logo", {"action": "renew"})["key"]
    client.ok("POST", "/api/control/1/logo", {"action": "show"})
    client.ok("POST", "/api/control/1/logo", {"action": "move", "position": "default"})
    client.ok("POST", "/api/control/1/lshape", {"action": "off"})
    client.ok("POST", "/api/control/1/lshape", {"action": "reset"})
    client.ok("POST", "/api/control/1/lshape", {"action": "move", "position": "default"})


def run(candidate, baseline, root):
    cases = []

    def compare(name, method, path, data=None, expected=200, **kwargs):
        results = []
        for client in [baseline, candidate]:
            options = dict(kwargs)
            query = dict(options.pop("query", {}))
            if options.pop("query_key", False):
                query["key"] = client.key
            results.append(client.call(method, path, data, query=query, **options))
        if results[0] != results[1] or results[1][0] != expected:
            # State/error bodies contain only synthetic fixture names, never keys.
            raise AssertionError(f"{name}: expected {expected}; baseline={results[0]!r}, candidate={results[1]!r}")
        cases.append({"case": name, "method": method, "path": path, "status": results[1][0], "response": results[1][1]})

    for endpoint in ["logo", "lshape"]:
        compare(f"unauthenticated {endpoint}", "GET", f"/api/control/1/{endpoint}", auth=False, expected=401)
    compare("missing channel", "GET", "/api/control/999/logo", expected=404)
    compare("wrong logo key", "GET", "/logo/1/show", auth=False, query={"key": "invalid"}, expected=403)
    compare("missing logo key", "POST", "/logo/1/show", auth=False, expected=403)
    compare("initial logo fields", "GET", "/api/control/1/logo")
    compare("initial L-shape fields", "GET", "/api/control/1/lshape")
    for action, extra in [
        ("hide", {}), ("show", {}), ("move", {"position": "bottom-left"}),
        ("replace", {"logo": "replacement-logo.png"}),
        ("move", {"x": 25, "y": 75}), ("show", {}), ("move", {"position": "default"}),
    ]:
        compare(f"authenticated logo {action} {extra}", "POST", "/api/control/1/logo", {"action": action, **extra})
    for method in ["GET", "POST"]:
        for action, extra in [("hide", {}), ("show", {}), ("replace", {"logo": "replacement-logo.png"}), ("move", {"position": "top-left"})]:
            for key_source in ["header", "query"]:
                compare(f"keyed logo {method} {action} {key_source}", method, f"/logo/1/{action}", auth=False,
                        header_key=key_source == "header", query_key=key_source == "query", query=extra)
    for extra in [{"action": "replace"}, {"action": "replace", "logo": "missing.png"},
                  {"action": "move", "x": 12}, {"action": "move", "x": 101, "y": 0},
                  {"action": "move", "position": "not-a-place"}]:
        compare(f"invalid logo {extra}", "POST", "/api/control/1/logo", extra, expected=400)
    for action, extra in [("on", {}), ("move", {"position": "bottom-left", "scale": 60}),
                          ("replace", {"graphic": "replacement-art.png"}), ("reset", {}),
                          ("move", {"position": "default"}), ("off", {})]:
        compare(f"authenticated L-shape {action}", "POST", "/api/control/1/lshape", {"action": action, **extra})
    for method in ["GET", "POST"]:
        for action, extra in [("on", {}), ("replace", {"graphic": "replacement-art.png"}),
                              ("move", {"position": "top-left", "scale": 60}), ("reset", {}), ("off", {})]:
            for key_source in ["header", "query"]:
                compare(f"keyed L-shape {method} {action} {key_source}", method, f"/lshape/1/{action}", auth=False,
                        header_key=key_source == "header", query_key=key_source == "query", query=extra)
    compare("wrong L-shape key", "POST", "/lshape/1/on", auth=False, query={"key": "invalid"}, expected=403)
    for extra in [{"action": "replace"}, {"action": "replace", "graphic": "missing.png"},
                  {"action": "move", "position": "center"}, {"action": "move", "position": "top-left", "scale": 1}]:
        compare(f"invalid L-shape {extra}", "POST", "/api/control/1/lshape", extra, expected=400)
    for name in ["Compatibility break", "Compatibility move", "COMPATIBILITY RESTORE!"]:
        compare("authenticated scene " + name, "POST", "/api/control/1/scene", {"name": name})
    for method in ["GET", "POST"]:
        for key_source in ["header", "query"]:
            compare(f"keyed scene {method} {key_source}", method, "/scene/1/compatibility-move", auth=False,
                    header_key=key_source == "header", query_key=key_source == "query")
    compare("unknown scene", "POST", "/api/control/1/scene", {"name": "missing"}, expected=404)
    compare("wrong scene key", "GET", "/scene/1/compatibility-break", auth=False, query={"key": "invalid"}, expected=403)
    compare("final restore", "POST", "/api/control/1/scene", {"name": "Compatibility restore"})
    # Guide icon follows configured guide/model assets, never transient on-air overrides.
    for client in [baseline, candidate]:
        client.ok("POST", "/api/control/1/logo", {"action": "replace", "logo": "replacement-logo.png"})
        xml = client.ok("GET", "/xmltv/1/guide.xml", auth=False, query={"days": 1})
        if "/guide-logo.png" not in xml:
            raise AssertionError("Explicit XMLTV guide icon changed with the on-air replacement")
        config = client.ok("GET", "/api/playout/config/1")
        config["processing"]["guide_logo"] = ""
        client.ok("PUT", "/api/playout/config/1", config)
        xml = client.ok("GET", "/xmltv/1/guide.xml", auth=False, query={"days": 1})
        if "/channel-logo.png" not in xml or "/replacement-logo.png" in xml:
            raise AssertionError("XMLTV fallback did not use the configured model logo")
        client.ok("POST", "/api/control/1/logo", {"action": "show"})
        client.ok("POST", "/api/control/1/logo", {"action": "move", "position": "default"})
    return {"status": "PASS", "comparison": "Exact HTTP status and JSON/text body equality", "cases": cases,
            "case_count": len(cases), "xmltv": {"explicit_guide_icon": "PASS", "configured_model_fallback": "PASS"}}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate", default="http://127.0.0.1:8787")
    parser.add_argument("--baseline", default="http://127.0.0.1:18787")
    parser.add_argument("--work-root", type=Path, required=True)
    parser.add_argument("--baseline-work-root", type=Path)
    parser.add_argument("--auth-file", type=Path, required=True)
    parser.add_argument("--evidence", type=Path, required=True)
    args = parser.parse_args()
    if os.getenv("NOAR_E2E_STAGING") != "1" or os.getenv("NOAR_E2E_DISPOSABLE") != "1":
        raise SystemExit("Explicit disposable staging opt-in is required")
    root = args.work_root.resolve()
    credentials = json.loads(args.auth_file.read_text())
    candidate = Client(args.candidate, credentials)
    baseline = Client(args.baseline, credentials)
    seed(candidate, root, False)
    seed(baseline, args.baseline_work_root.resolve() if args.baseline_work_root else root, True)
    result = run(candidate, baseline, root)
    args.evidence.write_text(json.dumps(result, indent=2) + "\n")
    print(f"PASS: {result['case_count']} exact legacy HTTP comparisons and both XMLTV icon cases")


if __name__ == "__main__":
    main()
