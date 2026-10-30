#!/usr/bin/env python3
"""Exercise a disposable actual NoAr binary against a separate local MediaMTX.

Requires NOAR_GST_STAGING=1 and an explicit NOAR_GST_BINARY path. Never uses
production databases, credentials or media. See INTEGRATED.md.
"""

import datetime as dt
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import secrets
import shutil
import socket
import subprocess
import threading
import time
import urllib.error
import urllib.request
from zoneinfo import ZoneInfo

from integrated_checks import HlsObserver, UdpCapture, decodes, scte35_metrics, transport_metrics

REQUESTED_OUTPUT = os.environ.get('NOAR_GST_TEST_OUTPUT', 'srt')
if REQUESTED_OUTPUT not in ['srt', 'rtmp', 'hls', 'udp', 'scte35']:
    raise SystemExit('NOAR_GST_TEST_OUTPUT must be srt, rtmp, hls, udp or scte35')
OUTPUT = 'udp' if REQUESTED_OUTPUT == 'scte35' else REQUESTED_OUTPUT
SCTE_TEST = REQUESTED_OUTPUT == 'scte35' or os.environ.get('NOAR_GST_TEST_SCTE35') == '1'
if SCTE_TEST and OUTPUT == 'rtmp':
    raise SystemExit('RTMP has no interoperable SCTE-35 mapping; select SRT, UDP or HLS')
INPUT = os.environ.get('NOAR_GST_TEST_INPUT', 'srt')
if INPUT not in ['srt', 'rtmp', 'hls', 'udp']:
    raise SystemExit('NOAR_GST_TEST_INPUT must be srt, rtmp, hls or udp')
RAW_SRT = OUTPUT == 'srt' and SCTE_TEST
PREROLL = 6 if OUTPUT == 'hls' else 2
AUTOMATIC = os.environ.get('NOAR_GST_TEST_SCHEDULE') == 'automatic'
if os.environ.get('NOAR_GST_STAGING') != '1':
    raise SystemExit('Refusing to run without NOAR_GST_STAGING=1')
BINARY = Path(os.environ.get('NOAR_GST_BINARY', '')).expanduser()
if not BINARY.is_file() or not os.access(BINARY, os.X_OK):
    raise SystemExit('Set NOAR_GST_BINARY to the executable staging NoAr binary')
BINARY = BINARY.resolve()
BINARY_SHA256 = hashlib.sha256(BINARY.read_bytes()).hexdigest()
REPO = Path(__file__).resolve().parents[2]
ROOT = Path(__file__).resolve().parent/'artifacts'/('integrated-' + dt.datetime.now(dt.timezone.utc).strftime('%Y%m%dT%H%M%S%fZ'))
ROOT.mkdir(parents=True, exist_ok=False)
BASE = 'http://127.0.0.1:19787'
MTX = 'http://127.0.0.1:29997/v3/'
TOKEN = None
PROCESSES = []
LOGS = []
LOG_THREADS = []
STARTED = time.monotonic()
raw_capture = None
hls_observer = None
capture = None


def output_connections():
    endpoint = 'rtmpconns/list' if OUTPUT == 'rtmp' else 'srtconns/list'
    return api(endpoint, base=MTX)

def api(path, data=None, method=None, base=BASE):
    headers = {}
    if TOKEN and base == BASE:
        headers['Authorization'] = 'Bearer ' + TOKEN
    payload = None if data is None else json.dumps(data).encode()
    if payload is not None:
        headers['Content-Type'] = 'application/json'
    request = urllib.request.Request(base + path, data=payload, headers=headers, method=method)
    try:
        with urllib.request.urlopen(request, timeout=20) as response:
            body = response.read()
            try: return json.loads(body)
            except ValueError: return body.decode()
    except urllib.error.HTTPError as error:
        raise RuntimeError(f'{method or "GET"} {path}: {error.code} {error.read().decode()}') from None

def start(name, args, **kwargs):
    path = ROOT / (name + '.log')
    if path.exists():
        path = ROOT / (name + '-restart.log')
    log = path.open('w')
    LOGS.append(log)
    process = subprocess.Popen(args, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, **kwargs)
    PROCESSES.append(process)
    def record_output():
        with process.stdout:
            for line in iter(process.stdout.readline, b''):
                log.write(f'[{time.monotonic() - STARTED:.6f}] ' + line.decode(errors='replace'))
                log.flush()
    reader = threading.Thread(target=record_output, daemon=True)
    LOG_THREADS.append(reader)
    reader.start()
    return process

