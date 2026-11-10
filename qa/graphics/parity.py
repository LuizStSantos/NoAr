#!/usr/bin/env python3
"""Compare actual migrated application frames using the fixed stage-d criteria."""
import argparse
from decimal import Decimal
import hashlib
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import time

import numpy as np
from PIL import Image

from compatibility import Client


WIDTH, HEIGHT, FPS, FRAMES = 640, 360, 25, 1000
REGIONS = {"logo": (548, 12, 80, 48), "lshape": (0, 0, 640, 360), "ticker": (0, 300, 640, 60)}


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def prepare(root, source_database):
    media = root / "media"
    (media / "Graphics").mkdir(parents=True)
    (root / "public").mkdir()
    (root / "logs").mkdir()
    (root / "playlists").mkdir()
    (root / ".qa-graphics-owned").touch()
    art = np.zeros((HEIGHT, WIDTH, 3), dtype=np.uint8)
    art[:] = (24, 56, 120)
    art[::40, :] = (220, 160, 32)
    art[:, ::40] = (220, 160, 32)
    Image.fromarray(art).save(media / "Graphics/art.png")
    logo = np.full((48, 80, 3), (224, 160, 32), dtype=np.uint8)
    logo[4:44:8, 4:76] = (32, 160, 224)
    Image.fromarray(logo).save(media / "Graphics/logo.png")
    process = subprocess.Popen([
        "ffmpeg", "-hide_banner", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgb24",
        "-s", "640x360", "-r", "25", "-i", "pipe:0", "-f", "lavfi", "-i",
        "sine=frequency=440:sample_rate=48000:duration=40", "-t", "40", "-c:v", "libx264",
        "-preset", "ultrafast", "-crf", "0", "-pix_fmt", "yuv420p", "-c:a", "aac", "-y",
        str(media / "deterministic.mp4")], stdin=subprocess.PIPE)
    for index in range(FRAMES):
        frame = np.full((HEIGHT, WIDTH, 3), 80, dtype=np.uint8)
        frame[::40, :] = 180
        frame[:, ::40] = 180
        bits = [1, 0, 1, 1] + [(index >> shift) & 1 for shift in range(11, -1, -1)]
        for bit, value in enumerate(bits):
            frame[96:120, 160 + bit * 20:180 + bit * 20] = 235 if value else 16
        process.stdin.write(frame.tobytes())
    process.stdin.close()
    assert process.wait() == 0
    (media / "deterministic.marks.json").write_text(json.dumps({
        "hide": [{"from": 8, "to": 10}], "lshape": [{"from": 14, "to": 20}],
    }) + "\n")
    database = root / "fixture-27.db"
    source = sqlite3.connect(source_database)
    target = sqlite3.connect(database)
    source.backup(target)
    source.close()
    assert target.execute("SELECT max(version) FROM _sqlx_migrations").fetchone()[0] == 27
    target.execute("UPDATE config_global SET storage=?,playlists=?,public=?,logs=?",
                   tuple(str(root / part) for part in ["media", "playlists", "public", "logs"]))
    target.execute("UPDATE channels SET storage=?,playlists=?,public=?,timezone='UTC',active=1",
                   tuple(str(root / part) for part in ["media", "playlists", "public"]))
    target.execute("UPDATE config_processing SET mode='folder',logo='Graphics/logo.png',add_logo=1,"
                   "logo_scale='80:48',logo_position='W-w-12:12',logo_opacity=0.6,"
                   "logo_transition_in='fade',logo_transition_out='fade',logo_transition_seconds=1,"
                   "lshape='Graphics/art.png',lshape_scale=70,lshape_corner='top-right'")
    target.execute("UPDATE config_storage SET extensions='mp4',shuffle=0")
    target.execute("UPDATE config_output SET active=0")
    target.execute("UPDATE config_output SET active=1,width=640,height=360,fps=25,"
                   "hls_playlist_name='parity',hls_segment_duration=2,hls_list_size=50,hls_variants='' "
                   "WHERE config_id=1 AND name='hls'")
    target.execute("DELETE FROM graphics_events")
    target.commit()
    target.execute("PRAGMA wal_checkpoint(TRUNCATE)")
    target.close()
    for name in ["main", "branch"]:
        directory = root / name
        (directory / "raw").mkdir(parents=True)
        shutil.copy2(database, directory / "noar.db")
    (root / "inputs.json").write_text(json.dumps({
        "database_sha256": digest(database),
        "files": {str(path.relative_to(root)): digest(path) for path in media.rglob("*") if path.is_file()},
        "format": "640x360 YUV420P, 25 fps, 1000 frames; raw planes exclude stride padding",
    }, indent=2) + "\n")


