#!/usr/bin/env python3
"""Run NoAr on air for a long time with synthetic media and faults, and check
that its transport and decoded moving pictures never stop.

Everything runs in disposable Docker containers on the loopback interface: a
MediaMTX that receives NoAr's output and relays a synthetic live, the NoAr
image under test with its own empty /data, and GStreamer publishers started
from the same image. Each cycle (12 minutes) airs playlist clips of different
sizes and frame rates, and three scheduled lives:

- an SRT live whose encoder drops out for 20 s and comes back,
- the same live read over RTMP (a picture announced late),
- a UDP live whose encoder drops its video for 30 s and takes it back (the
  stream collection changes under the decoder),

and then restarts the playout in the middle of a clip.

Every 2 s it reads NoAr's /health, SRT receiver bytes or the HLS media playlist,
and the clip on air. An independent FFmpeg receiver continuously decodes the
actual output to grayscale frames. Missing frames or a frozen picture for more
than 12 s fail too, even when transport bytes and /health keep advancing.

    NOAR_SOAK=1 NOAR_SOAK_IMAGE=noar-soak:ci NOAR_SOAK_MINUTES=90 \\
        python3 qa/soak/soak.py

NOAR_SOAK_BINARY mounts another noar binary over the image's (a local build
in a development image). NOAR_SOAK_DIR sets where logs and the report go.
"""

import datetime as dt
import hashlib
import json
import os
import re
import secrets
import shlex
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

from picture import PictureMonitor

if os.environ.get('NOAR_SOAK') != '1':
    raise SystemExit('Refusing to run without NOAR_SOAK=1: it starts containers and binds loopback ports')

HERE = Path(__file__).resolve().parent
IMAGE = os.environ.get('NOAR_SOAK_IMAGE', 'noar-soak:ci')
BINARY = os.environ.get('NOAR_SOAK_BINARY')
MINUTES = float(os.environ.get('NOAR_SOAK_MINUTES', '90'))
OUTPUT = os.environ.get('NOAR_SOAK_OUTPUT', 'srt')
if OUTPUT not in {'srt', 'hls'}:
    raise SystemExit('NOAR_SOAK_OUTPUT must be srt or hls; NDI/SDI need separate hardware evidence')
WORK = Path(os.environ.get('NOAR_SOAK_DIR', HERE / 'artifacts' / 'run')).resolve()
MEDIAMTX = 'bluenviron/mediamtx:1.15.3'
PREFIX = 'noar-soak-' + secrets.token_hex(4) + '-'
BASE = 'http://127.0.0.1:18787'
MTX = 'http://127.0.0.1:9997/v3/'

CYCLE = 12 * 60
POLL = 2.0
STALL_LIMIT = 12.0
UNHEALTHY_LIMIT = 20.0
RESTART_GRACE = 45.0
HEALTHY = {'on_air', 'starting', 'stopped'}

# (offset, end offset, name) of the lives each cycle airs, from its start.
EVENTS = [(30, 210, 'srt'), (270, 390, 'rtmp'), (420, 570, 'udp')]
# (offset, action) within each cycle.
ACTIONS = [
    (0, 'publish srt-av'),
    (90, 'unpublish'),       # the SRT live drops out...
    (110, 'publish srt-av'),  # ...and comes back
    (400, 'publish udp-av'),
    (480, 'publish udp-audio'),  # the encoder drops its video...
    (510, 'publish udp-av'),     # ...and takes it back
    (580, 'unpublish'),
    (630, 'restart'),
]

VIDEO = ('videotestsrc is-live=true pattern=smpte horizontal-speed=3 ! video/x-raw,width=640,height=360,framerate=25/1 '
         '! x264enc tune=zerolatency speed-preset=ultrafast bitrate=800 key-int-max=50 '
         '! h264parse config-interval=-1 ! mux. ')
AUDIO = ('audiotestsrc is-live=true freq=440 ! audio/x-raw,rate=48000,channels=2 '
         '! audioconvert ! avenc_aac ! aacparse ! mux. ')
PUBLISHERS = {
    'srt-av': VIDEO + AUDIO + 'mpegtsmux name=mux alignment=7 '
              '! srtsink uri=srt://127.0.0.1:8890 streamid=publish:live/feed latency=200',
    'udp-av': VIDEO + AUDIO + 'mpegtsmux name=mux alignment=7 ! udpsink host=127.0.0.1 port=5004',
    'udp-audio': AUDIO + 'mpegtsmux name=mux alignment=7 ! udpsink host=127.0.0.1 port=5004',
}
# name, seconds, width, height, fps, audio
CLIPS = [('a', 15, 640, 360, 25, True), ('b', 25, 1280, 720, 30, True), ('c', 20, 854, 480, 25, False)]

