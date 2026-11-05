"""Test harness decisions without starting containers or contacting a server."""
import importlib.util
import os
from pathlib import Path
import unittest
from unittest.mock import patch


class HlsProgressTests(unittest.TestCase):
    def setUp(self):
        spec = importlib.util.spec_from_file_location('soak_under_test', Path(__file__).with_name('soak.py'))
        self.soak = importlib.util.module_from_spec(spec)
        # Import only: main() remains guarded; no process/API calls are made.
        with patch.dict(os.environ, {'NOAR_SOAK': '1', 'NOAR_SOAK_OUTPUT': 'hls'}):
            spec.loader.exec_module(self.soak)

    def test_rotating_media_sequence_counts_as_progress_with_constant_playlist_size(self):
        first = '#EXTM3U\n#EXT-X-MEDIA-SEQUENCE:1\n#EXTINF:2,\nsegment1.ts\n'
        second = first.replace('SEQUENCE:1', 'SEQUENCE:2').replace('segment1', 'segment2')
        self.assertEqual(len(first), len(second))
        with patch.object(self.soak, 'request', side_effect=[(200, first), (200, first), (200, second)]):
            initial = self.soak.output_bytes()
            self.assertEqual(initial, self.soak.output_bytes())
            self.assertNotEqual(initial, self.soak.output_bytes())

    def test_master_playlist_without_media_does_not_count_as_output_progress(self):
        with patch.object(self.soak, 'request', return_value=(200, '#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=100\nlow.m3u8\n')):
            self.assertIsNone(self.soak.output_bytes())

    def test_error_page_does_not_count_as_hls_progress(self):
        with patch.object(self.soak, 'request', return_value=(503, 'Unavailable')):
            self.assertIsNone(self.soak.output_bytes())


if __name__ == '__main__':
    unittest.main()