def run(root, name, binary, credentials):
    directory = root / name
    for part in ["logs", "public"]:
        shutil.rmtree(root / part)
        (root / part).mkdir()
    assert digest(directory / "noar.db") == digest(root / "fixture-27.db")
    command = ["docker", "exec", "-e", f"DB={directory}/noar.db", "-e", "NOAR_E2E_DISPOSABLE=1",
               "-e", f"NOAR_QA_CAPTURE_DIR={directory}/raw", "noar-dev", binary,
               "-l", "0.0.0.0:8787", "--fake-time", "2026-11-06T00:00:00+00:00"]
    health = []
    with (directory / "server.log").open("w") as stream:
        process = subprocess.Popen(command, stdout=stream, stderr=subprocess.STDOUT)
        try:
            client = None
            for _ in range(60):
                try:
                    if client is None:
                        client = Client("http://127.0.0.1:8787", credentials)
                    if client.ok("GET", "/health")["status"] == "ok":
                        break
                except (OSError, RuntimeError):
                    pass
                time.sleep(0.5)
            else:
                raise AssertionError(f"{name} did not become healthy")
            deadline = time.monotonic() + 90
            while not (directory / "raw/000999.yuv").is_file():
                assert time.monotonic() < deadline, f"{name}: missing final pre-encode frame"
                sample = client.ok("GET", "/health")
                assert sample["status"] == "ok", sample
                health.append(sample)
                time.sleep(0.5)
            time.sleep(3)
            health.append(client.ok("GET", "/health"))
            client.ok("POST", "/api/control/1/process", {"command": "stop"})
            subprocess.run(["docker", "exec", "noar-dev", "sh", "-c",
                            f"kill -TERM $(pgrep -f '^{binary} -l')"], check=True)
            assert process.wait(timeout=20) == 0
        finally:
            if process.poll() is None:
                subprocess.run(["docker", "exec", "noar-dev", "sh", "-c",
                                f"kill -TERM $(pgrep -f '^{binary} -l')"], check=False)
                process.wait(timeout=20)
    shutil.copytree(root / "public", directory / "public")
    shutil.copytree(root / "logs", directory / "logs")
    (directory / "health.json").write_text(json.dumps(health, indent=2) + "\n")
    log = (directory / "logs/noar_1.log").read_text()
    assert "ERROR" not in log, f"{name}: channel log contains errors"
    database = sqlite3.connect(directory / "noar.db")
    version = database.execute("SELECT max(version) FROM _sqlx_migrations").fetchone()[0]
    assert version == (27 if name == "main" else 34)
    if name == "branch":
        enabled, layers = database.execute(
            "SELECT b.enabled,t.layers FROM graphics_channel_bindings b JOIN graphics_templates t "
            "ON t.id=b.template_id WHERE b.role='ticker' AND b.channel_id=1").fetchone()
        assert not enabled and not json.loads(layers)
    database.close()
    print("Captured", name, flush=True)


def decoded(directory):
    playlist = directory / "public/live/parity.m3u8"
    text = playlist.read_text()
    snapshot = playlist.with_name("snapshot.m3u8")
    snapshot.write_text(text if "#EXT-X-ENDLIST" in text else text + "\n#EXT-X-ENDLIST\n")
    metadata = json.loads(subprocess.check_output([
        "ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries",
        "frame=best_effort_timestamp_time", "-of", "json", str(snapshot)]))
    times = [Decimal(item["best_effort_timestamp_time"]) for item in metadata["frames"]]
    assert len(times) >= FRAMES
    origin = times[0]
    for index, stamp in enumerate(times[:FRAMES]):
        assert stamp - origin == Decimal(index) / FPS, (index, stamp, origin)
    process = subprocess.Popen(["ffmpeg", "-hide_banner", "-loglevel", "error", "-i", str(snapshot),
                                "-frames:v", str(FRAMES), "-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1"],
                               stdout=subprocess.PIPE)
    return process, str(origin), [str(t - origin) for t in times[:FRAMES]]


def read_frame(process):
    size = WIDTH * HEIGHT * 3
    data = bytearray()
    while len(data) < size:
        part = process.stdout.read(size - len(data))
        assert part, "HLS decoder ended before the final matched PTS"
        data.extend(part)
    return np.frombuffer(data, dtype=np.uint8).reshape(HEIGHT, WIDTH, 3)