token = None
started = time.monotonic()


def log(message):
    line = f'[{time.monotonic() - started:8.1f}s] {message}'
    print(line, flush=True)
    with (WORK / 'soak.log').open('a') as handle:
        handle.write(line + '\n')


def docker(*args, check=True):
    return subprocess.run(['docker', *args], check=check, capture_output=True, text=True)


def container(name, *args, image=IMAGE, entrypoint=None):
    docker('rm', '-f', PREFIX + name, check=False)
    command = ['run', '--detach', '--name', PREFIX + name, '--network', 'host']
    if entrypoint:
        command += ['--entrypoint', entrypoint]
    docker(*command, *args[:-1], image, *args[-1])


def request(path, data=None, method=None, base=BASE, timeout=10):
    headers = {}
    if token and base == BASE:
        headers['Authorization'] = 'Bearer ' + token
    body = None if data is None else json.dumps(data).encode()
    if body is not None:
        headers['Content-Type'] = 'application/json'
    req = urllib.request.Request(base + path, data=body, headers=headers, method=method)
    try:
        with urllib.request.urlopen(req, timeout=timeout) as response:
            return response.status, parse(response.read())
    except urllib.error.HTTPError as error:
        return error.code, parse(error.read())
    except OSError as error:  # Refused, reset or timed out: no answer.
        return 0, str(error)


def parse(raw):
    try:
        return json.loads(raw or b'null')
    except ValueError:
        return raw.decode(errors='replace')


def api(path, data=None, method=None, base=BASE):
    code, body = request(path, data, method, base)
    if code == 0 or code >= 400:
        raise RuntimeError(f'{method or "GET"} {path}: {code} {body}')
    return body


def login(password):
    global token
    token = None
    token = api('/auth/login', {'username': 'soak', 'password': password}, 'POST')['access']


def make_clips(media):
    media.mkdir(parents=True, exist_ok=True)
    for name, seconds, width, height, fps, audio in CLIPS:
        frames = seconds * fps
        pipeline = (f'videotestsrc num-buffers={frames} pattern=smpte horizontal-speed=3 ! video/x-raw,width={width},height={height},'
                    f'framerate={fps}/1 ! x264enc speed-preset=ultrafast key-int-max={fps * 2} ! h264parse ! mux. ')
        if audio:
            pipeline += (f'audiotestsrc num-buffers={seconds * 25} samplesperbuffer=1920 freq=660 '
                         '! audio/x-raw,rate=48000,channels=2 ! audioconvert ! avenc_aac ! aacparse ! mux. ')
        pipeline += f'mp4mux name=mux ! filesink location=/data/media/{name}.mp4'
        subprocess.run(['docker', 'run', '--rm', '--network', 'none', '-v', f'{WORK / "data"}:/data',
                        '--entrypoint', 'gst-launch-1.0', IMAGE, '-q', '-e', *shlex.split(pipeline)],
                       check=True, capture_output=True, text=True)


def playlist(date):
    program, total = [], 0.0
    while total < 86400:
        for name, seconds, *_ in CLIPS:
            program.append({'source': f'/data/media/{name}.mp4', 'in': 0, 'out': seconds, 'duration': seconds})
            total += seconds
    return {'channel': 'Channel 1', 'date': date, 'program': program}


