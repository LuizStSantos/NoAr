#!/usr/bin/env python3
"""Verify the picture observer against real, disposable NoAr SRT/HLS outputs.

Each output airs moving synthetic material (must pass) and an intentionally
static fixture (must be detected while decoded frames and /health keep flowing).
Requires NOAR_SOAK=1 and an explicit current binary. Never targets an existing API.
"""
from __future__ import annotations

import argparse
import datetime as dt
import json
import os
from pathlib import Path
import secrets
import shutil
import socket
import subprocess
import tempfile
import time
import urllib.error
import urllib.request

from picture import FREEZE_LIMIT, PictureMonitor

ROOT = Path(__file__).resolve().parents[2]
BASE = 'http://127.0.0.1:18787'


class Api:
    def __init__(self):
        self.token = ''
        self.opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))

    def call(self, path, data=None, method=None):
        headers = {'Content-Type': 'application/json'}
        if self.token:
            headers['Authorization'] = 'Bearer ' + self.token
        request = urllib.request.Request(BASE + path, headers=headers, method=method,
                                         data=None if data is None else json.dumps(data).encode())
        try:
            with self.opener.open(request, timeout=10) as response:
                body = response.read().decode()
                try:
                    return json.loads(body)
                except ValueError:
                    return body
        except urllib.error.HTTPError as error:
            raise RuntimeError(f'{method or "GET"} {path}: {error.code} {error.read().decode()}') from error


