"""Measure how far the sound airs from its picture, on the real output.

The soak's clips carry a marker once a second: a box in the top-left corner,
black except for one white frame, and a beep that starts on that frame (see
marker_clip). An FFmpeg receiver watches the output: blackdetect on the
corner finds each flash, silencedetect each beep, both on the stream's own
timestamps. Each beep is paired with its flash, and the offset (sound minus
picture) is recorded over the whole run, so a drift shows.

Lives and the waiting card carry no marker: they add no measurement. A flash
must be one frame long and a beep short, so their starts are not taken for
markers.
"""
from __future__ import annotations

from collections import deque
import json
from pathlib import Path
import re
import shutil
import subprocess
import threading
import time

RATE = 48_000
# Frame of each second with the marker, away from where a clip starts.
FLASH_FRAME = 10
# EBU R37: the sound may lead its picture by up to 40 ms or follow it by 60 ms.
EARLY, LATE = 0.040, 0.060
# Offsets outside that window in a row before it counts as a problem.
VIOLATIONS = 3
# A flash is one frame, a beep one frame of sound: anything longer is not a marker.
MARK = 0.15
# A beep pairs with the flash nearest to it, at most this far.
PAIR = 0.5
# Offsets averaged at the start and at the end, for the drift.
EDGE = 20
# Enough pairs to say anything.
MIN_PAIRS = 20
# FFmpeg prints the detected times with six significant digits: from 1000 s
# on, to 10 ms only. A receiver open longer starts over, so its clock stays
# under that and the offsets keep their millisecond.
RECEIVER_SECONDS = 600

BLACK = re.compile(r'black_start:\s*([-\d.]+)\s+black_end:\s*([-\d.]+)')
SILENCE_START = re.compile(r'silence_start:\s*([-\d.]+)')
SILENCE_END = re.compile(r'silence_end:\s*([-\d.]+)')

RECEIVER_FILTER = ('[0:v:0]crop=iw/16:ih/16:0:0,blackdetect=d=0.3:pix_th=0.10:pic_th=0.98[v];'
                   '[0:a:0]silencedetect=n=-30dB:d=0.3[a]')


def marker_clip(ffmpeg: str, path: str, seconds: int, width: int, height: int, fps: int,
                audio: bool) -> list[str]:
    """The FFmpeg command for a moving test clip with the marker."""
    flash = (f'testsrc2=s={width}x{height}:r={fps}:d={seconds},format=yuv420p,'
             'drawbox=x=0:y=0:w=iw/8:h=ih/8:color=black:t=fill,'
             f"drawbox=x=0:y=0:w=iw/8:h=ih/8:color=white:t=fill:enable='eq(mod(n\\,{fps})\\,{FLASH_FRAME})'")
    command = [ffmpeg, '-nostdin', '-v', 'error', '-y', '-f', 'lavfi', '-i', flash]
    if audio:
        start = RATE * FLASH_FRAME // fps
        end = start + RATE // fps
        beep = (f"aevalsrc='if(gte(mod(n\\,{RATE})\\,{start})*lt(mod(n\\,{RATE})\\,{end})"
                f"\\,0.5*sin(2*PI*1000*t)\\,0)':s={RATE}:d={seconds}:c=stereo")
        command += ['-f', 'lavfi', '-i', beep]
    command += ['-c:v', 'libx264', '-preset', 'ultrafast', '-g', str(fps * 2), '-pix_fmt', 'yuv420p']
    if audio:
        command += ['-c:a', 'aac', '-b:a', '128k', '-shortest']
    return command + [path]


