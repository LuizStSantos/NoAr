"""Observe decoded pictures, independently of transport byte counters.

Intended only for the soak's known-moving synthetic sources. A real programme
may legitimately hold a still picture; this is not a production freeze alarm.
"""
from __future__ import annotations

import json
from pathlib import Path
import shutil
import subprocess
import threading
import time

WIDTH, HEIGHT = 160, 90
FRAME_BYTES = WIDTH * HEIGHT
FRAME_RATE = 2
FREEZE_LIMIT = 12.0
MIN_CHANGE = 0.5  # Mean absolute difference in 8-bit grayscale levels.


class PictureTracker:
    def __init__(self, started: float, limit: float = FREEZE_LIMIT):
        self.started = started
        self.limit = limit
        self.last_frame = None
        self.last_change = None
        self.reference = None
        self.frames = 0
        self.changes = 0
        self.delta = None

    def observe(self, frame: bytes, now: float) -> None:
        if len(frame) != FRAME_BYTES:
            raise ValueError('Decoded frame has an unexpected size')
        if self.reference is None:
            self.reference = frame
            self.last_change = now
        else:
            self.delta = sum(abs(a - b) for a, b in zip(frame, self.reference)) / FRAME_BYTES
            if self.delta >= MIN_CHANGE:
                # Compare against the last changed image: slow movement can
                # accumulate, rather than being dismissed on every frame.
                self.reference = frame
                self.last_change = now
                self.changes += 1
        self.last_frame = now
        self.frames += 1

    def problem(self, now: float, grace_until: float = 0) -> str | None:
        if now < grace_until:
            return None
        last_frame = self.last_frame if self.last_frame is not None else self.started
        if now - max(last_frame, grace_until) > self.limit:
            return 'no decoded video frames'
        last_change = self.last_change if self.last_change is not None else self.started
        if self.frames and now - max(last_change, grace_until) > self.limit:
            return 'decoded picture frozen'
        return None

    def snapshot(self, now: float) -> dict:
        return {
            'frames': self.frames, 'changes': self.changes,
            'frame_age': None if self.last_frame is None else round(now - self.last_frame, 3),
            'change_age': None if self.last_change is None else round(now - self.last_change, 3),
            'mean_pixel_delta': None if self.delta is None else round(self.delta, 3),
        }


class PictureMonitor:
    """Continuously decode a local test output, reconnecting without resetting deadlines."""

    def __init__(self, url: str, directory: Path, name: str = 'output', limit: float = FREEZE_LIMIT):
        self.url = url
        self.directory = directory
        self.name = name
        self.ffmpeg = shutil.which('ffmpeg')
        if not self.ffmpeg:
            raise RuntimeError('Decoded-picture checks require ffmpeg on the test host')
        self.tracker = PictureTracker(time.monotonic(), limit)
        self.lock = threading.Lock()
        self.stop = threading.Event()
        self.process = None
        self.thread = None
        self.attempts = 0

    def start(self) -> None:
        self.directory.mkdir(parents=True, exist_ok=True)
        self.thread = threading.Thread(target=self._run, name=f'picture-{self.name}', daemon=True)
        self.thread.start()

    def _run(self) -> None:
        with (self.directory / f'picture-{self.name}.decoder.log').open('a') as errors, \
                (self.directory / f'picture-{self.name}.jsonl').open('w') as samples:
            while not self.stop.is_set():
                command = [self.ffmpeg, '-nostdin', '-hide_banner', '-loglevel', 'warning',
                           '-threads', '1', '-i', self.url, '-map', '0:v:0', '-an',
                           '-vf', f'fps={FRAME_RATE},scale={WIDTH}:{HEIGHT},format=gray',
                           '-threads', '1', '-f', 'rawvideo', 'pipe:1']
                with self.lock:
                    if self.stop.is_set():
                        break
                    self.attempts += 1
                    self.process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=errors)
                    process = self.process
                try:
                    while not self.stop.is_set():
                        frame = process.stdout.read(FRAME_BYTES)
                        if len(frame) != FRAME_BYTES:
                            break
                        now = time.monotonic()
                        with self.lock:
                            self.tracker.observe(frame, now)
                            sample = {'elapsed': round(now - self.tracker.started, 3),
                                      **self.tracker.snapshot(now)}
                        samples.write(json.dumps(sample) + '\n')
                        samples.flush()
                finally:
                    self._end_process(process)
                    process.stdout.close()
                    with self.lock:
                        self.process = None
                self.stop.wait(1)

    @staticmethod
    def _end_process(process) -> None:
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=3)

    def sample(self, grace_until: float = 0) -> dict:
        now = time.monotonic()
        with self.lock:
            return {**self.tracker.snapshot(now), 'attempts': self.attempts,
                    'problem': self.tracker.problem(now, grace_until)}

    def close(self) -> None:
        self.stop.set()
        with self.lock:
            process = self.process
        if process is not None:
            self._end_process(process)
        if self.thread is not None:
            self.thread.join(timeout=10)
            if self.thread.is_alive():
                raise RuntimeError('Decoded-picture observer did not stop')
