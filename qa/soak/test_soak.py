"""Test harness decisions without starting containers or contacting a server."""
import importlib.util
import copy
import json
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import unittest
from unittest.mock import MagicMock, patch

from graphics_cycle import GraphicsCycler


def load_soak(**environment):
    spec = importlib.util.spec_from_file_location('soak_under_test', Path(__file__).with_name('soak.py'))
    soak = importlib.util.module_from_spec(spec)
    with patch.dict(os.environ, {'NOAR_SOAK': '1', 'NOAR_SOAK_OUTPUT': 'hls',
                                'NOAR_SOAK_MODE': 'docker', 'NOAR_SOAK_MINUTES': '90',
                                'NOAR_SOAK_GRAPHICS': '0', **environment}):
        spec.loader.exec_module(soak)
    return soak


class HlsProgressTests(unittest.TestCase):
    def setUp(self):
        self.soak = load_soak()

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


class NativeAdapterTests(unittest.TestCase):
    def setUp(self):
        self.soak = load_soak(NOAR_SOAK_MODE='native')

    def test_guard_and_duration_validation_remain_mandatory(self):
        for environment in [{'NOAR_SOAK': '0'}, {'NOAR_SOAK_MODE': 'unknown'},
                            {'NOAR_SOAK_MINUTES': 'nan'}, {'NOAR_SOAK_MINUTES': '-1'}]:
            with self.subTest(environment=environment), self.assertRaises(SystemExit):
                load_soak(**environment)

    def test_native_database_and_setup_paths_override_inherited_installation(self):
        with tempfile.TemporaryDirectory(prefix='noar-soak-unit-') as directory:
            self.soak.WORK = Path(directory)
            with patch.dict(os.environ, {'DB': '/existing/noar.db', 'NOAR_SETUP_STORAGE': '/existing/media'}):
                env = self.soak.native_environment()
            self.assertEqual(env['DB'], str(Path(directory) / 'data/db/noar.db'))
            self.assertEqual(env['NOAR_SETUP_STORAGE'], str(Path(directory) / 'data/media'))
            self.assertEqual(env['TZ'], 'UTC')
            sources = self.soak.playlist('2026-11-09')['program']
            self.assertTrue(all(Path(item['source']).is_relative_to(Path(directory) / 'data/media')
                                for item in sources))

    def test_work_directory_is_fresh_and_cannot_be_reused(self):
        with tempfile.TemporaryDirectory(prefix='noar-soak-unit-') as directory:
            self.soak.WORK = Path(directory) / 'new run'
            with patch.object(self.soak, 'native_preflight', return_value=('/noar', '/mediamtx', '/gst')):
                self.soak.prepare_work()
                self.assertTrue((self.soak.WORK / 'owned-run.json').is_file())
                with self.assertRaises(FileExistsError):
                    self.soak.prepare_work()

    def test_native_preflight_refuses_repository_artifacts_and_missing_explicit_directory(self):
        with patch.dict(os.environ, {'NOAR_SOAK_DIR': ''}):
            with self.assertRaisesRegex(RuntimeError, 'requires NOAR_SOAK_DIR'):
                self.soak.native_preflight()
        self.soak.WORK = self.soak.HERE / 'artifacts/native'
        with patch.dict(os.environ, {'NOAR_SOAK_DIR': str(self.soak.WORK)}):
            with self.assertRaisesRegex(RuntimeError, 'outside the repository'):
                self.soak.native_preflight()

    def test_native_fixtures_and_publishers_never_invoke_docker(self):
        with tempfile.TemporaryDirectory(prefix='noar-soak-unit-') as directory:
            self.soak.WORK = Path(directory) / 'path with spaces'
            self.soak.native = MagicMock(gst='/tools/gst-launch-1.0')
            with patch.object(self.soak.subprocess, 'run') as run, \
                    patch.object(self.soak, 'docker') as docker, patch.object(self.soak, 'log'):
                self.soak.make_clips(self.soak.WORK / 'data/media')
                for invocation in run.call_args_list:
                    command = invocation.args[0]
                    self.assertEqual(command[0], '/tools/gst-launch-1.0')
                    self.assertTrue(command[-1].startswith('location=' + str(self.soak.WORK)))
                    self.assertIn('path with spaces', command[-1])
                self.soak.act('publish udp-audio')
                self.soak.act('unpublish')
                self.soak.native.start.assert_called_once()
                self.assertIn('udpsink', self.soak.native.start.call_args.args[1])
                self.soak.native.stop.assert_called_once_with('publisher')
                docker.assert_not_called()

    def test_cleanup_targets_only_owned_process_groups_and_reaps_children(self):
        with tempfile.TemporaryDirectory(prefix='noar-soak-unit-') as directory:
            owner = self.soak.NativeProcesses(Path(directory))
            child = MagicMock(pid=12345)
            with patch.object(self.soak.subprocess, 'Popen', return_value=child) as popen, \
                    patch.object(self.soak.os, 'killpg') as killpg:
                owner.start('publisher', ['/tools/gst', '-q'])
                self.assertTrue(popen.call_args.kwargs['start_new_session'])
                owner.stop('unowned')
                killpg.assert_not_called()
                child.wait.side_effect = [subprocess.TimeoutExpired('gst', 8), None, None]
                self.assertEqual(owner.close(), [])
                self.assertEqual(killpg.call_args_list[0].args, (12345, signal.SIGTERM))
                self.assertIn((12345, signal.SIGKILL), [call.args for call in killpg.call_args_list])
                self.assertEqual(owner.processes, {})

    def test_docker_playlist_and_fixture_commands_keep_container_paths(self):
        self.soak.MODE = 'docker'
        with tempfile.TemporaryDirectory(prefix='noar-soak-unit-') as directory:
            self.soak.WORK = Path(directory)
            with patch.object(self.soak.subprocess, 'run') as run:
                self.soak.make_clips(self.soak.WORK / 'data/media')
                command = run.call_args.args[0]
                self.assertEqual(command[0], 'docker')
                self.assertEqual(command[-1], 'location=/data/media/c.mp4')
            self.assertEqual(self.soak.playlist('2026-11-09')['program'][0]['source'], '/data/media/a.mp4')

    def test_early_setup_or_observer_failure_still_cleans_owned_processes_and_fails_duration(self):
        with tempfile.TemporaryDirectory(prefix='noar-soak-unit-') as directory:
            self.soak.WORK = Path(directory)
            self.soak.native = MagicMock()
            self.soak.native.close.return_value = []
            observer = MagicMock()
            observer.sample.return_value = {'problem': None}
            observer.tracker.frames = observer.tracker.changes = 0
            observer.close.side_effect = RuntimeError('observer failed to close')
            with patch.object(self.soak, 'prepare_work'), \
                    patch.object(self.soak, 'PictureMonitor', return_value=observer), \
                    patch.object(self.soak, 'set_up', side_effect=RuntimeError('setup failed')), \
                    patch.object(self.soak.signal, 'signal'), patch('builtins.print'):
                self.assertEqual(self.soak.main(), 1)
            self.soak.native.close.assert_called_once_with()
            report = json.loads((Path(directory) / 'report.json').read_text())
            self.assertFalse(report['passed'])
            self.assertEqual(report['measurement_seconds'], 0)
            self.assertTrue(any('duration was not completed' in incident['what'] for incident in report['incidents']))
            self.assertTrue(any('observer cleanup failed' in incident['what'] for incident in report['incidents']))


