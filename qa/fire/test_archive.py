"""Regression checks for original HLS evidence across a playout restart."""

import argparse
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from run import FireTest


class ArchiveTests(unittest.TestCase):
    def fixture(self, root):
        (root / '.qa-fire-owned').touch()
        arguments = argparse.Namespace(work_root=root, scenario='full')

        with patch.dict(os.environ, NOAR_E2E_STAGING='1', NOAR_E2E_DISPOSABLE='1'):
            return FireTest(arguments)

    def test_restart_preserves_old_references_without_copying_them_into_the_new_clock(self):
        with tempfile.TemporaryDirectory(prefix='noar-fire-archive-') as directory:
            root = Path(directory)
            runner = self.fixture(root)
            public = root / 'public/live'
            public.mkdir()
            first = ['fire_111_99_0.ts', 'fire_111_99_1.ts']
            next_session = 'fire_222_99_2.ts'

            for name in [*first, next_session]:
                (public / name).write_bytes(b'\x47' + bytes(187))

            playlist = public / 'fire.m3u8'
            playlist.write_text('#EXTM3U\n' + ''.join(f'#EXTINF:2,\n{name}\n' for name in first))
            runner.archive()
            self.assertEqual(len(runner.archived), 2)
            self.assertEqual(runner.generation, 0)

            # The real server resumes a growing list rather than replacing it.
            playlist.write_text(playlist.read_text() + f'#EXT-X-DISCONTINUITY\n#EXTINF:2,\n{next_session}\n')
            runner.archive()
            runner.archive()
            evidence = list(runner.archived.values())
            self.assertEqual([item['generation'] for item in evidence], [0, 0, 1])
            self.assertEqual(runner.generation, 1)
            self.assertEqual(len(list((root / 'archive').glob('*.ts'))), 3)
            self.assertEqual((root / 'archive' / evidence[-1]['file']).read_bytes(), (public / next_session).read_bytes())

    def test_a_playlist_cannot_copy_a_segment_outside_the_owned_fixture(self):
        with tempfile.TemporaryDirectory(prefix='noar-fire-archive-') as directory:
            root = Path(directory)
            runner = self.fixture(root)
            public = root / 'public/live'
            public.mkdir()
            (public / 'fire.m3u8').write_text('#EXTM3U\n#EXTINF:2,\n../../../outside.ts\n')

            with self.assertRaisesRegex(RuntimeError, 'escaped the owned fixture'):
                runner.archive()

            self.assertEqual(runner.archived, {})

    def test_old_native_runtime_is_refused_before_backend_or_media_start(self):
        with tempfile.TemporaryDirectory(prefix='noar-fire-preflight-') as directory:
            runner = self.fixture(Path(directory))

            with patch.object(runner, 'tool', return_value=b'GStreamer 1.26.2\n'), \
                    patch.object(runner, 'create_media') as create_media, \
                    patch.object(runner, 'start') as start:
                with self.assertRaisesRegex(RuntimeError, 'corrected core/base/libav'):
                    runner.setup()

                create_media.assert_not_called()
                start.assert_not_called()

    def test_new_core_with_an_old_decoder_plugin_is_also_refused(self):
        with tempfile.TemporaryDirectory(prefix='noar-fire-preflight-') as directory:
            runner = self.fixture(Path(directory))

            with patch.object(runner, 'tool', side_effect=[b'GStreamer 1.26.11\n', b'  Version 1.26.2\n']), \
                    patch.object(runner, 'start') as start:
                with self.assertRaisesRegex(RuntimeError, 'corrected core/base/libav'):
                    runner.setup()

                start.assert_not_called()


if __name__ == '__main__':
    unittest.main()
