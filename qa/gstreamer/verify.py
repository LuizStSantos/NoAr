#!/usr/bin/env python3
"""Observe one SRT reader/publisher, then inspect the captured media itself."""

import argparse
from array import array
from collections import Counter
from datetime import datetime, timezone
import json
import os
import platform
from pathlib import Path
import subprocess
import sys
import threading
import time
import urllib.request


ARTIFACTS = Path("/artifacts")
OPENER = urllib.request.build_opener(urllib.request.ProxyHandler({}))


def api(path):
    with OPENER.open("http://mediamtx:9997/v3/" + path, timeout=2) as response:
        return json.load(response)


def read_events():
    path = ARTIFACTS / "events.jsonl"
    if not path.exists():
        return []
    records = []
    for line in path.read_text().splitlines():
        try:
            records.append(json.loads(line))
        except json.JSONDecodeError:
            # A streaming writer may still be completing the last line.
            continue
    return records


def packet_metrics(probe):
    result = {}
    for kind in ("video", "audio"):
        stream = next(item for item in probe["streams"] if item["codec_type"] == kind)
        timestamps = [float(item["dts_time"]) for item in probe["packets"]
                      if item["stream_index"] == stream["index"] and "dts_time" in item]
        if len(timestamps) < 2:
            raise ValueError(f"Insufficient {kind} packets")
        gaps = [right - left for left, right in zip(timestamps, timestamps[1:])]
        result[kind] = {
            "packets": len(timestamps), "dts_regressions": sum(gap <= 0 for gap in gaps),
            "maximum_gap_seconds": round(max(gaps), 6),
            "first_dts": timestamps[0], "last_dts": timestamps[-1],
            "duration_seconds": round(timestamps[-1] - timestamps[0], 6),
        }
    return result


def connection_metrics(snapshots):
    result = {}
    for state in ("publish", "read"):
        identities = {item["id"] for snapshot in snapshots for item in snapshot["items"]
                      if item.get("path") == "output" and item.get("state") == state}
        records = [item for snapshot in snapshots for item in snapshot["items"]
                   if item.get("path") == "output" and item.get("state") == state]
        result[state] = {
            "unique_ids": sorted(identities), "count": len(identities),
            "drop_statistics_available": bool(records) and all(
                "packetsSendDrop" in item and "packetsReceivedDrop" in item for item in records),
            "maximum_send_drops": max((item.get("packetsSendDrop", 0) for item in records), default=0),
            "maximum_receive_drops": max((item.get("packetsReceivedDrop", 0) for item in records), default=0),
        }
    return result


def decode_markers(capture):
    pixels = subprocess.run([
        "ffmpeg", "-v", "error", "-i", str(capture), "-an", "-vf",
        "fps=4:start_time=0,crop=64:64:0:0,scale=1:1", "-pix_fmt", "rgb24", "-f", "rawvideo", "-",
    ], check=True, capture_output=True).stdout
    colors = Counter()
    video_sequence = []
    for offset in range(0, len(pixels), 3):
        red, green, blue = pixels[offset:offset + 3]
        name = None
        if red > max(green, blue) * 1.8 and red > 80:
            name = "a"
        elif blue > max(red, green) * 1.8 and blue > 80:
            name = "b"
        elif green > max(red, blue) * 1.8 and green > 50:
            name = "live"
        if name:
            colors[name] += 1
            if not video_sequence or video_sequence[-1] != name:
                video_sequence.append(name)

    pcm = subprocess.run([
        "ffmpeg", "-v", "error", "-i", str(capture), "-vn", "-ac", "1", "-ar", "48000",
        "-f", "s16le", "-",
    ], check=True, capture_output=True).stdout
    samples = array("h")
    samples.frombytes(pcm)
    if sys.byteorder != "little":
        samples.byteswap()
    tones = Counter()
    audio_sequence = []
    for start in range(0, len(samples) - 12000, 12000):
        window = samples[start:start + 12000]
        if max(window) - min(window) < 500:
            continue
        frequency = sum(left <= 0 < right for left, right in zip(window, window[1:])) * 4
        for name, expected in (("a", 440), ("b", 660), ("live", 880)):
            if abs(frequency - expected) < 24:
                tones[name] += 1
                if not audio_sequence or audio_sequence[-1] != name:
                    audio_sequence.append(name)
    return {"video_quarter_seconds": dict(colors), "audio_quarter_seconds": dict(tones),
            "video_sequence": video_sequence, "audio_sequence": audio_sequence}