class FakeGraphicsApi:
    def __init__(self):
        self.models, self.on, self.takes = {}, {}, []
        self.calls = []

    def __call__(self, path, data=None, method=None):
        self.calls.append((path, copy.deepcopy(data), method))
        if path == '/api/graphics/1/templates' and method == 'POST':
            model = {**copy.deepcopy(data), 'id': len(self.models) + 1}
            self.models[model['id']] = model
            return 200, copy.deepcopy(model)
        if '/templates/' in path:
            model_id = int(path.split('/')[5])
            if path.endswith('/assignment'):
                self.models[model_id]['role'] = data['role']
            elif method == 'PUT':
                self.models[model_id] = copy.deepcopy(data)
            return 200, copy.deepcopy(self.models[model_id])
        if '/channel/' in path:
            self.on[int(path.rsplit('/', 1)[1])] = data['on']
        if '/channel' in path:
            return 200, [{'template': copy.deepcopy(model), 'on': self.on.get(model_id, False), 'ready': True}
                         for model_id, model in self.models.items()]
        if path.endswith('/take'):
            take = {'template_id': data['template_id'], 'take_id': len(self.calls)}
            self.takes.append(take)
            return 200, {'state': 'on_air', 'take_id': take['take_id']}
        if path.endswith('/out'):
            self.takes = [take for take in self.takes if take['take_id'] != data['take_id']]
            return 200, {'on_air': copy.deepcopy(self.takes)}
        if path.endswith('/on-air'):
            return 200, copy.deepcopy(self.takes)
        raise AssertionError((path, data, method))


