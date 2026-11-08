#!/usr/bin/env python3
"""Exercise actual Scheduler/live/graphics output in a new owned native fixture."""

import argparse
import datetime as dt
import hashlib
import json
import math
import os
from pathlib import Path
import re
import secrets
import shutil
import socket
import subprocess
import sys
import time
import urllib.error
import urllib.request

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / 'qa/soak'))
from picture import PictureMonitor


def write_json(path, value):
    temporary = path.with_suffix('.tmp')
    temporary.write_text(json.dumps(value, indent=2) + '\n')
    temporary.replace(path)


class FireTest:
    def __init__(self, args):
        self.args = args
        self.root = args.work_root.resolve()
        if (os.getenv('NOAR_E2E_STAGING') != '1' or os.getenv('NOAR_E2E_DISPOSABLE') != '1'
                or not (self.root / '.qa-fire-owned').is_file() or (self.root / 'db/noar.db').exists()):
            raise RuntimeError('Use explicit disposable staging and a fresh .qa-fire-owned directory')
        self.base = 'http://127.0.0.1:8787'
        self.opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
        self.token = ''
        self.processes = []
        self.handles = []
        self.publisher = None
        self.focused_udp = args.scenario == 'udp-tracks'
        self.picture = None
        self.epoch = None
        self.archived = {}
        self.hls_sessions = {}
        self.generation = 0
        self.last_output = None
        self.output_changed = time.monotonic()
        self.checkpoints = []
        self.report = {'status': 'RUNNING', 'incidents': [], 'actions': [], 'captures': [],
                       'events': [], 'admitted': {}, 'samples': 0, 'checked_picture_samples': 0}
        for folder in ['db', 'media/Graphics', 'media/Commercials', 'playlists', 'public', 'logs', 'archive', 'captures']:
            (self.root / folder).mkdir(parents=True, exist_ok=True)

    def log(self, text):
        elapsed = None if self.epoch is None else round(time.time() - self.epoch, 1)
        line = f'[{elapsed if elapsed is not None else "setup"}] {text}'
        print(line, flush=True)
        with (self.root / 'run.log').open('a') as handle:
            handle.write(line + '\n')

    def api(self, method, path, body=None, expected=200):
        request = urllib.request.Request(self.base + path, method=method,
            headers={'Content-Type': 'application/json', **({'Authorization': 'Bearer ' + self.token} if self.token else {})},
            data=None if body is None else json.dumps(body).encode())
        try:
            response = self.opener.open(request, timeout=15)
        except urllib.error.HTTPError as error:
            response = error
        raw = response.read().decode()
        if response.code != expected:
            raise RuntimeError(f'{method} {path}: HTTP {response.code}: {raw[:300]}')
        try:
            return json.loads(raw) if raw else None
        except json.JSONDecodeError:
            return raw

    def start(self, name, command, env=None):
        handle = (self.root / f'{name}.log').open('a')
        self.handles.append(handle)
        process = subprocess.Popen(command, stdout=handle, stderr=subprocess.STDOUT, env=env, cwd=REPO)
        self.processes.append(process)
        return process

    @staticmethod
    def stop(process):
        if process is not None and process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=8)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=5)

    def tool(self, *command):
        return subprocess.check_output(command, stderr=subprocess.PIPE, timeout=120)

    def create_media(self):
        for name, seconds, width, height, fps, color, frequency in [
                ('a', 15, 640, 360, 25, 'blue', 440), ('b', 25, 1280, 720, 30, 'red', 660),
                ('c', 20, 854, 480, 25, 'yellow', None), ('ad', 10, 640, 360, 25, 'magenta', 550)]:
            target = self.root / ('media/Commercials' if name == 'ad' else 'media') / f'{name}.mp4'
            source = f'testsrc2=size={width}x{height}:rate={fps},drawbox=x=0:y=0:w=iw*0.15:h=ih*4/15:color={color}:t=fill'
            command = ['ffmpeg', '-v', 'error', '-y', '-f', 'lavfi', '-i', source]
            if frequency:
                command += ['-f', 'lavfi', '-i', f'sine=frequency={frequency}:sample_rate=48000']
            command += ['-t', str(seconds), '-c:v', 'libx264', '-preset', 'ultrafast', '-threads', '2', '-g', str(fps), '-pix_fmt', 'yuv420p']
            command += ['-c:a', 'aac', '-ac', '2'] if frequency else ['-an']
            self.tool(*command, str(target))
        for name, size, color in [('logo-orange', '80x48', '0xe0a020'), ('logo-cyan', '80x48', '0x22aaff'),
                                  ('logo-clip', '80x48', '0xff00ff'), ('channel-art', '640x360', '0x183878'),
                                  ('clip-art', '640x360', '0x285018')]:
            self.tool('ffmpeg', '-v', 'error', '-y', '-f', 'lavfi', '-i', f'color=c={color}:s={size}',
                      '-frames:v', '1', '-threads', '1', str(self.root / f'media/Graphics/{name}.png'))
        self.tool('ffmpeg', '-v', 'error', '-y', '-f', 'lavfi', '-i', 'testsrc2=size=32x32:rate=4',
                  '-t', '1', '-loop', '0', str(self.root / 'media/Graphics/bug.gif'))
        self.log('Created moving mixed-resolution/fps files, a silent clip, commercial, artwork and GIF')

    def model(self, name, layers, **extra):
        return self.api('POST', '/api/graphics/1/templates', {'name': name, 'group': 'Fire test', 'duration': 0, 'layers': layers, **extra})

    def setup(self):
        self.report['runtime'] = {}
        for factory in [None, 'uridecodebin3', 'avdec_h264']:
            output = self.tool('gst-inspect-1.0', *([factory] if factory else ['--version'])).decode()
            pattern = r'\bVersion\s+(\d+)\.(\d+)\.(\d+)' if factory else r'GStreamer (\d+)\.(\d+)\.(\d+)'
            version = re.search(pattern, output)
            if not version or tuple(map(int, version.groups())) < (1, 26, 11):
                raise RuntimeError('The fire test requires the corrected core/base/libav runtime from the NoAr Docker image (1.26.11+)')
            self.report['runtime'][factory or 'core'] = '.'.join(version.groups())
        for port, kind in [(8787, socket.SOCK_STREAM), (9997, socket.SOCK_STREAM), (1935, socket.SOCK_STREAM),
                           (8888, socket.SOCK_STREAM), (8890, socket.SOCK_DGRAM), (5004, socket.SOCK_DGRAM)]:
            with socket.socket(socket.AF_INET, kind) as check:
                check.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
                check.bind(('127.0.0.1', port))
        self.create_media()
        relay = self.start('mediamtx', [str(self.args.mediamtx), str(Path(__file__).with_name('mediamtx.yml'))])
        time.sleep(1)
        if relay.poll() is not None:
            raise RuntimeError('Dedicated MediaMTX exited; inspect its log')
        env = dict(os.environ, DB=str(self.root / 'db/noar.db'), TZ='UTC')
        for key, folder in [('STORAGE', 'media'), ('PLAYLISTS', 'playlists'), ('PUBLIC', 'public'), ('LOGS', 'logs')]:
            env[f'NOAR_SETUP_{key}'] = str(self.root / folder)
        env['no_proxy'] = ','.join(filter(None, [env.get('no_proxy', ''), '127.0.0.1', 'localhost']))
        self.noar = self.start('backend', [str(self.args.binary), '--listen', '0.0.0.0:8787', '--log-to-console'], env)
        deadline = time.monotonic() + 30
        while time.monotonic() < deadline:
            try:
                initial = self.api('GET', '/api/setup')
                break
            except OSError:
                time.sleep(.2)
        else:
            raise RuntimeError('Fresh NoAr did not start')
        assert initial['required'] and Path(initial['settings']['storage']).resolve() == self.root / 'media'
        password = secrets.token_hex(24)
        self.api('POST', '/api/setup', {**initial['settings'], 'username': 'fire-operator', 'password': password,
            'mail': 'fire@example.invalid', 'shared': False, 'two_factor': False})
        self.token = self.api('POST', '/auth/login', {'username': 'fire-operator', 'password': password})['access']
        auth = self.root / 'auth.json'
        auth.write_text(json.dumps({'username': 'fire-operator', 'password': password}))
        auth.chmod(0o600)
        if os.geteuid() == 0:
            owner = REPO.stat()
            for writable in [self.root, self.root / 'captures', auth]:
                os.chown(writable, owner.st_uid, owner.st_gid)
        channel = self.api('GET', '/api/channel/1')
        channel.update(name='Fire Test', timezone='UTC')
        self.api('PATCH', '/api/channel/1', channel)
        # Select the output before authoring models; legacy configuration DTOs use pixels.
        initial_config = self.api('GET', '/api/playout/config/1')
        self.configure_hls(initial_config)
        initial_config['storage']['filler'] = 'a.mp4'
        self.api('PUT', '/api/playout/config/1', initial_config)
        logo = self.api('GET', '/api/graphics/1/templates')[0]
        primary = logo['layers'][0]
        primary.update(image='Graphics/logo-orange.png', opacity=1, x=85.625, y=3.3333333333, width=12.5, height=13.3333333333,
                       animation_in='fade', animation_out='fade', animation_ms=400)
        logo.update(name='Fire logo', group='Fire test', always_on=True, duration=0)
        self.logo = self.api('PUT', f"/api/graphics/1/templates/{logo['id']}", logo)
        self.lshape = self.model('Fire L-shape', [
            {'id': 'art', 'kind': 'image', 'image': 'Graphics/channel-art.png', 'x': 0, 'y': 0, 'width': 100, 'height': 100},
            {'id': 'video', 'kind': 'video', 'x': 25, 'y': 0, 'width': 75, 'height': 75}])
        self.api('PUT', f"/api/graphics/1/templates/{self.lshape['id']}/assignment", {'role': 'lshape', 'control_layer_id': 'art'})
        self.source = self.api('POST', '/api/graphics/1/sources', {'name': 'Fire prices', 'kind': 'csv', 'interval': 30, 'document': 'price\n5.10'})
        self.ticker = self.model('Fire crawl', [{'id': 'crawl', 'kind': 'text', 'text': 'USD {price} | FIRE TEST | Continuous output',
            'x': 5, 'y': 90, 'width': 90, 'height': 8, 'font_size': 5, 'fill': '#101010',
            'scroll': {'direction': 'left', 'speed': 80, 'gap': 80},
            'data_fields': {'price': {'source_id': self.source['id'], 'column': 'price', 'row': 0}}}], always_on=True)
        self.clock = self.model('Fire clock', [{'id': 'clock', 'kind': 'clock', 'x': 5, 'y': 5, 'width': 20,
            'height': 8, 'font_size': 5, 'fill': '#101010', 'clock_timezone': 'UTC'}], always_on=True)
        self.gif = self.model('Fire GIF', [{'id': 'bug', 'kind': 'image', 'image': 'Graphics/bug.gif', 'x': 45,
            'y': 5, 'width': 5, 'height': 9}], always_on=True)
        self.gc = self.model('Fire lower third', [
            {'id': 'panel', 'kind': 'box', 'fill': '#d02020', 'x': 10, 'y': 75, 'width': 60, 'height': 10},
            {'id': 'name', 'kind': 'text', 'text': '{name} - {role}', 'x': 12, 'y': 75, 'width': 56, 'height': 10, 'font_size': 5}])
        self.notice = self.model('Fire scheduled notice', [
            {'id': 'panel', 'kind': 'box', 'fill': '#d02060', 'x': 10, 'y': 15, 'width': 60, 'height': 10},
            {'id': 'title', 'kind': 'text', 'text': 'SCHEDULED NOTICE', 'x': 12, 'y': 15, 'width': 56, 'height': 10, 'font_size': 5}])
        self.api('PUT', '/api/file/1/commercial', {'path': 'Commercials', 'commercial': True}, expected=204)
        self.api('PUT', '/api/file/1/logo-marks', {'path': 'a.mp4', 'hide': [{'from': 5, 'to': 7}],
            'logo_image': 'Graphics/logo-clip.png', 'logo_position': '0%:50%',
            'lshape': [{'from': 7, 'to': 11}], 'lshape_graphic': 'Graphics/clip-art.png',
            'lshape_corner': 'bottom-left', 'lshape_scale': 65})
        config = self.api('GET', '/api/playout/config/1')
        self.configure_hls(config)
        config['processing']['mode'] = 'playlist'
        config['processing']['graphics_scenes'] = [{'name': 'Fire corner', 'logo': 'hide', 'lshape': 'on',
                                                   'lshape_corner': 'bottom-right', 'lshape_scale': 65}]
        self.epoch = math.ceil(time.time()) + (15 if self.focused_udp else 60)
        zero = dt.datetime.fromtimestamp(self.epoch, dt.timezone.utc)
        config['playlist'].update(day_start=zero.strftime('%H:%M:%S'), length='24:00:00', infinit=True)
        config['storage']['filler'] = 'a.mp4'
        config['recording']['enable'] = False
        self.api('PUT', '/api/playout/config/1', config)
        program = [{'source': str(self.root / f'media/{name}.mp4'), 'in': 0, 'out': length, 'duration': length}
                   for _ in range(30) for name, length in [('a', 15), ('b', 25), ('c', 20)]]
        for offset in [-1, 0, 1]:
            self.api('POST', '/api/playlist/1', {'channel': 'Fire Test', 'date': str(zero.date() + dt.timedelta(days=offset)), 'program': program})
        lives = {}
        for protocol, url in [('rtmp', 'rtmp://127.0.0.1:1935/live/feed'), ('udp', 'udp://127.0.0.1:5004'),
                              ('hls', 'http://127.0.0.1:8888/live/feed/index.m3u8')]:
            lives[protocol] = self.api('POST', '/api/schedule/1/lives', {'name': f'Fire {protocol.upper()}', 'url': url})['path']
        events = [(5, 85, 'udp', 'clock')] if self.focused_udp else [
            (270, 390, 'rtmp', 'continue'), (420, 570, 'udp', 'clock'), (750, 810, 'hls', 'clock')]
        for begin, end, protocol, after in events:
            self.schedule(begin, end, f'Fire {protocol.upper()}', [lives[protocol]], 'live', after)
        if not self.focused_udp:
            self.schedule(395, None, 'Fire commercial', ['Commercials/ad.mp4'], 'media', 'continue')
        self.publish('udp' if self.focused_udp else 'srt')
        plan = {'epoch': self.epoch, 'date': zero.date().isoformat(), 'base': self.base, 'channel': 'Fire Test',
            'srt_url': 'srt://127.0.0.1:8890?streamid=read:live/feed&latency=200000',
            'srt_start': self.time_at(30), 'srt_end': self.time_at(210), 'notice_start': self.time_at(150),
            'notice_end': self.time_at(190), 'logo_id': self.logo['id'], 'gc_id': self.gc['id'], 'notice_id': self.notice['id']}
        write_json(self.root / 'ready.json', plan)
        self.log('Actual live inputs and graphics models are prepared')
        if not self.focused_udp:
            self.log('Ready for browser provisioning')
            while not (self.root / 'ui-ready.json').exists():
                if time.time() > self.epoch - 5:
                    raise RuntimeError('Browser did not provision the SRT live and graphics event before the deadline')
                time.sleep(.5)
            ui = json.loads((self.root / 'ui-ready.json').read_text())
            self.report['events'] += [ui['live_event'], ui['graphics_event']]
        self.start('observer', [sys.executable, str(Path(__file__).with_name('observe.py')), str(self.root)])
        while time.time() < self.epoch:
            time.sleep(.1)
        self.api('POST', '/api/control/1/process', {'command': 'start'})
        self.picture = PictureMonitor(self.base + '/public/1/live/fire.m3u8', self.root, name='media-roi',
            video_filter='crop=120:60:240:140,fps=2,scale=160:90,format=gray')
        self.picture.start()
        self.log('Output started; watching a video-only ROI excluding clock, ticker, logo and GC')

    def configure_hls(self, config):
        output = next(item for item in self.api('GET', '/api/playout/outputs/1') if item['name'] == 'hls')
        config['output'].update(id=output['id'], mode='hls', width=640, height=360, fps=25,
            video_codec='libx264', audio_codec='aac', audio_bitrate=128, hls_playlist_name='fire',
            hls_segment_duration=2, hls_list_size=600, hls_variants=[])
        config['output']['video_options']['preset'] = 'ultrafast'

    def time_at(self, seconds):
        return dt.datetime.fromtimestamp(self.epoch + seconds, dt.timezone.utc).strftime('%H:%M:%S')

    def schedule(self, begin, end, name, sources, kind, after):
        event = {'kind': kind, 'title': name, 'sources': sources, 'start_date': dt.datetime.fromtimestamp(self.epoch + begin, dt.timezone.utc).date().isoformat(),
                 'start_time': self.time_at(begin), 'entry_mode': 'interrupt', 'after_mode': after, 'retry_seconds': 120}
        if end is not None:
            event['end_time'] = self.time_at(end)
        saved = self.api('POST', '/api/schedule/1', event)
        self.report['events'].append(saved['id'])

    def publish(self, mode):
        self.stop(self.publisher)
        if mode == 'off':
            self.publisher = None
            return
        command = ['ffmpeg', '-nostdin', '-v', 'warning', '-re']
        if mode != 'udp-audio':
            command += ['-f', 'lavfi', '-i', 'testsrc2=size=640x360:rate=25,drawbox=x=0:y=0:w=96:h=96:color=0x00ff00:t=fill']
        command += ['-f', 'lavfi', '-i', 'sine=frequency=880:sample_rate=48000']
        if mode != 'udp-audio':
            command += ['-c:v', 'libx264', '-preset', 'ultrafast', '-tune', 'zerolatency', '-g', '25', '-threads', '2']
        command += ['-c:a', 'aac', '-ac', '2', '-f', 'mpegts']
        target = 'srt://127.0.0.1:8890?streamid=publish:live/feed&latency=200000' if mode == 'srt' else 'udp://127.0.0.1:5004?pkt_size=1316'
        self.publisher = self.start('publisher-' + mode, [*command, target])

    def archive(self):
        playlist = self.root / 'public/live/fire.m3u8'
        if not playlist.exists():
            return
        text = playlist.read_text()
        entries = []
        duration = 0
        for line in text.splitlines():
            if line.startswith('#EXTINF:'):
                duration = float(line.split(':', 1)[1].split(',')[0])
            elif line and not line.startswith('#'):
                entries.append((line, duration))
        if text != self.last_output:
            self.last_output = text
            self.output_changed = time.monotonic()
        for uri, duration in entries:
            if uri in self.archived:
                continue
            source = (playlist.parent / uri).resolve()
            if not source.is_relative_to(self.root) or source.suffix != '.ts':
                raise RuntimeError('HLS reference escaped the owned fixture')
            if not source.exists():
                continue
            # NoAr preserves previous HLS references on restart. Its unique
            # session prefix identifies the new clock, even if the list grows.
            session = source.stem.rsplit('_', 1)[0]
            generation = self.hls_sessions.setdefault(session, len(self.hls_sessions))
            self.generation = max(self.generation, generation)
            target = self.root / 'archive' / f'g{generation}-{source.name}'
            shutil.copyfile(source, target)
            self.archived[uri] = {'generation': generation, 'file': target.name, 'duration': duration,
                'observed_at': time.time(), 'sha256': hashlib.sha256(target.read_bytes()).hexdigest()}
        write_json(self.root / 'archive/segments.json', list(self.archived.values()))

    def capture(self, name, checks):
        entries = [item for item in self.archived.values() if item['generation'] == self.generation]
        if len(entries) < 2:
            raise RuntimeError('No completed HLS segment for ' + name)
        segment = self.root / 'archive' / entries[-1]['file']
        raw = self.tool('ffmpeg', '-v', 'error', '-i', str(segment), '-vf', r'select=eq(n\,25)', '-frames:v', '1', '-pix_fmt', 'rgb24', '-f', 'rawvideo', 'pipe:1')
        if len(raw) != 640 * 360 * 3:
            raise RuntimeError('Incomplete decoded frame for ' + name)
        measured = []
        for label, x, y, width, height, color, minimum in checks:
            matched = sum(max(abs(raw[(row * 640 + col) * 3 + c] - color[c]) for c in range(3)) <= 25
                          for row in range(y, y + height) for col in range(x, x + width))
            measured.append({'label': label, 'matched_pixels': matched, 'minimum': minimum})
            if matched < minimum:
                self.report['incidents'].append({'at': round(time.time() - self.epoch, 2), 'what': f'{name}: {label} ({matched} pixels, minimum {minimum})'})
        filename = f'output-fire-{name}.jpg'
        self.tool('ffmpeg', '-v', 'error', '-y', '-i', str(segment), '-vf', r'select=eq(n\,25)', '-frames:v', '1', '-q:v', '2', str(self.root / 'captures' / filename))
        self.report['captures'].append({'name': name, 'file': filename, 'segment': segment.name,
            'checks': measured, 'at': time.time() - self.epoch})
        self.log('Captured real HLS: ' + name)

    def action(self, name):
        if name.startswith('publish:'):
            self.publish(name.split(':')[1])
        elif name == 'logo-swap':
            self.api('POST', '/api/control/1/logo', {'action': 'replace', 'logo': 'Graphics/logo-cyan.png'})
        elif name == 'logo-move':
            self.api('POST', '/api/control/1/logo', {'action': 'move', 'position': '100%:50%'})
        elif name == 'logo-reset':
            self.api('POST', '/api/control/1/logo', {'action': 'show'})
            self.api('POST', '/api/control/1/logo', {'action': 'move', 'position': 'default'})
        elif name == 'lshape-on':
            self.api('POST', f"/api/graphics/1/channel/{self.lshape['id']}", {'on': True})
        elif name == 'fullscreen':
            self.api('POST', '/api/control/1/lshape', {'action': 'off'})
            self.api('POST', '/api/control/1/lshape', {'action': 'move', 'position': 'default'})
            self.api('POST', '/api/control/1/logo', {'action': 'show'})
        elif name == 'gc-out':
            for take in self.api('GET', '/api/graphics/1/on-air'):
                if take['template_id'] == self.gc['id']:
                    self.api('POST', '/api/graphics/1/out', {'take_id': take['take_id']})
        elif name == 'data-update':
            self.source = self.api('PUT', f"/api/graphics/1/sources/{self.source['id']}", {
                'name': 'Fire prices', 'kind': 'csv', 'interval': 30, 'revision': self.source['revision'], 'document': 'price\n5.20'})
        elif name == 'scene':
            self.api('POST', '/api/control/1/scene', {'name': 'Fire corner'})
        elif name == 'restart':
            self.api('POST', '/api/control/1/process', {'command': 'restart'})
        elif name == 'gc-burst':
            result = self.api('POST', '/api/graphics/1/take', {'template_id': self.gc['id'], 'duration': 4, 'fields': {'name': 'STRESS', 'role': 'Short take'}})
            assert result['state'] == 'on_air'
        self.report['actions'].append({'at': round(time.time() - self.epoch, 3), 'action': name})
        self.log('Action: ' + name)

    def run(self):
        self.setup()
        orange = ('orange logo', 552, 16, 68, 36, (224, 160, 32), 500)
        green = ('live source', 4, 64, 16, 16, (0, 255, 0), 100)
        actions = [(65, 'logo-swap'), (75, 'logo-move'), (85, 'logo-reset'), (90, 'publish:off'),
            (110, 'publish:srt'), (125, 'lshape-on'), (150, 'data-update'), (165, 'gc-out'), (170, 'fullscreen'),
            (300, 'scene'), (340, 'fullscreen'), (400, 'publish:udp'), (480, 'publish:udp-audio'),
            (510, 'publish:udp'), (580, 'publish:off'), (630, 'restart'), (700, 'publish:srt'), (820, 'publish:off')]
        actions += [(second, 'gc-burst') for second in [835, 845, 855, 865, 875]]
        captures = [(45, 'srt-live', [green, orange]),
            (72, 'logo-swapped', [('cyan logo', 552, 16, 68, 36, (34, 170, 255), 500)]),
            (84, 'logo-moved', [('moved cyan logo', 564, 160, 68, 36, (34, 170, 255), 500)]),
            (104, 'srt-loss', []), (122, 'srt-recovered', [green]),
            (146, 'lshape-and-gc', [('corner live', 164, 50, 16, 16, (0, 255, 0), 100),
                ('GC follows video', 212, 205, 70, 15, (208, 32, 32), 100)]),
            (160, 'scheduled-notice-data', []), (184, 'full-screen-notice', [green]),
            (202, 'notice-ended', [green]),
            (244, 'clip-logo', [('clip logo', 4, 160, 68, 36, (255, 0, 255), 500)]),
            # Short marks are checked over their decoded interval by analyse.py;
            # a wall-clock snapshot can land in the entry/exit animation.
            (254, 'clip-lshape', []),
            (316, 'rtmp-scene', [('scene video', 230, 172, 16, 16, (0, 255, 0), 100)]),
            (348, 'rtmp-fullscreen', [green]), (404, 'commercial', [('commercial', 4, 64, 16, 16, (255, 0, 255), 100)]),
            (450, 'udp-live', [green]), (498, 'udp-audio-only', []), (540, 'udp-recovered', [green]),
            (680, 'after-restart', []), (780, 'hls-live', [green]), (865, 'late-graphics', [])]
        if self.focused_udp:
            actions = [(25, 'publish:udp-audio'), (55, 'publish:udp')]
            captures = [(23, 'udp-before-track-loss', [green]), (45, 'udp-audio-only', []),
                (80, 'udp-after-track-recovery', [green])]
        grace_until = time.monotonic() + max(0, self.epoch - time.time()) + 15
        restart_grace = 0
        next_log = -180
        sample_path = self.root / 'samples.jsonl'
        while time.time() < self.epoch + self.args.seconds:
            elapsed = time.time() - self.epoch
            while actions and elapsed >= actions[0][0]:
                offset, name = actions.pop(0)
                self.action(name)
                if offset == 90:
                    grace_until = max(grace_until, time.monotonic() + 40)
                elif offset == 480 or (self.focused_udp and offset == 25):
                    grace_until = max(grace_until, time.monotonic() + 50)
                elif name == 'restart':
                    restart_grace = time.monotonic() + 45
                    grace_until = max(grace_until, restart_grace)
            if self.noar.poll() is not None:
                self.report['backend_exit_code'] = self.noar.returncode
                raise RuntimeError(f'NoAr exited with code {self.noar.returncode}')
            self.archive()
            health = self.api('GET', '/health')
            current = self.api('GET', '/api/control/1/media/current')
            scheduled = self.api('GET', '/api/schedule/1/status')
            visual = self.picture.sample(grace_until)
            active = scheduled.get('active')
            if active:
                self.report['admitted'].setdefault(str(active['event_id']), {'at': elapsed, 'source': current.get('media', {}).get('source')})
            sample = {'elapsed': round(elapsed, 3), 'health': health, 'media': current,
                'schedule': scheduled, 'picture': visual, 'generation': self.generation}
            with sample_path.open('a') as handle:
                handle.write(json.dumps(sample) + '\n')
            self.report['samples'] += 1
            self.report['elapsed'] = elapsed
            if time.monotonic() >= grace_until:
                self.report['checked_picture_samples'] += 1
            if elapsed >= 15 and time.monotonic() >= restart_grace:
                if health.get('status') != 'ok':
                    self.report['incidents'].append({'at': elapsed, 'what': 'unhealthy output', 'health': health})
                if time.monotonic() - self.output_changed > 12:
                    raise RuntimeError('HLS stopped advancing for more than 12 seconds')
            if visual['problem']:
                raise RuntimeError('Media ROI: ' + visual['problem'])
            while captures and elapsed >= captures[0][0]:
                _, name, checks = captures.pop(0)
                self.capture(name, checks)
            if elapsed >= next_log:
                self.log(f'Output {health.get("status")}, media ROI frames {visual["frames"]}, changes {visual["changes"]}, stage {active.get("event_id") if active else "playlist"}')
                next_log = elapsed + 20
            write_json(self.root / 'progress.json', {**self.report, 'elapsed': elapsed})
            time.sleep(1)

    def close(self):
        if self.picture:
            self.report['picture'] = self.picture.sample()
            self.picture.close()
        if hasattr(self, 'noar') and self.noar.poll() is None and self.token:
            try:
                self.api('POST', '/api/control/1/process', {'command': 'stop'})
                self.archive()
            except Exception as error:
                self.report['incidents'].append({'what': 'cleanup: ' + str(error)})
        for process in reversed(self.processes):
            self.stop(process)
        for handle in self.handles:
            handle.close()
        for generation in sorted({item['generation'] for item in self.archived.values()}):
            entries = [item for item in self.archived.values() if item['generation'] == generation]
            text = '#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:3\n#EXT-X-PLAYLIST-TYPE:VOD\n'
            text += ''.join(f"#EXTINF:{item['duration']},\n{item['file']}\n" for item in entries)
            (self.root / f'archive/g{generation}.m3u8').write_text(text + '#EXT-X-ENDLIST\n')
        log = re.sub(r'\x1b\[[0-9;]*m', '', (self.root / 'backend.log').read_text(errors='replace')) if (self.root / 'backend.log').exists() else ''
        self.report['fatal_log_lines'] = [line for line in log.splitlines() if any(marker in line for marker in ['panicked', 'CRITICAL', 'assertion', 'Stream switch requested'])]
        self.report['error_log_lines'] = [line for line in log.splitlines() if re.search(r'\[\s*ERROR\]', line)]
        self.report['segments'] = len(self.archived)
        if self.report.get('elapsed', 0) < self.args.seconds - 2:
            self.report['incidents'].append({'what': 'The requested cycle did not complete'})
        self.report['seconds_requested'] = self.args.seconds
        self.report['scenario'] = self.args.scenario
        self.report['status'] = 'FAIL' if self.report['incidents'] or self.report['fatal_log_lines'] else 'PASS'
        self.report['binary_sha256'] = hashlib.file_digest(self.args.binary.open('rb'), 'sha256').hexdigest()
        write_json(self.root / 'report.json', self.report)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--mediamtx', type=Path, required=True)
    parser.add_argument('--work-root', type=Path, required=True)
    parser.add_argument('--seconds', type=int, default=900)
    parser.add_argument('--scenario', choices=['full', 'udp-tracks'], default='full')
    args = parser.parse_args()
    if args.scenario == 'udp-tracks':
        args.seconds = 90
    elif args.seconds < 900:
        parser.error('The full fire scenario requires at least 900 seconds')
    run = FireTest(args)
    try:
        run.run()
    except (Exception, KeyboardInterrupt) as error:
        run.report['incidents'].append({'what': str(error)})
        run.log('FAIL: ' + str(error))
    finally:
        run.close()
    print(json.dumps({'status': run.report['status'], 'segments': run.report['segments']}), flush=True)
    return 0 if run.report['status'] == 'PASS' else 1


if __name__ == '__main__':
    raise SystemExit(main())