def stop(process):
    if process.poll() is None:
        process.terminate()
        try: process.wait(timeout=12)
        except subprocess.TimeoutExpired:
            process.kill(); process.wait()

def source():
    transport, target = 'mpegts', 'srt://127.0.0.1:29890?streamid=publish:input&latency=200000'
    if INPUT == 'rtmp':
        transport, target = 'flv', 'rtmp://127.0.0.1:21935/live/input'
    elif INPUT == 'udp':
        target = 'udp://127.0.0.1:29912?pkt_size=1316'
    return start('source', ['ffmpeg', '-hide_banner', '-loglevel', 'warning', '-re',
        '-f', 'lavfi', '-i', 'testsrc2=size=640x360:rate=25,drawbox=x=0:y=0:w=96:h=96:color=green:t=fill',
        '-f', 'lavfi', '-i', 'sine=frequency=880:sample_rate=48000',
        '-c:v', 'libx264', '-preset', 'ultrafast', '-tune', 'zerolatency', '-g', '25', '-threads', '2',
        '-c:a', 'aac', '-b:a', '96k', '-f', transport, target])

try:
    for command in ['ffmpeg', 'ffprobe']:
        if shutil.which(command) is None:
            raise RuntimeError(f'Required test dependency is unavailable: {command}')
    if api('srtconns/list', base=MTX)['items']:
        raise RuntimeError('Refusing to use MediaMTX with existing SRT sessions; start the dedicated disposable instance')
    if any(item.get('ready') for item in api('paths/list', base=MTX)['items']):
        raise RuntimeError('Refusing to use MediaMTX with an active media path')
    if OUTPUT == 'rtmp' and api('rtmpconns/list', base=MTX)['items']:
        raise RuntimeError('Refusing to use MediaMTX with existing RTMP sessions')
    if SCTE_TEST:
        import threefive  # Fail before setup if the independent decoder is unavailable.
    with socket.socket() as check_port:
        check_port.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        check_port.bind(('127.0.0.1', 19787))
    env = os.environ.copy()
    env.update({'XDG_CACHE_HOME': str(ROOT / 'cache'), 'TZ': 'UTC',
        'DB': str(ROOT / 'noar.db'), 'NOAR_SETUP_LOGS': str(ROOT / 'logs'),
        'NOAR_SETUP_STORAGE': str(ROOT / 'storage'), 'NOAR_SETUP_PLAYLISTS': str(ROOT / 'playlists'),
        'NOAR_SETUP_PUBLIC': str(ROOT / 'public')})
    noar = start('noar', [str(BINARY), '--listen', '127.0.0.1:19787', '--log-to-console'], env=env, cwd=REPO)
    for _ in range(100):
        if noar.poll() is not None:
            raise RuntimeError('Staging NoAr exited before setup; inspect noar.log')
        try:
            status = api('/api/setup'); break
        except (OSError, RuntimeError): time.sleep(0.1)
    else: raise RuntimeError('NoAr did not start')
    if not status['required']:
        raise RuntimeError('Refusing to configure an existing installation')
    password = secrets.token_urlsafe(24)
    api('/api/setup', {'logs': str(ROOT/'logs'), 'playlists': str(ROOT/'playlists'),
        'public': str(ROOT/'public'), 'storage': str(ROOT/'storage'), 'shared': False,
        'smtp_server': '', 'smtp_user': '', 'smtp_password': '', 'smtp_starttls': False, 'smtp_port': 465,
        'username': 'staging', 'mail': 'staging@example.invalid', 'password': password, 'two_factor': False}, 'POST')
    TOKEN = api('/auth/login', {'username': 'staging', 'password': password}, 'POST')['access']
    config = api('/api/playout/config/1')
    if 'ingest' in config:
        raise RuntimeError('The server still exposes removed input-server settings')
    channel = api('/api/channel/1')
    storage = Path(channel['storage'])
    if not storage.is_relative_to(ROOT):
        raise RuntimeError('Refusing to use a storage directory outside this test run')
    storage.mkdir(parents=True, exist_ok=True)
    for name in ['a', 'b']:
        shutil.copyfile(REPO/'qa/gstreamer/fixtures'/f'{name}.mp4', storage/f'{name}.mp4')
    outputs = api('/api/playout/outputs/1')
    stream = next(output for output in outputs if output['name'] == ('hls' if OUTPUT == 'hls' else 'stream'))
    now = dt.datetime.now(ZoneInfo(channel.get('timezone') or 'UTC'))
    config['output'].update({'id': stream['id'], 'mode': 'stream', 'stream_type': 'srt',
        'stream_url': 'srt://127.0.0.1:29890?streamid=publish:output&latency=200000',
        'width': 640, 'height': 360, 'fps': 25, 'video_codec': 'libx264', 'audio_codec': 'aac',
        'audio_bitrate': 128, 'muxer_options': {}, 'metadata_options': {}, 'protocol_options': {}})
    if OUTPUT == 'rtmp':
        config['output'].update({'stream_type': 'rtmp', 'stream_url': 'rtmp://127.0.0.1:21935/live/output'})
    elif OUTPUT == 'hls':
        config['output'].update({'mode': 'hls', 'hls_playlist_name': 'stream',
            'hls_segment_duration': 2, 'hls_list_size': 4,
            'hls_variants': ['low:320x180:600k:96k']})
    elif OUTPUT == 'udp':
        config['output'].update({'stream_type': 'udp', 'stream_url': 'udp://127.0.0.1:29910'})
    if SCTE_TEST:
        config['scte35'].update({'enable': True, 'pid': 500, 'preroll': PREROLL,
                                'min_break': 1, 'max_break': 60})
    if RAW_SRT:
        config['output']['stream_url'] = 'srt://127.0.0.1:29911?mode=caller&latency=200000'
    config['processing']['add_logo'] = False
    config['playlist'].update({'day_start': (now-dt.timedelta(seconds=2)).strftime('%H:%M:%S'), 'length': '24:00:00', 'infinit': True})
    config['recording'].update({'enable': True, 'source': 'stream', 'source_output_id': stream['id'],
        'path': str(ROOT/'recordings'), 'segment_duration': 30, 'minimum_free_space_gb': 0})
    if OUTPUT == 'hls':
        config['recording'].update({'source': 'hls_variant', 'variant': 'stream'})
    api('/api/playout/config/1', config, 'PUT')
    playlist = {'channel': channel['name'], 'date': now.strftime('%Y-%m-%d'), 'program': [
        {'source': str(storage/f'{name}.mp4'), 'in': 0, 'out': 12, 'duration': 180}
        for name in ['a','b'] * 20]}
    api('/api/playlist/1', playlist, 'POST')
    live = api('/api/schedule/1/lives', {'name': 'Synthetic live',
        'url': {'srt': 'srt://127.0.0.1:29890?streamid=read:input&latency=200000',
                'rtmp': 'rtmp://127.0.0.1:21935/live/input',
                'hls': 'http://127.0.0.1:21988/input/index.m3u8',
                'udp': 'udp://127.0.0.1:29912'}[INPUT]}, 'POST')
    # Future wall time prevents an automatic start; each event is triggered through the real API.
    events = []
    for index, after in enumerate(['clock', 'continue', 'clock'][:int(os.environ.get('NOAR_GST_TEST_EVENTS','3'))]):
        planned = now + (dt.timedelta(seconds=14 + index*18) if AUTOMATIC
                         else dt.timedelta(hours=1, seconds=index*20))
        event = api('/api/schedule/1', {'kind': 'live', 'sources': [live['path']],
            'title': f'Native {after} {index}', 'start_date': planned.strftime('%Y-%m-%d'),
            'start_time': planned.strftime('%H:%M:%S'), 'end_time': (planned+dt.timedelta(seconds=8)).strftime('%H:%M:%S'),
            'entry_mode': 'interrupt', 'after_mode': after, 'retry_seconds': 1}, 'POST')
        events.append(event)
    publisher = source()
    if OUTPUT == 'udp':
        raw_capture = UdpCapture(ROOT/'capture.ts')
    if RAW_SRT:
        capture = start('receiver', ['python3', str(REPO/'qa/gstreamer/srt_capture.py'),
            str(ROOT/'capture.ts'), 'srt://127.0.0.1:29911?mode=listener&latency=200000'])
        time.sleep(0.5)
    api('/api/control/1/process', {'command': 'start'}, 'POST')
    for _ in range(150):
        if RAW_SRT:
            ready = (ROOT/'capture.ts').exists() and (ROOT/'capture.ts').stat().st_size > 0
        elif OUTPUT == 'udp':
            ready = bool(raw_capture.observations)
        elif OUTPUT == 'hls':
            try:
                with urllib.request.urlopen(BASE + '/public/1/live/stream.m3u8', timeout=2) as response:
                    ready = b'.ts' in response.read()
                if ready:
                    with urllib.request.urlopen(BASE + '/public/1/live/low.m3u8', timeout=2) as response:
                        ready = b'.ts' in response.read()
            except urllib.error.HTTPError:
                ready = False
        else:
            connections = output_connections()
            output_path = 'live/output' if OUTPUT == 'rtmp' else 'output'
            ready = any(item.get('path') == output_path and item.get('state') == 'publish' for item in connections['items'])
        if ready:
            break
        time.sleep(0.2)
    else: raise RuntimeError('NoAr did not publish output')
    if OUTPUT != 'udp' and not RAW_SRT:
        receiver_url = {'srt': 'srt://127.0.0.1:29890?streamid=read:output&latency=200000',
            'rtmp': 'rtmp://127.0.0.1:21935/live/output', 'hls': BASE + '/public/1/live/stream.m3u8'}[OUTPUT]
        capture = start('receiver', ['ffmpeg', '-hide_banner', '-loglevel', 'warning',
            '-probesize', '131072', '-analyzeduration', '1000000', '-i', receiver_url,
            '-map', '0:v:0', '-map', '0:a:0', '-c', 'copy',
            '-stats_period', '0.1', '-progress', str(ROOT/'progress.txt'), '-y', str(ROOT/'capture.ts')])
    if OUTPUT == 'hls':
        hls_observer = HlsObserver(BASE + '/public/1/live/', ROOT/'hls-downloads')
    snapshots, statuses, trigger_times = [], [], []
    begin = time.monotonic()
    next_event = 0
    fault_stopped = False
    fault_restarted = False
    recovery = os.environ.get('NOAR_GST_TEST_RECOVERY') == '1'
    cue_requests = []
    cancel_requested = False
    progress_samples = []
    progress_observations = []
    progress_stop = threading.Event()
    def observe_progress():
        previous = None
        while not progress_stop.wait(0.05):
            if RAW_SRT:
                path = ROOT/'capture.ts'
                current = path.stat().st_size if path.exists() else 0
                if current and current != previous:
                    observed = time.monotonic() - begin
                    progress_samples.append(observed)
                    progress_observations.append({'elapsed': observed, 'bytes': current})
                    previous = current
                continue
            path = ROOT/'progress.txt'
            if path.is_file():
                values = [line for line in path.read_text().splitlines() if line.startswith('out_time_us=')]
                current = values[-1] if values else None
                if current is not None and current != previous:
                    observed = time.monotonic() - begin
                    progress_samples.append(observed)
                    progress_observations.append({'elapsed': observed, 'out_time_us': current.split('=', 1)[1]})
                    previous = current
    progress_thread = threading.Thread(target=observe_progress, daemon=True)
    progress_thread.start()
    while time.monotonic() - begin < int(os.environ.get('NOAR_GST_TEST_SECONDS','60')):
        elapsed = time.monotonic() - begin
        if capture is not None and capture.poll() is not None:
            raise RuntimeError('Output receiver exited early; inspect receiver.log')
        if recovery and elapsed >= 11 and not fault_stopped:
            stop(publisher)
            fault_stopped = True
        if recovery and elapsed >= 15 and not fault_restarted:
            publisher = source()
            fault_restarted = True
        if not AUTOMATIC and next_event < len(events) and elapsed >= 8 + next_event*18:
            api(f'/api/schedule/1/{events[next_event]["id"]}/trigger', {}, 'POST')
            trigger_times.append({'elapsed': elapsed, 'event': events[next_event]['id'], 'after': events[next_event]['after_mode']})
            next_event += 1
        if SCTE_TEST:
            if len(cue_requests) < 3 and elapsed >= [6, 32, 48][len(cue_requests)]:
                cue = api('/api/control/1/scte35/break', {'duration': 6}, 'POST')
                cue_requests.append({'event_id': cue['event_id'], 'elapsed': elapsed,
                                     'cancelled': len(cue_requests) == 2})
            if len(cue_requests) == 3 and not cancel_requested and elapsed >= 48.5:
                status = api('/api/control/1/scte35')
                announced = any(item['event_id'] == cue_requests[-1]['event_id'] and item['out_messages'] > 0
                                for item in status['breaks'])
                if announced:
                    api(f'/api/control/1/scte35/break/{cue_requests[-1]["event_id"]}', method='DELETE')
                    cue_requests[-1]['cancelled_at'] = time.monotonic()
                    cancel_requested = True
        snapshots.append(output_connections())
        if hls_observer is not None:
            hls_observer.poll()
        statuses.append({'elapsed': elapsed, 'schedule': api('/api/schedule/1/status'),
            'media': api('/api/control/1/media/current')})
        time.sleep(0.35)
    ended = time.monotonic() - begin
    progress_stop.set()
    progress_thread.join()
    api('/api/control/1/process', {'command': 'stop'}, 'POST')
    if raw_capture is not None:
        raw_capture.stop()
        progress_samples = [at - begin for at in raw_capture.observations if begin <= at <= begin + ended]
        progress_observations = [{'elapsed': elapsed, 'datagrams': index}
                                 for index, elapsed in enumerate(progress_samples, 1)]
    else:
        stop(capture)
    stop(publisher)
    (ROOT/'connections.json').write_text(json.dumps(snapshots))
    (ROOT/'status.json').write_text(json.dumps(statuses))
    (ROOT/'triggers.json').write_text(json.dumps(trigger_times))
    (ROOT/'scte35-requests.json').write_text(json.dumps(cue_requests))
    (ROOT/'receiver-progress.json').write_text(json.dumps(progress_observations))
    (ROOT/'timing.json').write_text(json.dumps({'capture_started_elapsed': begin - STARTED, 'capture_duration': ended}))
    spec = importlib.util.spec_from_file_location('verify', REPO/'qa/gstreamer/verify.py')
    verify = importlib.util.module_from_spec(spec); spec.loader.exec_module(verify)
    result = subprocess.run(['ffprobe', '-v', 'error', '-show_streams', '-show_packets', '-of', 'json', str(ROOT/'capture.ts')], capture_output=True, check=True)
    packets = verify.packet_metrics(json.loads(result.stdout))
    connections = verify.connection_metrics(snapshots, path='live/output' if OUTPUT == 'rtmp' else 'output')
    markers = verify.decode_markers(ROOT/'capture.ts')
    gaps = [b-a for a,b in zip(progress_samples, progress_samples[1:])]
    if progress_samples:
        gaps.append(ended - progress_samples[-1])
    progress_limit = hls_observer.target_duration + 1 if hls_observer is not None else 0.75
    checks = {
        'video_timestamps': packets['video']['dts_regressions'] == 0
            and packets['video']['maximum_gap_seconds'] <= 0.08,
        'audio_timestamps': packets['audio']['dts_regressions'] == 0
            and packets['audio']['maximum_gap_seconds'] <= 0.1,
        'av_end_alignment': abs(packets['video']['last_dts']-packets['audio']['last_dts']) < 0.15,
        'receiver_progress': bool(gaps) and max(gaps) <= progress_limit,
        'captured_duration': packets['audio']['duration_seconds'] >= int(os.environ.get('NOAR_GST_TEST_SECONDS','60')) - 2.5,
        'three_video_sources': all(markers['video_quarter_seconds'].get(name, 0) > 0 for name in ['a','b','live']),
        'three_audio_sources': all(markers['audio_quarter_seconds'].get(name, 0) > 0 for name in ['a','b','live']),
        'all_scheduled_live_entries': len(events) == 3 and (AUTOMATIC or len(trigger_times) == len(events))
            and markers['video_sequence'].count('live') >= len(events)
            and markers['audio_sequence'].count('live') >= len(events),
        'recording_created': any((ROOT/'recordings').glob('*.mkv')),
        'fault_was_injected': not recovery or (fault_stopped and fault_restarted),
        'decode': decodes(ROOT/'capture.ts'),
    }
    if OUTPUT in ['srt', 'rtmp'] and not RAW_SRT:
        checks.update({'single_output_publisher': connections['publish']['count'] == 1,
                       'single_output_reader': connections['read']['count'] == 1})
    if OUTPUT == 'srt' and not RAW_SRT:
        checks['no_srt_packet_drops'] = all(connections[key]['drop_statistics_available']
            and connections[key]['maximum_send_drops'] == 0 and connections[key]['maximum_receive_drops'] == 0
            for key in ['publish', 'read'])
    extra_metrics = {}
    if hls_observer is not None:
        extra_checks, extra_metrics = hls_observer.finish(verify)
        checks.update(extra_checks)
        if SCTE_TEST:
            cue_checks, cue_metrics = hls_observer.scte35(cue_requests)
            checks.update(cue_checks)
            extra_metrics['scte35'] = cue_metrics
    if OUTPUT == 'udp':
        transport = transport_metrics(ROOT/'capture.ts')
        checks['udp_capture'] = bool(raw_capture.observations) and not raw_capture.errors
        checks['udp_transport_continuity'] = not transport['continuity_errors'] and not transport['transport_errors']
        if not SCTE_TEST:
            checks['udp_scte35_disabled'] = 500 not in transport['pids']
        extra_metrics['transport'] = transport
    if SCTE_TEST and OUTPUT != 'hls':
        extra_checks, scte_metrics = scte35_metrics(ROOT/'capture.ts', cue_requests, preroll=PREROLL)
        extra_metrics.update(scte_metrics)
        checks.update(extra_checks)
    if SCTE_TEST:
        checks['scte35_requests_executed'] = len(cue_requests) == 3 and cancel_requested
    report = {'status': 'PASS' if all(checks.values()) else 'FAIL',
        'checks': {key: 'PASS' if value else 'FAIL' for key,value in checks.items()},
        'engine': 'gstreamer', 'output': OUTPUT, 'scenario': 'source_loss' if recovery else 'normal',
        'scte35_enabled': SCTE_TEST,
        'live_input': INPUT,
        'schedule_admission': 'automatic' if AUTOMATIC else 'trigger_api',
        'binary_sha256': BINARY_SHA256,
        'fault_injected': fault_stopped and fault_restarted, 'packets': packets,
        'connections': connections if OUTPUT in ['srt', 'rtmp'] else {'note': 'MediaMTX is used for input only'}, 'markers': markers,
        'triggered_events': trigger_times, 'receiver_progress': {
            'samples': len(progress_samples), 'maximum_gap_seconds': max(gaps, default=None),
            'limit_seconds': progress_limit, 'measurement': 'UDP datagram arrival' if raw_capture else 'FFmpeg media progress'},
        'protocol_checks': extra_metrics,
        'recordings': [path.name for path in (ROOT/'recordings').glob('*.mkv')],
        'limitations': ['No browser interaction or hardware validation',
            'Admission is verified for the selected scheduler mode and input protocol only',
            'Recording presence is checked; recording decode/retention need separate validation',
            'Each run covers the selected output only; no long soak or downstream ad insertion validation']}
    (ROOT/'report.json').write_text(json.dumps(report, indent=2))
    print(json.dumps({'artifacts': str(ROOT), 'report': report}, indent=2), flush=True)
    if report['status'] != 'PASS':
        raise SystemExit(1)
except Exception as error:
    status = 'NOT_AVAILABLE' if isinstance(error, ModuleNotFoundError) else 'FAIL'
    report = {'status': status, 'output': OUTPUT, 'checks': {'execution': status}, 'error': str(error)}
    (ROOT/'report.json').write_text(json.dumps(report, indent=2))
    print(json.dumps({'artifacts': str(ROOT), 'report': report}, indent=2), flush=True)
    raise SystemExit(1) from None
finally:
    if raw_capture is not None:
        raw_capture.stop()
    for process in reversed(PROCESSES): stop(process)
    for reader in LOG_THREADS: reader.join(timeout=3)
    for log in LOGS: log.close()