class GraphicsCycleTests(unittest.TestCase):
    def test_real_model_contract_cycles_all_roles_and_records_every_api_response(self):
        with tempfile.TemporaryDirectory(prefix='noar-soak-unit-') as directory:
            directory = Path(directory)
            server = FakeGraphicsApi()
            cycle = GraphicsCycler(server, directory, directory)
            cycle.setup()
            for _ in range(16):
                cycle.tick()
            self.assertEqual(cycle.steps, 16)
            self.assertEqual({model.get('role') for model in server.models.values()},
                             {'logo', 'lshape', 'ticker', None})
            self.assertEqual(server.takes, [])
            self.assertTrue(any(path.endswith('/out') for path, _, _ in server.calls))
            self.assertTrue(any(path.endswith('/take') for path, _, _ in server.calls))
            ticker = server.models[cycle.models['ticker']['id']]['layers'][0]
            self.assertIn('15', ticker['text'])
            self.assertEqual(ticker['scroll'], {'direction': 'left', 'speed': 80, 'gap': 80})
            receipts = [json.loads(line) for line in (directory / 'graphics-receipts.jsonl').read_text().splitlines()]
            self.assertEqual(len(receipts), len(server.calls))
            self.assertEqual(cycle.snapshot()['receipts'], len(receipts))
            self.assertTrue(all(receipt['status'] == 200 for receipt in receipts))
            self.assertEqual((directory / 'soak-logo-a.png').read_bytes()[:8], b'\x89PNG\r\n\x1a\n')

    def test_http_failure_is_receipted_and_cannot_count_as_completed_work(self):
        with tempfile.TemporaryDirectory(prefix='noar-soak-unit-') as directory:
            cycle = GraphicsCycler(lambda *args: (409, {'error': 'unready'}), Path(directory), Path(directory))
            with self.assertRaisesRegex(RuntimeError, '409'):
                cycle.setup()
            receipt = json.loads((Path(directory) / 'graphics-receipts.jsonl').read_text())
            self.assertEqual(receipt['status'], 409)
            self.assertEqual(cycle.steps, 0)

    def test_control_state_mismatch_fails_even_when_http_succeeds(self):
        with tempfile.TemporaryDirectory(prefix='noar-soak-unit-') as directory:
            cycle = GraphicsCycler(lambda *args: (200, [{'template': {'id': 1}, 'on': False, 'ready': True}]),
                                  Path(directory), Path(directory))
            cycle.models['logo'] = {'id': 1}
            with self.assertRaisesRegex(RuntimeError, 'did not acknowledge'):
                cycle.control('logo', True)

    def test_authored_overlay_regions_cannot_cover_the_moving_video_roi(self):
        with tempfile.TemporaryDirectory(prefix='noar-soak-unit-') as directory:
            server = FakeGraphicsApi()
            cycle = GraphicsCycler(server, Path(directory), Path(directory))
            cycle.setup()
            # Apply the GC's default follow-picture transform as well as its full-frame bounds.
            for role, model in cycle.models.items():
                for layer in model['layers']:
                    if role == 'lshape':
                        continue
                    for scale in ([1, .75] if role == 'gc' else [1]):
                        x, y = layer['x'] * scale, layer['y'] * scale
                        right, bottom = x + layer['width'] * scale, y + layer['height'] * scale
                        self.assertTrue(right <= 10 or x >= 45 or bottom <= 15 or y >= 50, (role, layer))
            video = cycle.models['lshape']['layers'][1]
            self.assertLessEqual(video['x'], 10)
            self.assertLessEqual(video['y'], 15)
            self.assertGreaterEqual(video['x'] + video['width'], 45)
            self.assertGreaterEqual(video['y'] + video['height'], 50)


if __name__ == '__main__':
    unittest.main()
