#!/usr/bin/env python3
"""Prove GIF playback and owned template scheduling through disposable real HLS.

Run after the browser setup on the same owned, fresh fixture. Uses its normally
issued browser session, never arbitrary operator credentials or production data.
"""
import argparse
import datetime as dt
import hashlib
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import time

import numpy as np
from PIL import Image

from compatibility import Client


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--work-root', type=Path, required=True)
    parser.add_argument('--screenshots', type=Path, required=True)
    args = parser.parse_args()
    root = args.work_root.resolve()
    if os.getenv('NOAR_E2E_STAGING') != '1' or os.getenv('NOAR_E2E_DISPOSABLE') != '1':
        raise SystemExit('Explicit disposable staging opt-in required')
    if not (root / '.qa-graphics-owned').is_file():
        raise SystemExit('Owned fixture marker required')
    state = json.loads((root / 'browser-state.json').read_text())
    client = Client.__new__(Client)
    client.base, client.key = 'http://127.0.0.1:8787', None
    client.token = next(item['value'] for origin in state['origins'] for item in origin['localStorage'] if item['name'] == 'token')
    channel = client.ok('GET', '/api/channel/1')
    if Path(channel['storage']).resolve() != root / 'media':
        raise SystemExit('Server is not the selected disposable fixture')
    screenshots = args.screenshots
    screenshots.mkdir(parents=True, exist_ok=True)
    captures, events = [], []
    log_path = root / 'logs/noar_1.log'
    log_start = log_path.stat().st_size if log_path.exists() else 0
    client.ok('POST', '/api/control/1/process', {'command': 'stop'})
    for event in client.ok('GET', '/api/schedule/1/graphics'):
        client.ok('DELETE', f"/api/schedule/1/graphics/{event['id']}")
    roles = client.ok('GET', '/api/graphics/1/channel')
    for role in roles:
        if role['kind'] in ['logo', 'ticker', 'permanent']:
            client.ok('POST', f"/api/graphics/1/channel/{role['template']['id']}", {'on': False})
    client.ok('POST', '/api/graphics/1/out', {})
    client.ok('POST', '/api/control/1/lshape', {'action':'off'})
    for model in client.ok('GET', '/api/graphics/1/templates'):
        if model['name'].startswith('QA '):
            client.ok('DELETE', f"/api/graphics/1/templates/{model['id']}")
    channel['timezone'] = 'UTC'
    client.ok('PATCH', '/api/channel/1', channel)
    config = client.ok('GET', '/api/playout/config/1')
    output = next(item for item in client.ok('GET', '/api/playout/outputs/1') if item['name'] == 'hls')
    config['output'].update({'id': output['id'], 'mode': 'hls', 'width': 640, 'height': 360, 'fps': 25,
        'video_codec': 'libx264', 'audio_codec': 'aac', 'audio_bitrate': 128,
        'hls_playlist_name': 'animated', 'hls_segment_duration': 2, 'hls_list_size': 40, 'hls_variants': []})
    config['output']['video_options']['preset'] = 'ultrafast'
    config['recording']['enable'] = False
    config['playlist'].update({'day_start': '00:00:00', 'length': '24:00:00', 'infinit': True})
    client.ok('PUT', '/api/playout/config/1', config)
    shutil.copyfile(Path(__file__).resolve().parents[2] / 'tests_assets/graphics.gif', root / 'media/animated.gif')
    subprocess.run(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-f', 'lavfi', '-i', 'color=c=0x505050:s=640x360:r=25', '-f', 'lavfi', '-i', 'sine=frequency=440:sample_rate=48000', '-t', '180', '-c:v', 'libx264', '-preset', 'ultrafast', '-pix_fmt', 'yuv420p', '-c:a', 'aac', '-y', str(root / 'media/synthetic.mp4')], check=True)
    today = dt.datetime.now(dt.timezone.utc).date()
    for date in [today, today + dt.timedelta(days=1)]:
        program = [{'source': str(root / 'media/synthetic.mp4'), 'in': 0, 'out': 180, 'duration': 180, 'title': 'Deterministic gray source'} for _ in range(481)]
        status, body = client.call('POST', '/api/playlist/1', {'channel': 'Animated graphics QA', 'date': str(date), 'program': program})
        if status == 409 and body == f'Playlist from {date}, already exists!':
            saved = client.ok('GET', f'/api/playlist/1?date={date}')
            assert len(saved['program']) == len(program)
            assert all(item['source'] == program[0]['source'] and item['out'] == 180 for item in saved['program'])
        else:
            assert status == 200, (status, body)

    def create(name, layers, **extra):
        return client.ok('POST', '/api/graphics/1/templates', {'name': name, 'group': 'QA graphics', 'duration': 0, 'layers': layers, **extra})

    def image(x, y, width, height):
        return {'id': 'gif', 'kind': 'image', 'image': 'animated.gif', 'x': x, 'y': y, 'width': width, 'height': height,
                'animation_in': 'none', 'animation_out': 'none'}

    def wait(predicate, description, seconds=50):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            value = predicate()
            if value:
                return value
            time.sleep(.25)
        raise AssertionError(description)

    playlist = root / 'public/live/animated.m3u8'

    def playlist_lines():
        try:
            return playlist.read_text().splitlines()
        except FileNotFoundError:
            return []

    def capture(name, gif_point=None):
        wait(lambda: playlist.exists(), 'HLS playlist missing')
        initial = set(playlist_lines())
        def fresh():
            segments = [line for line in playlist_lines() if line.endswith('.ts')]
            return playlist.parent / segments[-2] if len(set(segments) - initial) >= 10 else None
        segment = wait(fresh, 'HLS did not publish ten fresh segments', 75)
        raw = subprocess.check_output(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-i', str(segment), '-f', 'rawvideo', '-pix_fmt', 'rgb24', 'pipe:1'])
        frames = np.frombuffer(raw, np.uint8).reshape(-1, 360, 640, 3)
        health = client.ok('GET', '/health')
        assert health['status'] == 'ok' and health['channels'][0]['state'] == 'on_air'
        record = {'stage': name, 'segment_sha256': hashlib.sha256(segment.read_bytes()).hexdigest(), 'health': health, 'frame_count': len(frames)}
        Image.fromarray(frames[0]).save(screenshots / f'output-{name}.jpg', quality=92)
        if gif_point:
            x, y = gif_point
            colors = frames[:, y:y+8, x:x+8].mean(axis=(1, 2))
            red = np.flatnonzero((colors[:, 0] > colors[:, 1] + 70) & (colors[:, 0] > 150))
            green = np.flatnonzero((colors[:, 1] > colors[:, 0] + 70) & (colors[:, 1] > 150))
            assert len(red) and len(green), (name, colors.tolist())
            for label, indexes in [('red', red), ('green', green)]:
                Image.fromarray(frames[indexes[0]]).save(screenshots / f'output-{name}-{label}.jpg', quality=92)
            record['gif_red_frames'], record['gif_green_frames'] = len(red), len(green)
        captures.append(record)
        (root / 'animated-progress.json').write_text(json.dumps({'status':'IN_PROGRESS','captures':captures,'events':events},indent=2)+'\n')
        print('CAPTURED', name, flush=True)
        return frames, record

    permanent = create('QA permanent GIF', [image(10, 10, 10, 100 * 48 / 360)], always_on=True)
    scheduled = create('QA scheduled GIF', [image(5, 65, 10, 100 * 48 / 360), {'id': 'text', 'kind': 'text', 'text': '{headline}', 'x': 17, 'y': 65, 'width': 70, 'height': 10, 'font_size': 5}])
    manual = create('QA manual stays', [{'id': 'box', 'kind': 'box', 'x': 65, 'y': 80, 'width': 25, 'height': 15, 'fill': '#2040ff'}])
    manual_take = client.ok('POST', '/api/graphics/1/take', {'template_id': manual['id'], 'duration': 0})['take_id']
    client.ok('POST', '/api/control/1/process', {'command': 'start'})
    frames, record = capture('gif-permanent', (90, 60))
    transparent = frames[:, 39:43, 66:70].astype(float)
    record['transparent_region_max_error'] = float(np.max(np.abs(transparent - 79)))
    assert record['transparent_region_max_error'] <= 12

    def schedule(model, start_after=3, end_after=35, values=None):
        now = dt.datetime.now(dt.timezone.utc)
        start, end = now + dt.timedelta(seconds=start_after), now + dt.timedelta(seconds=end_after)
        event = client.ok('POST', '/api/schedule/1/graphics', {'title': model['name'], 'start_date': start.strftime('%Y-%m-%d'), 'start_time': start.strftime('%H:%M:%S'), 'end_time': end.strftime('%H:%M:%S'), 'template': {'template_id': model['id'], 'values': values or {}, 'records': {}}})
        events.append(event)
        return event

    def instances(model):
        return [item for item in client.ok('GET', '/api/graphics/1/on-air') if item['template_id'] == model['id'] and not item['leaving']]

    schedule(scheduled, values={'headline': 'SCHEDULED BULLETIN'})
    instance = wait(lambda: instances(scheduled), 'Scheduled GIF did not start')[0]
    frames, record = capture('scheduled-gif-on', (60, 258))
    record['take_id'] = instance['take_id']
    record['white_text_pixels'] = int(np.all(frames[0, 235:270, 105:610] > 210, axis=2).sum())
    assert record['white_text_pixels'] > 100
    wait(lambda: not instances(scheduled), 'Scheduled GIF did not leave')
    frames, record = capture('scheduled-gif-off')
    record['former_gif_region_max_error'] = int(np.max(np.abs(frames[:, 236:275, 49:93].astype(float) - 79)))
    assert record['former_gif_region_max_error'] <= 12
    assert any(item['take_id'] == manual_take for item in client.ok('GET', '/api/graphics/1/on-air'))

    lshape = create('QA animated L-shape', [{'id': 'base', 'kind': 'box', 'fill': '#173d5e', 'x': 0, 'y': 0, 'width': 100, 'height': 100}, image(0, 0, 100, 100), {'id': 'video', 'kind': 'video', 'x': 25, 'y': 0, 'width': 75, 'height': 75}])
    schedule(lshape)
    wait(lambda: instances(lshape), 'Scheduled L-shape did not start')
    capture('scheduled-animated-lshape-on', (500, 330))
    wait(lambda: not instances(lshape), 'Scheduled L-shape did not leave')
    frames, record = capture('scheduled-animated-lshape-off')
    assert np.max(np.abs(frames[:, 310:340, 200:240].astype(float) - 79)) <= 12
    assert any(item['take_id'] == manual_take for item in client.ok('GET', '/api/graphics/1/on-air'))

    logo = next(item['template'] for item in client.ok('GET', '/api/graphics/1/channel') if item['kind'] == 'logo')
    logo['always_on'] = False
    primary = next(layer for layer in logo['layers'] if layer['id'] == logo['control_layer_id'])
    primary.update({'x': 100 * (640 - 42 - 12) / 640, 'y': 100 * 12 / 360, 'width': 100 * 42 / 640, 'height': 100 * 42 / 360})
    logo = client.ok('PUT', f"/api/graphics/1/templates/{logo['id']}", logo)
    database = sqlite3.connect(root / 'db/noar.db')
    client.ok('POST', '/api/control/1/logo', {'action':'show'})
    before = database.execute("SELECT enabled FROM graphics_channel_bindings WHERE channel_id=1 AND role='logo'").fetchone()[0]
    assert not before and not client.ok('GET', '/api/control/1/logo')['hidden']
    first = schedule(logo, end_after=15)
    second = schedule(logo, start_after=8, end_after=30)
    wait(lambda: next(item for item in client.ok('GET', '/api/graphics/1/channel') if item['kind'] == 'logo')['on'], 'Scheduled disabled logo did not start')
    frames, record = capture('scheduled-logo-overlap-on')
    record['logo_non_background_pixels'] = int(np.any(np.abs(frames[0, 12:54, 586:628].astype(float) - 79) > 25, axis=2).sum())
    assert record['logo_non_background_pixels'] > 20
    assert next(item for item in client.ok('GET', '/api/graphics/1/channel') if item['kind'] == 'logo')['on']
    wait(lambda: not next(item for item in client.ok('GET', '/api/graphics/1/channel') if item['kind'] == 'logo')['on'], 'Last logo lease did not restore the disabled state')
    frames, record = capture('scheduled-logo-overlap-off')
    assert np.max(np.abs(frames[:, 12:54, 586:628].astype(float) - 79)) <= 12
    after = database.execute("SELECT enabled FROM graphics_channel_bindings WHERE channel_id=1 AND role='logo'").fetchone()[0]
    assert before == after == 0
    transition = measure_logo_transition(root, screenshots, primary)
    complete_log = log_path.read_bytes()
    prior_log = complete_log[:log_start].decode()
    channel_log = complete_log[log_start:].decode()
    (root / 'animated-channel-log.txt').write_text(channel_log)
    (root / 'animated-prior-channel-log.txt').write_text(prior_log)
    assert 'ERROR' not in channel_log
    evidence = {'status': 'PASS', 'captures': captures, 'events': events, 'permanent_template_id': permanent['id'], 'manual_take_id_preserved': manual_take, 'logo_startup_enabled_before': before, 'logo_startup_enabled_after': after, 'channel_error_count': 0, 'prior_log_error_count': prior_log.count('[ERROR]'), 'log_scope': 'Bytes appended during this owned test run', 'logo_transitions': transition, 'channel_log_sha256': hashlib.sha256(channel_log.encode()).hexdigest()}
    (root / 'animated-evidence.json').write_text(json.dumps(evidence, indent=2) + '\n')
    print('PASS GIF frames, alpha, owned schedules, L-shape and overlapping disabled-logo leases', flush=True)


def measure_logo_transition(root, screenshots, primary):
    """Keep a closed HLS snapshot and check intermediate entry/exit pixels."""
    live = root / 'public/live'
    target = root / 'logo-transition'
    target.mkdir(exist_ok=True)
    text = (live / 'animated.m3u8').read_text()
    segments = [line for line in text.splitlines() if line.endswith('.ts')]
    for name in segments:
        shutil.copyfile(live / name, target / name)
    snapshot = target / 'snapshot.m3u8'
    snapshot.write_text(text + ('\n#EXT-X-ENDLIST\n' if '#EXT-X-ENDLIST' not in text else ''))
    command = ['ffmpeg', '-hide_banner', '-loglevel', 'error', '-i', str(snapshot)]
    raw = subprocess.check_output(command + ['-an', '-vf', 'crop=42:42:586:12', '-f', 'rawvideo', '-pix_fmt', 'rgb24', 'pipe:1'])
    frames = np.frombuffer(raw, np.uint8).reshape(-1, 42, 42, 3)
    strength = np.abs(frames.astype(float) - 79).mean(axis=(1, 2, 3))
    peak = float(strength.max())
    assert peak > 5
    normalized = strength / peak
    steady = np.flatnonzero(normalized >= .9)
    entry = np.flatnonzero((normalized > .1) & (normalized < .9) & (np.arange(len(frames)) < steady[0]))
    exit_frames = np.flatnonzero((normalized > .1) & (normalized < .9) & (np.arange(len(frames)) > steady[-1]))
    record = {'animation_in': primary['animation_in'], 'animation_out': primary['animation_out'], 'configured_ms': primary['animation_ms'],
              'entry_intermediate_frames': len(entry), 'exit_intermediate_frames': len(exit_frames), 'peak_mean_rgb_distance_from_gray': peak,
              'segments': {name: hashlib.sha256((target / name).read_bytes()).hexdigest() for name in segments}}
    for kind, indexes, animation in [('in', entry, primary['animation_in']), ('out', exit_frames, primary['animation_out'])]:
        if animation == 'fade':
            assert len(indexes) >= 2, (kind, record)
            index = int(indexes[len(indexes) // 2])
            subprocess.run(command + ['-vf', f'select=eq(n\\,{index})', '-frames:v', '1', '-q:v', '2', '-update', '1', '-y', str(screenshots / f'output-scheduled-logo-fade-{kind}.jpg')], check=True)
    (root / 'logo-transition-evidence.json').write_text(json.dumps(record, indent=2) + '\n')
    return record


if __name__ == '__main__':
    main()