def set_up():
    for directory in ['db', 'media', 'playlists', 'public', 'logs']:
        (WORK / 'data' / directory).mkdir(parents=True, exist_ok=True)
    make_clips(WORK / 'data' / 'media')
    log('Synthetic clips made: ' + ', '.join(f'{c[0]}.mp4 {c[1]} s {c[2]}x{c[3]}@{c[4]}' for c in CLIPS))

    container('mediamtx', '-v', f'{HERE / "mediamtx.yml"}:/mediamtx.yml:ro', [], image=MEDIAMTX)
    noar_args = ['-v', f'{WORK / "data"}:/data', '-e', 'TZ=UTC']
    if BINARY:
        noar_args += ['-v', f'{Path(BINARY).resolve()}:/usr/local/bin/noar:ro']
    container('noar', *noar_args, ['--listen', '127.0.0.1:18787'], entrypoint='noar' if BINARY else None)

    for _ in range(120):
        try:
            if request('/api/setup')[0] == 200:
                break
        except OSError:
            pass
        time.sleep(0.5)
    else:
        raise RuntimeError('NoAr did not answer')
    if not api('/api/setup')['required']:
        raise RuntimeError('Refusing to configure an installation that is already set up')

    password = secrets.token_urlsafe(24)
    api('/api/setup', {'logs': '/data/logs', 'playlists': '/data/playlists', 'public': '/data/public',
                       'storage': '/data/media', 'shared': False, 'smtp_server': '', 'smtp_user': '',
                       'smtp_password': '', 'smtp_starttls': False, 'smtp_port': 465, 'username': 'soak',
                       'mail': 'soak@example.invalid', 'password': password, 'two_factor': False}, 'POST')
    login(password)

    config = api('/api/playout/config/1')
    stream = next(output for output in api('/api/playout/outputs/1')
                  if output['name'] == ('hls' if OUTPUT == 'hls' else 'stream'))
    config['output'].update({
        'id': stream['id'], 'mode': 'stream', 'stream_type': 'srt',
        'stream_url': 'srt://127.0.0.1:8890?streamid=publish:output&latency=200000',
        'width': 640, 'height': 360, 'fps': 25, 'video_codec': 'libx264', 'audio_codec': 'aac',
        'audio_bitrate': 128, 'muxer_options': {}, 'metadata_options': {}, 'protocol_options': {}})
    if OUTPUT == 'hls':
        config['output'].update({'mode': 'hls', 'hls_playlist_name': 'stream',
                                 'hls_segment_duration': 2, 'hls_list_size': 6, 'hls_variants': []})
    config['processing']['add_logo'] = False
    config['playlist'].update({'day_start': '00:00:00', 'length': '24:00:00', 'infinit': True})
    if 'recording' in config:
        config['recording']['enable'] = False
    api('/api/playout/config/1', config, 'PUT')

    today = dt.datetime.now(dt.timezone.utc).date()
    for day in range(2):
        api('/api/playlist/1', playlist(str(today + dt.timedelta(days=day))), 'POST')

    lives = {}
    for name, url in [('srt', 'srt://127.0.0.1:8890?streamid=read:live/feed&latency=200000'),
                      ('rtmp', 'rtmp://127.0.0.1:1935/live/feed'),
                      ('udp', 'udp://127.0.0.1:5004')]:
        lives[name] = api('/api/schedule/1/lives', {'name': f'Soak {name}', 'url': url}, 'POST')['path']

    api('/api/control/1/process', {'command': 'start'}, 'POST')
    log('NoAr set up and started')
    return password, lives


def schedule_cycle(lives, cycle, cycle_start):
    for offset, end, name in EVENTS:
        begin = cycle_start + dt.timedelta(seconds=offset)
        finish = cycle_start + dt.timedelta(seconds=end)
        if begin.date() != finish.date():
            log(f'Cycle {cycle}: {name} live would cross midnight; skipped')
            continue
        api('/api/schedule/1', {
            'kind': 'live', 'sources': [lives[name]], 'title': f'Soak {name} live {cycle}',
            'start_date': begin.strftime('%Y-%m-%d'), 'start_time': begin.strftime('%H:%M:%S'),
            'end_time': finish.strftime('%H:%M:%S'), 'entry_mode': 'interrupt',
            'after_mode': 'clock', 'retry_seconds': 1}, 'POST')


def act(action):
    if action == 'unpublish':
        docker('rm', '-f', PREFIX + 'publisher', check=False)
    elif action.startswith('publish '):
        container('publisher', shlex.split('-q ' + PUBLISHERS[action.split()[1]]), entrypoint='gst-launch-1.0')
    elif action == 'restart':
        api('/api/control/1/process', {'command': 'restart'}, 'POST')
    log('Action: ' + action)


def output_bytes():
    if OUTPUT == 'hls':
        # Segment rotation can keep total bytes constant. Observe the actual
        # media playlist/sequence advancing instead of summing directory size.
        code, body = request('/public/1/live/stream.m3u8', timeout=5)
        if code != 200 or not isinstance(body, str) or '#EXTINF:' not in body:
            return None
        return hashlib.sha256(body.encode()).hexdigest()
    code, body = request('paths/get/output', base=MTX, timeout=5)
    if code != 200 or not isinstance(body, dict):
        return None
    return body.get('bytesReceived')


def picture_url():
    if OUTPUT == 'hls':
        return BASE + '/public/1/live/stream.m3u8'
    return 'srt://127.0.0.1:8890?streamid=read:output&latency=200000'