def scenario(binary, directory, clip, output, frozen, seconds):
    directory.mkdir()
    data = directory / 'data'
    for folder in ('db', 'media', 'playlists', 'public', 'logs'):
        (data / folder).mkdir(parents=True)
    shutil.copyfile(clip, data / 'media/fixture.mp4')
    env = {**os.environ, 'DB': str(data / 'db/noar.db'), 'TZ': 'UTC'}
    for key, folder in [('STORAGE', 'media'), ('PLAYLISTS', 'playlists'), ('PUBLIC', 'public'), ('LOGS', 'logs')]:
        env[f'NOAR_SETUP_{key}'] = str(data / folder)
    process = monitor = None
    result = {'output': output, 'fixture': 'frozen' if frozen else 'moving', 'passed': False}
    try:
        with (directory / 'noar.log').open('w') as log:
            process = subprocess.Popen([str(binary), '--listen', '127.0.0.1:18787', '--log-to-console'],
                                       env=env, stdout=log, stderr=subprocess.STDOUT)
            api = Api()
            deadline = time.monotonic() + 20
            while True:
                if process.poll() is not None:
                    raise RuntimeError('NoAr exited before setup')
                try:
                    initial = api.call('/api/setup')
                    break
                except (OSError, urllib.error.URLError):
                    if time.monotonic() >= deadline:
                        raise RuntimeError('NoAr setup did not become ready')
                    time.sleep(0.2)
            if not initial['required'] or initial['settings']['storage'] != str(data / 'media'):
                raise RuntimeError('Refusing initialized or unexpected storage')
            password = secrets.token_urlsafe(24)
            api.call('/api/setup', {**initial['settings'], 'shared': False, 'username': 'picture-test',
                                   'password': password, 'mail': 'picture@example.invalid',
                                   'two_factor': False}, 'POST')
            api.token = api.call('/auth/login', {'username': 'picture-test', 'password': password}, 'POST')['access']
            channel = api.call('/api/channel/1')
            channel['timezone'] = 'UTC'
            api.call('/api/channel/1', channel, 'PATCH')
            config = api.call('/api/playout/config/1')
            selected = next(item for item in api.call('/api/playout/outputs/1')
                            if item['name'] == ('hls' if output == 'hls' else 'stream'))
            config['output'].update({
                'id': selected['id'], 'mode': 'hls' if output == 'hls' else 'stream',
                'stream_type': 'srt', 'stream_url': 'srt://127.0.0.1:18990?mode=listener&latency=200000',
                'hls_playlist_name': 'stream', 'hls_segment_duration': 2, 'hls_list_size': 6, 'hls_variants': [],
                'width': 640, 'height': 360, 'fps': 25, 'video_codec': 'libx264', 'audio_codec': 'aac',
                'audio_bitrate': 128,
                'muxer_options': {}, 'metadata_options': {}, 'protocol_options': {},
            })
            config['output']['video_options']['preset'] = 'ultrafast'
            config['processing']['add_logo'] = False
            now = dt.datetime.now(dt.timezone.utc)
            config['playlist'].update({'day_start': (now - dt.timedelta(seconds=1)).strftime('%H:%M:%S'),
                                       'length': '24:00:00', 'infinit': True})
            config['recording']['enable'] = False
            api.call('/api/playout/config/1', config, 'PUT')
            api.call('/api/playlist/1', {'channel': channel['name'], 'date': now.date().isoformat(),
                                        'program': [{'source': str(data / 'media/fixture.mp4'),
                                                     'in': 0, 'out': 60, 'duration': 60}]}, 'POST')
            api.call('/api/control/1/process', {'command': 'start'}, 'POST')
            url = (BASE + '/public/1/live/stream.m3u8' if output == 'hls'
                   else 'srt://127.0.0.1:18990?mode=caller&latency=200000')
            monitor = PictureMonitor(url, directory)
            monitor.start()
            deadline = time.monotonic() + 25
            while monitor.sample()['frames'] == 0:
                if time.monotonic() >= deadline or process.poll() is not None:
                    raise RuntimeError('No decoded frames from the actual NoAr output')
                time.sleep(0.2)
            # Start the measurement after receiver acquisition, using the same
            # deadline semantics as the soak's bounded startup grace.
            grace_until = time.monotonic()
            end = grace_until + seconds
            problems, observations = [], []
            while time.monotonic() < end:
                sample = monitor.sample(grace_until)
                observations.append(sample)
                if sample['problem'] and sample['problem'] not in problems:
                    problems.append(sample['problem'])
                if process.poll() is not None:
                    raise RuntimeError('NoAr exited while observing pictures')
                time.sleep(0.5)
            health = api.call('/health')
            final = monitor.sample(grace_until)
            result.update({'picture': final, 'health': health, 'problems': problems,
                           'observation_seconds': seconds})
            if frozen:
                result['passed'] = (problems == ['decoded picture frozen'] and final['frames'] >= 20
                                    and final['frame_age'] < 4 and health['status'] == 'ok')
            else:
                result['passed'] = (not problems and final['frames'] >= 20 and final['changes'] >= 6
                                    and health['status'] == 'ok')
            (directory / 'observations.json').write_text(json.dumps(observations, indent=2) + '\n')
    except Exception as error:
        result['error'] = str(error)
    finally:
        if monitor is not None:
            monitor.close()
        if process is not None:
            process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=5)
        shutil.rmtree(data)
    (directory / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result), flush=True)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--outputs', nargs='+', choices=['srt', 'hls'], default=['srt', 'hls'])
    parser.add_argument('--seconds', type=float, default=18)
    args = parser.parse_args()
    if os.environ.get('NOAR_SOAK') != '1':
        parser.error('NOAR_SOAK=1 is required for disposable media-output tests')
    if args.seconds < FREEZE_LIMIT + 2:
        parser.error('--seconds must exceed the real freeze threshold by at least two seconds')
    binary = args.binary.resolve()
    if not binary.is_file():
        parser.error('--binary must name a current Linux NoAr executable')
    for port, kind in ((18787, socket.SOCK_STREAM), (18990, socket.SOCK_DGRAM)):
        with socket.socket(socket.AF_INET, kind) as check:
            if kind == socket.SOCK_STREAM:
                check.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            check.bind(('127.0.0.1', port))
    (ROOT / 'work').mkdir(exist_ok=True)
    directory = Path(tempfile.mkdtemp(prefix='picture-', dir=ROOT / 'work'))
    print(f'Disposable picture evidence: {directory}', flush=True)
    clips = {}
    for name, source in [('moving', 'testsrc2=size=640x360:rate=25'),
                         ('frozen', 'color=c=blue:size=640x360:rate=25')]:
        clips[name] = directory / f'{name}.mp4'
        subprocess.run(['ffmpeg', '-nostdin', '-hide_banner', '-loglevel', 'error',
                        '-f', 'lavfi', '-i', source, '-f', 'lavfi', '-i', 'sine=frequency=440:sample_rate=48000',
                        '-t', '60', '-c:v', 'libx264', '-preset', 'ultrafast', '-g', '50',
                        '-pix_fmt', 'yuv420p', '-c:a', 'aac', '-shortest', str(clips[name])], check=True)
    results = []
    try:
        for output in args.outputs:
            for frozen in (False, True):
                name = 'frozen' if frozen else 'moving'
                results.append(scenario(binary, directory / f'{output}-{name}', clips[name], output,
                                        frozen, args.seconds))
    finally:
        for clip in clips.values():
            clip.unlink(missing_ok=True)
    report = {'passed': all(result['passed'] for result in results), 'results': results,
              'ndi': {'status': 'NOT_AVAILABLE', 'reason': 'NDI runtime/receiver not provided'},
              'sdi': {'status': 'NOT_AVAILABLE', 'reason': 'DeckLink output/capture hardware not provided'},
              'scope': 'Short native observer verification, not a 90-minute release-image soak'}
    (directory / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    return 0 if report['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