def contains_sequence(actual, expected):
    remaining = iter(actual)
    return all(any(value == wanted for value in remaining) for wanted in expected)


def evaluate(packets, connections, markers, events, recovery):
    checks = {}
    for kind, maximum in (("video", 0.12), ("audio", 0.10)):
        metrics = packets[kind]
        checks[f"{kind}_timestamps"] = (
            metrics["dts_regressions"] == 0 and metrics["maximum_gap_seconds"] <= maximum)
    checks["av_end_alignment"] = abs(packets["video"]["last_dts"] - packets["audio"]["last_dts"]) < 0.2
    checks["single_output_publisher"] = connections["publish"]["count"] == 1
    checks["single_output_reader"] = connections["read"]["count"] == 1
    checks["no_srt_packet_drops"] = all(
        connections[state]["drop_statistics_available"] for state in ("publish", "read")) and all(
        connections[state][metric] == 0 for state in ("publish", "read")
        for metric in ("maximum_send_drops", "maximum_receive_drops"))
    checks["three_video_sources"] = all(markers["video_quarter_seconds"].get(name, 0) >= 4
                                         for name in ("a", "b", "live"))
    checks["three_audio_sources"] = all(markers["audio_quarter_seconds"].get(name, 0) >= 4
                                         for name in ("a", "b", "live"))
    start = next(item for item in events if item["event"] == "start")
    cycles = start["cycles"]
    expected = ["a", "b", "live", "b"] * cycles
    checks["decoded_video_transitions"] = contains_sequence(markers["video_sequence"], expected)
    checks["decoded_audio_transitions"] = contains_sequence(markers["audio_sequence"], expected)
    checks["live_inputs_opened_and_closed"] = (
        sum(item["event"] == "live_open" for item in events) == cycles and
        sum(item["event"] == "live_closed" for item in events) == cycles)
    checks["all_scheduled_live_entries"] = sum(
        item["event"] == "switch" and item.get("source") == "live" for item in events) >= cycles
    checks["demo_completed"] = any(item["event"] == "finished" and item["status"] == "PASS"
                                    for item in events)
    checks["no_pipeline_error"] = not any(item["event"] == "error" for item in events)
    unexpected = [item for item in events if item["event"] in ("live_error", "switch_unavailable") or
                  item.get("reason") == "live_stalled"]
    checks["expected_live_behavior"] = recovery or not unexpected
    if recovery:
        fallbacks = [item for item in events if item["event"] == "switch" and
                     item.get("reason") in ("live_error", "live_stalled")]
        checks["source_loss_fallback"] = bool(fallbacks)
        checks["live_returns_after_failure"] = bool(fallbacks) and any(
            item["event"] == "switch" and item.get("source") == "live" and
            item["elapsed"] > fallbacks[0]["elapsed"] for item in events)
        checks["fault_was_injected"] = (ARTIFACTS / "fault.json").exists()
    return {name: "PASS" if passed else "FAIL" for name, passed in checks.items()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--recovery", action="store_true")
    args = parser.parse_args()
    if os.environ.get("NOAR_GST_STAGING") != "1":
        parser.error("This verifier is restricted to the disposable prototype")
    report = {"status": "FAIL", "checks": {}, "started_utc": datetime.now(timezone.utc).isoformat(),
              "scenario": "source_loss" if args.recovery else "normal"}
    capture = ARTIFACTS / "capture.ts"
    process = None
    try:
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline:
            events = read_events()
            if any(item["event"] == "error" for item in events):
                raise RuntimeError("Prototype pipeline failed; inspect events.jsonl")
            starts = [item for item in events if item["event"] == "start"]
            if starts and any(item.get("path") == "output" and item.get("state") == "publish"
                              for item in api("srtconns/list")["items"]):
                break
            time.sleep(0.2)
        else:
            raise RuntimeError("Output did not become ready within 15 s")
        # Leave two seconds at the end so the capture finishes before playout closes.
        elapsed = max(item["elapsed"] for item in events)
        duration = starts[0]["duration"] - elapsed - 2
        if duration < 12:
            raise RuntimeError("Verifier joined too late; rerun the disposable test")
        snapshots = []
        progress = []

        def observe_progress():
            for line in process.stdout:
                if line.startswith("out_time_us="):
                    try:
                        timestamp = int(line.partition("=")[2])
                    except ValueError:
                        continue
                    if not progress or timestamp > progress[-1][1]:
                        progress.append((time.monotonic(), timestamp))

        with (ARTIFACTS / "capture.log").open("w") as log:
            process = subprocess.Popen([
                "ffmpeg", "-hide_banner", "-loglevel", "warning", "-y",
                "-stats_period", "0.1", "-progress", "pipe:1",
                "-probesize", "131072", "-analyzeduration", "1000000",
                "-i", "srt://mediamtx:8890?streamid=read:output", "-map", "0:v:0",
                "-map", "0:a:0", "-c", "copy", "-t", str(duration), "-f", "mpegts", str(capture),
            ], stdout=subprocess.PIPE, stderr=log, text=True)
            progress_thread = threading.Thread(target=observe_progress, daemon=True)
            progress_thread.start()
            deadline = time.monotonic() + duration + 15
            while process.poll() is None and time.monotonic() < deadline:
                snapshots.append(api("srtconns/list"))
                time.sleep(0.25)
            if process.poll() is None:
                raise RuntimeError("Capture exceeded its deadline")
            if process.returncode:
                raise RuntimeError("SRT capture failed; inspect capture.log")
            progress_thread.join(timeout=2)
        (ARTIFACTS / "connections.json").write_text(json.dumps(snapshots, indent=2) + "\n")
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline:
            events = read_events()
            if any(item["event"] == "finished" for item in events):
                break
            time.sleep(0.2)
        probe = json.loads(subprocess.run([
            "ffprobe", "-v", "error", "-show_streams", "-show_packets",
            "-show_entries", "stream=index,codec_type:packet=stream_index,dts_time",
            "-of", "json", str(capture),
        ], capture_output=True, text=True, check=True).stdout)
        report["packets"] = packet_metrics(probe)
        report["connections"] = connection_metrics(snapshots)
        report["markers"] = decode_markers(capture)
        report["versions"] = {
            "gstreamer": starts[0]["gstreamer"], "python": platform.python_version(),
            "packages": subprocess.run([
                "dpkg-query", "-W", "ffmpeg", "libsrt1.5-gnutls", "gstreamer1.0-plugins-bad",
            ], check=True, capture_output=True, text=True).stdout.splitlines(),
        }
        report["checks"] = evaluate(report["packets"], report["connections"],
                                     report["markers"], events, args.recovery)
        maximum_gap = max((right[0] - left[0] for left, right in zip(progress, progress[1:])),
                          default=0)
        report["receiver_progress"] = {"samples": len(progress),
                                       "maximum_gap_seconds": round(maximum_gap, 6)}
        report["checks"]["receiver_progress"] = "PASS" if len(progress) >= 20 and maximum_gap < 0.75 else "FAIL"
        decoded = subprocess.run([
            "ffmpeg", "-v", "error", "-xerror", "-i", str(capture),
            "-map", "0:v:0", "-map", "0:a:0", "-f", "null", "-",
        ], capture_output=True, text=True, timeout=60)
        (ARTIFACTS / "decode.log").write_text(decoded.stderr)
        report["checks"]["decode"] = "PASS" if decoded.returncode == 0 and not decoded.stderr else "FAIL"
        report["status"] = "PASS" if all(value == "PASS" for value in report["checks"].values()) else "FAIL"
    except (OSError, ValueError, KeyError, StopIteration, RuntimeError, subprocess.SubprocessError) as error:
        report["error"] = str(error)
    finally:
        if process is not None and process.poll() is None:
            process.kill()
            process.wait()
    report["limitations"] = ["Independent Python prototype; no NoAr engine or scheduler integration",
                              "Files advance on the clock; Continue mode is not implemented",
                              "VLC observation and long soak require separate validation"]
    (ARTIFACTS / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    return int(report["status"] != "PASS")


if __name__ == "__main__":
    sys.exit(main())
