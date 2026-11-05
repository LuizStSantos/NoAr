import unittest
from pathlib import Path
import shutil
import subprocess
import tempfile
import time

from picture import FRAME_BYTES, PictureMonitor, PictureTracker


class PictureTrackerTests(unittest.TestCase):
    def test_frozen_frames_fail_even_while_new_frames_keep_arriving(self):
        tracker = PictureTracker(0)
        for second in range(15):
            tracker.observe(bytes(FRAME_BYTES), second)
        self.assertEqual(tracker.frames, 15)
        self.assertEqual(tracker.problem(14), 'decoded picture frozen')

    def test_moving_frames_do_not_fail(self):
        tracker = PictureTracker(0)
        for second in range(30):
            tracker.observe(bytes([second]) * FRAME_BYTES, second)
            self.assertIsNone(tracker.problem(second))

    def test_missing_video_is_distinct_from_a_frozen_image(self):
        tracker = PictureTracker(0)
        self.assertEqual(tracker.problem(13), 'no decoded video frames')

    def test_reconnection_does_not_reset_a_missing_frame_deadline(self):
        tracker = PictureTracker(0)
        tracker.observe(bytes(FRAME_BYTES), 1)
        self.assertEqual(tracker.problem(14), 'no decoded video frames')

    def test_only_a_bounded_grace_window_suppresses_a_failure(self):
        tracker = PictureTracker(0)
        for second in range(40):
            tracker.observe(bytes(FRAME_BYTES), second)
        self.assertIsNone(tracker.problem(39, grace_until=40))
        self.assertIsNone(tracker.problem(45, grace_until=40))
        tracker.observe(bytes(FRAME_BYTES), 53)
        self.assertEqual(tracker.problem(53, grace_until=40), 'decoded picture frozen')

    def test_low_per_frame_motion_accumulates_against_the_reference(self):
        tracker = PictureTracker(0)
        tracker.observe(bytes(FRAME_BYTES), 0)
        for second in range(1, 16):
            # Each change is small, but it eventually changes most of the image.
            changed = min(FRAME_BYTES, second * 1000)
            tracker.observe(bytes([1]) * changed + bytes(FRAME_BYTES - changed), second)
        self.assertGreater(tracker.changes, 0)
        self.assertIsNone(tracker.problem(15))

    def test_truncated_frame_is_not_counted_as_success(self):
        tracker = PictureTracker(0)
        with self.assertRaises(ValueError):
            tracker.observe(b'partial', 1)
        self.assertEqual(tracker.frames, 0)


@unittest.skipUnless(shutil.which('ffmpeg'), 'Independent FFmpeg receiver is unavailable')
class PictureDecoderTests(unittest.TestCase):
    def exercise(self, frozen):
        with tempfile.TemporaryDirectory(prefix='noar-picture-') as folder:
            directory = Path(folder)
            clip = directory / 'fixture.mkv'
            source = 'color=blue:size=160x90:rate=4' if frozen else 'testsrc2=size=160x90:rate=4'
            subprocess.run(['ffmpeg', '-nostdin', '-hide_banner', '-loglevel', 'error',
                            '-f', 'lavfi', '-i', source, '-t', '1', '-c:v', 'ffv1', str(clip)],
                           check=True, timeout=15)
            monitor = PictureMonitor(str(clip), directory, limit=2.5)
            monitor.start()
            try:
                deadline = time.monotonic() + 8
                while monitor.sample()['frames'] < 2:
                    self.assertLess(time.monotonic(), deadline, 'Receiver did not decode the fixture')
                    time.sleep(0.05)
                end = time.monotonic() + 4
                problems = []
                while time.monotonic() < end:
                    problem = monitor.sample()['problem']
                    if problem:
                        problems.append(problem)
                    time.sleep(0.05)
                sample = monitor.sample()
                self.assertGreaterEqual(sample['frames'], 6)
                if frozen:
                    self.assertIn('decoded picture frozen', problems)
                    self.assertNotIn('no decoded video frames', problems)
                else:
                    self.assertEqual(problems, [])
                    self.assertGreater(sample['changes'], 0)
            finally:
                monitor.close()

    def test_real_decoder_detects_repeated_static_pictures_across_reconnects(self):
        self.exercise(frozen=True)

    def test_real_decoder_accepts_moving_pictures(self):
        self.exercise(frozen=False)


if __name__ == '__main__':
    unittest.main()