def picture_grace(action, now, previous):
    # Deliberate encoder faults may air the static waiting card. Suppress
    # only the known fault interval plus a bounded signal-recovery allowance.
    seconds = {'unpublish': 40, 'publish udp-audio': 50, 'restart': RESTART_GRACE}.get(action, 0)
    return max(previous, now + seconds) if seconds else previous


def running(name):
    result = docker('inspect', '-f', '{{.State.Running}}', PREFIX + name, check=False)
    return result.stdout.strip() == 'true'


def main():
    WORK.mkdir(parents=True, exist_ok=True)
    (WORK / 'soak.log').write_text('')
    report = {'image': IMAGE, 'output': OUTPUT, 'minutes': MINUTES, 'incidents': [], 'cycles': 0, 'clips': 0,
              'restarts': 0, 'samples': 0, 'picture_checks': 0,
              'output_coverage': {'srt': 'NOT_RUN', 'hls': 'NOT_RUN',
                                  'ndi': 'NOT_AVAILABLE', 'sdi': 'NOT_AVAILABLE'},
              'coverage_notes': {'ndi': 'No NDI runtime/receiver fixture configured',
                                 'sdi': 'No DeckLink output/capture hardware fixture configured'}}
    picture = None
    picture_grace_until = 0
    try:
        # Fail before starting containers if the independent receiver is absent.
        picture = PictureMonitor(picture_url(), WORK)
        password, lives = set_up()
        picture.start()
        end = time.monotonic() + MINUTES * 60
        next_login = time.monotonic() + 1800
        cycle, cycle_mono, pending = 0, None, []
        last_bytes, bytes_changed = None, time.monotonic()
        unhealthy_since, stalled_since = None, None
        grace_until = time.monotonic() + 60
        picture_grace_until = grace_until
        last_picture_problem = None
        last_source = None

        while time.monotonic() < end:
            now = time.monotonic()
            if cycle_mono is None or now - cycle_mono >= CYCLE:
                cycle += 1
                cycle_mono = now
                schedule_cycle(lives, cycle, dt.datetime.now(dt.timezone.utc).replace(microsecond=0))
                pending = list(ACTIONS)
                report['cycles'] = cycle
                log(f'Cycle {cycle} scheduled')
            while pending and now - cycle_mono >= pending[0][0]:
                offset, action = pending.pop(0)
                act(action)
                if action != 'unpublish' or offset == 90:
                    picture_grace_until = picture_grace(action, time.monotonic(), picture_grace_until)
                if action == 'restart':
                    report['restarts'] += 1
                    grace_until = time.monotonic() + RESTART_GRACE
            if now >= next_login:
                login(password)
                next_login = now + 1800

            if not running('noar'):
                report['incidents'].append({'at': round(now - started), 'what': 'NoAr exited'})
                log('NoAr exited')
                break

            code, health = request('/health', timeout=5)
            state = (health or {}).get('channels', [{}])[0].get('state') if isinstance(health, dict) else None
            current = output_bytes()
            status_code, status = request('/api/control/1/media/current', timeout=5)
            source = status.get('media', {}).get('source') if isinstance(status, dict) else None
            if source and source != last_source:
                report['clips'] += 1
                last_source = source
            report['samples'] += 1
            visual = picture.sample(picture_grace_until)
            if time.monotonic() >= picture_grace_until:
                report['picture_checks'] += 1
            if visual['problem'] and visual['problem'] != last_picture_problem:
                report['incidents'].append({'at': round(now - started), 'what': visual['problem'],
                                            'decoded_frames': visual['frames'], 'output_progress': current})
                log(f'Picture check: {visual["problem"]} ({visual["frames"]} frames received)')
            last_picture_problem = visual['problem']

            if current is not None and current != last_bytes:
                last_bytes, bytes_changed = current, now
            in_grace = now < grace_until

            stalled = now - bytes_changed > STALL_LIMIT and not in_grace
            if stalled and stalled_since is None:
                stalled_since = bytes_changed
                log(f'Output stopped growing (health {state})')
            elif not stalled and stalled_since is not None:
                if now - stalled_since > STALL_LIMIT:
                    report['incidents'].append({'at': round(stalled_since - started),
                                                'what': f'output stalled for {now - stalled_since:.0f} s'})
                stalled_since = None
                log('Output growing again')

            healthy = code == 200 and state in HEALTHY
            if not healthy and not in_grace:
                if unhealthy_since is None:
                    unhealthy_since = now
                    log(f'Health not ok: {code} {state}')
            elif unhealthy_since is not None:
                if now - unhealthy_since > UNHEALTHY_LIMIT:
                    report['incidents'].append({'at': round(unhealthy_since - started),
                                                'what': f'health not ok for {now - unhealthy_since:.0f} s'})
                unhealthy_since = None
                log('Health ok again')

            time.sleep(POLL)

        for since, what, limit in [(stalled_since, 'output stalled', STALL_LIMIT),
                                   (unhealthy_since, 'health not ok', UNHEALTHY_LIMIT)]:
            if since is not None and time.monotonic() - since > limit:
                report['incidents'].append({'at': round(since - started),
                                            'what': f'{what} at the end, for {time.monotonic() - since:.0f} s'})
    except Exception as error:  # A broken harness fails the run too, with its reason.
        report['incidents'].append({'at': round(time.monotonic() - started), 'what': f'test error: {error}'})
        log(f'Test error: {error}')
    finally:
        if picture is not None:
            report['picture'] = picture.sample(picture_grace_until)
            if report['picture']['problem'] and not any(
                    incident['what'] == report['picture']['problem'] for incident in report['incidents']):
                report['incidents'].append({'at': round(time.monotonic() - started),
                                            'what': report['picture']['problem']})
            picture.close()
            if picture.tracker.frames < 5 or picture.tracker.changes < 2:
                report['incidents'].append({'at': None, 'what': 'insufficient decoded moving-picture evidence'})
            if not report['picture_checks']:
                report['incidents'].append({'at': None, 'what': 'no picture checks outside startup/fault grace'})
        logs = docker('logs', PREFIX + 'noar', check=False)
        noar_log = (logs.stdout or '') + (logs.stderr or '')
        (WORK / 'noar.log').write_text(noar_log)
        if any(incident['what'] == 'NoAr exited' for incident in report['incidents']):
            tail = [line for line in noar_log.splitlines() if '"GET /' not in line][-25:]
            print('Last lines of NoAr before it exited:\n' + '\n'.join(tail), flush=True)
        for marker in ['panicked', 'Stream switch requested', 'CRITICAL', 'assertion']:
            hits = [line for line in noar_log.splitlines() if marker in line]
            if hits:
                report['incidents'].append({'at': None, 'what': f'{len(hits)} log line(s) with "{marker}"',
                                            'first': hits[0][:300]})
        # The channel's own log says what aired; its HTML markup is dropped.
        channel_log = ''
        for path in sorted((WORK / 'data' / 'logs').glob('*.log')):
            channel_log += re.sub(r'<[^>]+>', '', path.read_text(errors='replace'))
        (WORK / 'channel.log').write_text(channel_log)
        lines = channel_log.splitlines()
        report['events_on_air'] = sum('Scheduled event' in line and line.endswith('on air') for line in lines)
        report['live_waits'] = sum('waiting for the signal' in line for line in lines)
        errors = [line for line in lines if '[ERROR]' in line]
        report['errors_logged'] = len(errors)
        if errors:
            report['incidents'].append({'at': None, 'what': f'{len(errors)} error(s) in the channel log',
                                        'first': errors[0][:300]})
        for name in ['publisher', 'noar', 'mediamtx']:
            docker('rm', '-f', PREFIX + name, check=False)

    report['passed'] = not report['incidents']
    report['output_coverage'][OUTPUT] = 'PASS' if report['passed'] else 'FAIL'
    (WORK / 'report.json').write_text(json.dumps(report, indent=2))
    summary = [f'## NoAr soak test: {"PASS" if report["passed"] else "FAIL"}', '',
               f'{OUTPUT.upper()}, {MINUTES:g} min, {report["cycles"]} cycles, {report["clips"]} clips seen on air, '
               f'{report["events_on_air"]} scheduled lives on air ({report["live_waits"]} waits for a lost signal), '
               f'{report["restarts"]} restarts, {report["errors_logged"]} errors in the channel log, '
               f'{report["samples"]} samples.', '']
    summary += [f'- {incident["what"]}' + (f' (at {incident["at"]} s)' if incident['at'] is not None else '')
                + (f': `{incident["first"]}`' if incident.get('first') else '')
                for incident in report['incidents']] or ['No incidents: transport and decoded moving pictures passed.']
    (WORK / 'summary.md').write_text('\n'.join(summary) + '\n')
    if os.environ.get('GITHUB_STEP_SUMMARY'):
        with open(os.environ['GITHUB_STEP_SUMMARY'], 'a') as handle:
            handle.write('\n'.join(summary) + '\n')
    print('\n'.join(summary))
    return 0 if report['passed'] else 1


if __name__ == '__main__':
    sys.exit(main())