def compare(root, screenshots):
    evidence = {"status": "FAIL", "criteria": "qa/graphics/parity-criteria.md", "frames": [],
                "first_raw_divergence": None, "first_hls_divergence": None}
    expected = {f"{index:06}.yuv" for index in range(FRAMES)}
    for name in ["main", "branch"]:
        assert {path.name for path in (root / name / "raw").glob("*.yuv")} == expected
    raw_differences = []
    for index in range(FRAMES):
        name = f"{index:06}.yuv"
        before = (root / "main/raw" / name).read_bytes()
        after = (root / "branch/raw" / name).read_bytes()
        assert len(before) == len(after) == WIDTH * HEIGHT * 3 // 2
        if before != after:
            a, b = np.frombuffer(before, np.uint8), np.frombuffer(after, np.uint8)
            raw_differences.append({"pts": index, "changed_bytes": int(np.count_nonzero(a != b)),
                                    "max_error": int(np.abs(a.astype(np.int16) - b).max())})
    evidence["raw_differences"] = raw_differences
    evidence["first_raw_divergence"] = raw_differences[0] if raw_differences else None
    screenshots.mkdir(parents=True, exist_ok=True)
    main, main_origin, main_times = decoded(root / "main")
    branch, branch_origin, branch_times = decoded(root / "branch")
    assert main_times == branch_times
    evidence["container_origins"] = {"main": main_origin, "branch": branch_origin}
    samples = {0, 175, 187, 200, 250, 262, 275, 350, 355, 360, 365, 370, 375,
               380, 420, 470, 475, 480, 485, 490, 495, 500, 999}
    for index in range(FRAMES):
        a, b = read_frame(main), read_frame(branch)
        delta = np.abs(a.astype(np.int16) - b)
        measurements = {}
        for region, (x, y, width, height) in REGIONS.items():
            error = delta[y:y + height, x:x + width]
            mean, percentile, maximum = float(error.mean()), float(np.percentile(error, 99.9)), int(error.max())
            passed = mean <= 0.75 and percentile <= 8 and maximum <= 32
            measurements[region] = {"mae": mean, "p99_9": percentile, "max": maximum, "pass": passed}
            if not passed and evidence["first_hls_divergence"] is None:
                evidence["first_hls_divergence"] = {"pts": index, "region": region, **measurements[region]}
        evidence["frames"].append({"pts": index, "seconds": main_times[index], "regions": measurements})
        if index in samples or (evidence["first_raw_divergence"] and evidence["first_raw_divergence"]["pts"] == index):
            for suffix, pixels in [("main", a), ("branch", b)]:
                Image.fromarray(pixels).save(screenshots / f"output-parity-d-{index:04}-{suffix}.jpg", quality=92)
            Image.fromarray(np.clip(delta * 8, 0, 255).astype(np.uint8)).save(
                screenshots / f"output-parity-d-{index:04}-diff.png")
        if index in {0, 999}:
            for frame in [a, b]:
                bits = [int(frame[104:112, 168 + bit * 20:172 + bit * 20].mean() > 128) for bit in range(16)]
                assert bits[:4] == [1, 0, 1, 1]
                number = sum(value << (11 - bit) for bit, value in enumerate(bits[4:]))
                assert number == index, (index, number)
    assert main.wait() == branch.wait() == 0
    evidence["status"] = "PASS" if not raw_differences and evidence["first_hls_divergence"] is None else "FAIL"
    evidence["raw_frame_count"] = FRAMES
    evidence["hls_frame_count"] = FRAMES
    evidence["regions"] = REGIONS
    evidence["segments"] = {name: {path.name: digest(path) for path in (root / name / "public/live").glob("*.ts")}
                            for name in ["main", "branch"]}
    indexed_segments = {
        name: {int(Path(path).stem.rsplit("_", 1)[1]): value for path, value in segments.items()}
        for name, segments in evidence["segments"].items()
    }
    common = sorted(indexed_segments["main"].keys() & indexed_segments["branch"].keys())
    evidence["segment_hash_comparison"] = {
        "matching_indices": [index for index in common if indexed_segments["main"][index] == indexed_segments["branch"][index]],
        "different_indices": [index for index in common if indexed_segments["main"][index] != indexed_segments["branch"][index]],
        "tested_interval": "PTS 0-999, 40 seconds; post-roll flushing is outside this interval",
    }
    (root / "evidence.json").write_text(json.dumps(evidence, indent=2) + "\n")
    print(evidence["status"], "exact raw frames and aligned HLS", flush=True)
    assert evidence["status"] == "PASS", (evidence["first_raw_divergence"], evidence["first_hls_divergence"])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["prepare", "run", "compare"])
    parser.add_argument("--work-root", type=Path, required=True)
    parser.add_argument("--source-database", type=Path)
    parser.add_argument("--name", choices=["main", "branch"])
    parser.add_argument("--binary")
    parser.add_argument("--auth-file", type=Path)
    parser.add_argument("--screenshots", type=Path)
    args = parser.parse_args()
    if os.getenv("NOAR_E2E_STAGING") != "1" or os.getenv("NOAR_E2E_DISPOSABLE") != "1":
        raise SystemExit("Explicit disposable staging opt-in is required")
    root = args.work_root.resolve()
    if args.command == "prepare":
        root.mkdir(parents=True, exist_ok=False)
        prepare(root, args.source_database)
    else:
        assert (root / ".qa-graphics-owned").is_file()
        if args.command == "run":
            run(root, args.name, args.binary, json.loads(args.auth_file.read_text()))
        else:
            compare(root, args.screenshots)


if __name__ == "__main__":
    main()
