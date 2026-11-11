import shutil
import subprocess
import tempfile
import time
import unittest
from pathlib import Path

from sync import RECEIVER_FILTER, SyncMonitor, SyncTracker, marker_clip


def feed(tracker: SyncTracker, lines: list[str]) -> None:
    for text in lines:
        tracker.line(text, 0)


def black(start: float, end: float) -> str:
    return f'[blackdetect @ 0x1] black_start:{start} black_end:{end} black_duration:{end - start:.3f}'


def marker(second: float, sound: float) -> list[str]:
    """The receiver's lines around the marker of one second."""
    return [
        black(second - 0.56, second + 0.40),
        f'[silencedetect @ 0x2] silence_end: {second + 0.40 + sound} | silence_duration: 0.96',
        f'[silencedetect @ 0x2] silence_start: {second + 0.44 + sound}',
    ]


class SyncTrackerTests(unittest.TestCase):
    def test_each_beep_pairs_with_its_flash(self):
        tracker = SyncTracker()
        for second in range(5):
            feed(tracker, marker(second, 0.021))
        # The flash of a second is confirmed by the black after it.
        feed(tracker, [black(4.44, 5.40)])
        snapshot = tracker.snapshot()
        self.assertEqual(snapshot['pairs'], 5)
        self.assertEqual(snapshot['offset_ms']['mean'], 21.0)
        self.assertEqual(snapshot['drift_ms'], 0.0)
        self.assertIsNone(tracker.problem())

    def test_a_live_starting_after_black_and_silence_is_no_marker(self):
        tracker = SyncTracker()
        # The waiting card ends: the black and the silence end for good.
        feed(tracker, [black(10.0, 20.0), '[silencedetect @ 0x2] silence_end: 20.0 | silence_duration: 10'])
        # Back to a clip after 60 s of live: neither was a short marker.
        feed(tracker, [black(80.0, 80.4), '[silencedetect @ 0x2] silence_start: 80.0'])
        self.assertEqual(tracker.snapshot()['pairs'], 0)

    def test_a_flash_without_sound_is_skipped(self):
        tracker = SyncTracker()
        feed(tracker, [black(0.0, 0.40), black(0.44, 1.40)])  # a clip without audio
        feed(tracker, marker(2, 0.0) + [black(2.44, 3.40)])
        self.assertEqual(tracker.snapshot()['pairs'], 1)

    def test_three_offsets_in_a_row_outside_ebu_r37_are_a_problem(self):
        tracker = SyncTracker()
        for second, sound in enumerate([0.02, 0.09, 0.02, 0.09, 0.09, 0.09]):
            feed(tracker, marker(second, sound))
        feed(tracker, [black(5.44, 6.40)])
        self.assertEqual(tracker.snapshot()['outside_window'], 4)
        self.assertIn('+90, +90, +90 ms', tracker.problem())

    def test_a_drift_compares_the_end_with_the_start(self):
        tracker = SyncTracker()
        for second in range(60):
            feed(tracker, marker(second, 0.001 * second))
        feed(tracker, [black(59.44, 60.40)])
        self.assertEqual(tracker.snapshot()['drift_ms'], 40.0)


@unittest.skipUnless(shutil.which('ffmpeg'), 'needs ffmpeg')
class SyncMonitorTests(unittest.TestCase):
    def test_the_marker_clip_measures_in_sync_through_a_real_decoder(self):
        for fps in (25, 30):
            with self.subTest(fps=fps), tempfile.TemporaryDirectory(prefix='noar-sync-unit-') as directory:
                clip = Path(directory) / 'marker.mp4'
                subprocess.run(marker_clip(shutil.which('ffmpeg'), str(clip), 8, 640, 360, fps, True),
                               check=True, capture_output=True, timeout=120)
                monitor = SyncMonitor(str(clip), Path(directory))
                monitor.start()
                deadline = time.monotonic() + 60
                while monitor.sample()['pairs'] < 7 and time.monotonic() < deadline:
                    time.sleep(0.2)
                monitor.close()
                sample = monitor.sample()
                self.assertGreaterEqual(sample['pairs'], 7, sample)
                self.assertLess(abs(sample['offset_ms']['max']), 2, sample)
                self.assertLess(abs(sample['offset_ms']['min']), 2, sample)
                self.assertIsNone(sample['problem'])


@unittest.skipUnless(shutil.which('ffmpeg'), 'needs ffmpeg')
class SyncReceiverTests(unittest.TestCase):
    def test_the_receiver_starts_over_so_its_times_keep_their_millisecond(self):
        with tempfile.TemporaryDirectory(prefix='noar-sync-unit-') as directory:
            clip = Path(directory) / 'marker.mp4'
            subprocess.run(marker_clip(shutil.which('ffmpeg'), str(clip), 4, 320, 180, 25, True),
                           check=True, capture_output=True, timeout=120)
            # A live-like input that never ends: the clip read at its own pace, again and again.
            monitor = SyncMonitor(str(clip), Path(directory), receiver_seconds=1)
            monitor.command = lambda: [monitor.ffmpeg, '-nostdin', '-hide_banner', '-nostats',
                                       '-loglevel', 'info', '-re', '-stream_loop', '-1', '-i', str(clip),
                                       '-filter_complex', RECEIVER_FILTER,
                                       '-map', '[v]', '-map', '[a]', '-f', 'null', '-']
            monitor.start()
            deadline = time.monotonic() + 30
            while monitor.sample()['attempts'] < 3 and time.monotonic() < deadline:
                time.sleep(0.2)
            monitor.close()
            self.assertGreaterEqual(monitor.sample()['attempts'], 3)


if __name__ == '__main__':
    unittest.main()