class SyncTracker:
    """Pairs flashes and beeps read from the receiver's log."""

    def __init__(self):
        self.black_end = None
        self.silence_end = None
        self.flashes = deque()
        self.beeps = deque()
        # (seconds since the start, offset in seconds)
        self.offsets: list[tuple[float, float]] = []
        self.violations = 0
        self.worst_run: list[float] = []

    def reset(self) -> None:
        """A new receiver: its timestamps start over."""
        self.black_end = self.silence_end = None
        self.flashes.clear()
        self.beeps.clear()

    def line(self, text: str, elapsed: float) -> None:
        if match := BLACK.search(text):
            start, end = float(match[1]), float(match[2])
            # The black around the previous flash ended one frame ago: it was one.
            if self.black_end is not None and 0 < start - self.black_end <= MARK:
                self._add(self.flashes, self.black_end, elapsed)
            self.black_end = end
        elif match := SILENCE_END.search(text):
            self.silence_end = float(match[1])
        elif match := SILENCE_START.search(text):
            start = float(match[1])
            if self.silence_end is not None and 0 < start - self.silence_end <= MARK:
                self._add(self.beeps, self.silence_end, elapsed)
            self.silence_end = None

    def _add(self, events: deque, time_: float, elapsed: float) -> None:
        events.append(time_)
        while self.flashes and self.beeps:
            flash, beep = self.flashes[0], self.beeps[0]
            if abs(beep - flash) <= PAIR:
                self.flashes.popleft()
                self.beeps.popleft()
                self._record(beep - flash, elapsed)
            elif flash < beep:
                self.flashes.popleft()  # a flash without sound (a clip without audio)
            else:
                self.beeps.popleft()

    def _record(self, offset: float, elapsed: float) -> None:
        self.offsets.append((round(elapsed, 3), offset))
        if -EARLY <= offset <= LATE:
            self.violations = 0
        else:
            self.violations += 1
            if self.violations >= VIOLATIONS:
                self.worst_run = [offset for _, offset in self.offsets[-self.violations:]]

    def problem(self) -> str | None:
        if self.worst_run:
            shown = ', '.join(f'{offset * 1000:+.0f}' for offset in self.worst_run[-VIOLATIONS:])
            return f'sound out of sync with the picture ({shown} ms, outside -40..+60 ms)'
        return None

    def snapshot(self) -> dict:
        values = [offset for _, offset in self.offsets]
        if not values:
            return {'pairs': 0}

        def mean(items):
            return round(sum(items) / len(items) * 1000, 2)

        return {
            'pairs': len(values),
            'offset_ms': {'min': round(min(values) * 1000, 2), 'max': round(max(values) * 1000, 2),
                          'mean': mean(values), 'last': round(values[-1] * 1000, 2)},
            # Positive: the sound came later at the end than at the start.
            'drift_ms': round(mean(values[-EDGE:]) - mean(values[:EDGE]), 2),
            'outside_window': sum(not -EARLY <= value <= LATE for value in values),
        }


class SyncMonitor:
    """Continuously measure a local test output, reconnecting as needed."""

    def __init__(self, url: str, directory: Path, receiver_seconds: float = RECEIVER_SECONDS):
        self.url = url
        self.directory = directory
        self.receiver_seconds = receiver_seconds
        self.ffmpeg = shutil.which('ffmpeg')
        if not self.ffmpeg:
            raise RuntimeError('A/V sync checks require ffmpeg on the test host')
        self.tracker = SyncTracker()
        self.started = time.monotonic()
        self.lock = threading.Lock()
        self.stop = threading.Event()
        self.process = None
        self.thread = None
        self.attempts = 0

    def command(self) -> list[str]:
        return [self.ffmpeg, '-nostdin', '-hide_banner', '-nostats', '-loglevel', 'info',
                '-threads', '1', '-i', self.url, '-filter_complex', RECEIVER_FILTER,
                '-map', '[v]', '-map', '[a]', '-f', 'null', '-']

    def start(self) -> None:
        self.directory.mkdir(parents=True, exist_ok=True)
        self.thread = threading.Thread(target=self._run, name='av-sync', daemon=True)
        self.thread.start()

    def _run(self) -> None:
        with (self.directory / 'av-sync.jsonl').open('w') as samples:
            while not self.stop.is_set():
                with self.lock:
                    if self.stop.is_set():
                        break
                    self.attempts += 1
                    self.tracker.reset()
                    self.process = subprocess.Popen(self.command(), stdout=subprocess.DEVNULL,
                                                    stderr=subprocess.PIPE, text=True, errors='replace')
                    process = self.process
                    opened = time.monotonic()
                try:
                    for text in process.stderr:
                        if self.stop.is_set() or time.monotonic() - opened > self.receiver_seconds:
                            break
                        with self.lock:
                            before = len(self.tracker.offsets)
                            self.tracker.line(text, time.monotonic() - self.started)
                            added = self.tracker.offsets[before:]
                        for elapsed, offset in added:
                            samples.write(json.dumps({'elapsed': elapsed, 'offset_ms': round(offset * 1000, 2)}) + '\n')
                            samples.flush()
                finally:
                    self._end_process(process)
                    process.stderr.close()
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

    def sample(self) -> dict:
        with self.lock:
            return {**self.tracker.snapshot(), 'attempts': self.attempts, 'problem': self.tracker.problem()}

    def close(self) -> None:
        self.stop.set()
        with self.lock:
            process = self.process
        if process is not None:
            self._end_process(process)
        if self.thread is not None:
            self.thread.join(timeout=10)
            if self.thread.is_alive():
                raise RuntimeError('A/V sync observer did not stop')
