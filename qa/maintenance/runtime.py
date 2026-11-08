#!/usr/bin/env python3
"""Prove a scheduled interruption and return through real disposable HLS output."""
import argparse
import datetime as dt
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time
from zoneinfo import ZoneInfo

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'graphics'))
from compatibility import Client


def media_command(container, tool, *arguments, capture=False):
    command = (['docker', 'exec', container] if container else []) + [tool, *map(str, arguments)]
    return subprocess.run(command, check=True, stdout=subprocess.PIPE if capture else subprocess.DEVNULL,
                          stderr=subprocess.PIPE, timeout=60).stdout


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--base', default='http://127.0.0.1:8787')
    parser.add_argument('--work-root', required=True, type=Path)
    parser.add_argument('--auth-file', required=True, type=Path)
    parser.add_argument('--container', help='Optional local container with the same mounted work-root path')
    parser.add_argument('--artifacts', required=True, type=Path)
    args = parser.parse_args()
    if os.getenv('NOAR_E2E_STAGING') != '1' or os.getenv('NOAR_E2E_DISPOSABLE') != '1':
        raise SystemExit('Explicit disposable staging opt-in is required')
    root = args.work_root.resolve()
    if not (root / '.qa-maintenance-owned').is_file():
        raise SystemExit('Missing owned disposable maintenance marker')
    client = Client(args.base, json.loads(args.auth_file.read_text()))
    channel = client.ok('GET', '/api/channel/1')
    storage = Path(channel['storage']).resolve()
    if not storage.is_relative_to(root) or not storage.is_dir():
        raise SystemExit('Refusing to modify storage outside the owned fixture')
    artifacts = args.artifacts.resolve()
    artifacts.mkdir(parents=True, exist_ok=True)
    client.ok('POST', '/api/control/1/process', {'command': 'stop'})
    # Other synthetic schedules in this independent copy must not own this run.
    for event in client.ok('GET', '/api/schedule/1/events'):
        if event['enabled']:
            event['enabled'] = False
            client.ok('PUT', f"/api/schedule/1/{event['id']}", event)
    for name, color, seconds, frequency in [
        ('a-maintenance-background.mp4', 'blue', 30, 440),
        ('z-maintenance-event.mp4', 'red', 3, 880),
    ]:
        media_command(args.container, 'ffmpeg', '-hide_banner', '-loglevel', 'error', '-y',
                      '-f', 'lavfi', '-i', f'color=c={color}:s=640x360:r=25:d={seconds}',
                      '-f', 'lavfi', '-i', f'sine=frequency={frequency}:sample_rate=48000:duration={seconds}',
                      '-c:v', 'libx264', '-preset', 'ultrafast', '-pix_fmt', 'yuv420p', '-threads', '2',
                      '-c:a', 'aac', '-shortest', storage / name)
    config = client.ok('GET', '/api/playout/config/1')
    output = next(item for item in client.ok('GET', '/api/playout/outputs/1') if item['name'] == 'hls')
    config['output'].update(id=output['id'], mode='hls', width=640, height=360, fps=25,
                            video_codec='libx264', audio_codec='aac', audio_bitrate=128,
                            hls_playlist_name='maintenance', hls_segment_duration=2,
                            hls_list_size=64, hls_variants=[])
    config['output']['video_options']['preset'] = 'ultrafast'
    config['recording']['enable'] = False
    config['processing']['mode'] = 'playlist'
    programme_start = dt.datetime.now(ZoneInfo(channel['timezone'])) - dt.timedelta(seconds=3)
    config['playlist'].update(day_start=programme_start.strftime('%H:%M:%S'), length='00:00:30', infinit=False)
    config['storage'].update(extensions=['mp4'], shuffle=False)
    client.ok('PUT', '/api/playout/config/1', config)
    # Anchor this short fixture near now, so Continue resumes the interrupted
    # blue clip. Folder mode deliberately resumes its next source instead.
    playlist_status, _ = client.call('POST', '/api/playlist/1', {
        'channel': 'Maintenance synthetic programme', 'date': programme_start.strftime('%Y-%m-%d'),
        'program': [{'source': str(storage / 'a-maintenance-background.mp4'),
                     'in': 0, 'out': 30, 'duration': 30}],
    })
    if playlist_status == 409:
        saved = client.ok('GET', '/api/playlist/1?date=' + programme_start.strftime('%Y-%m-%d'))
        assert len(saved['program']) == 1
        clip = saved['program'][0]
        assert clip['source'] == str(storage / 'a-maintenance-background.mp4')
        assert clip['in'] == 0 and clip['out'] == 30 and clip['duration'] == 30
    else:
        assert playlist_status == 200, f'Synthetic playlist save failed: HTTP {playlist_status}'
    client.ok('POST', '/api/control/1/lshape', {'action': 'off'})
    start = dt.datetime.now(ZoneInfo(channel['timezone'])) + dt.timedelta(seconds=10)
    event = client.ok('POST', '/api/schedule/1', {
        'kind': 'media', 'title': 'Maintenance scheduled interruption',
        'sources': [str(storage / 'z-maintenance-event.mp4')],
        'start_date': start.strftime('%Y-%m-%d'), 'start_time': start.strftime('%H:%M:%S'),
        'end_time': None, 'repeat_days': 0, 'entry_mode': 'interrupt',
        'after_mode': 'continue', 'show_in_guide': True, 'retry_seconds': 30, 'enabled': True,
    })
    log_path = root / 'logs/noar_1.log'
    log_offset = log_path.stat().st_size if log_path.exists() else 0
    samples = []
    running_seen = False
    done_seen = False
    started = time.monotonic()
    try:
        client.ok('POST', '/api/control/1/process', {'command': 'start'})
        while time.monotonic() - started < 35:
            health = client.ok('GET', '/health')
            status = client.ok('GET', '/api/schedule/1/status')
            day = client.ok('GET', f"/api/schedule/1?date={event['start_date']}")
            occurrence = next(item for item in day['occurrences'] if item['event']['id'] == event['id'])
            active = status.get('active')
            running_seen |= bool(active and active['event_id'] == event['id'])
            done_seen |= occurrence['status'] == 'done'
            samples.append({'elapsed': round(time.monotonic() - started, 3), 'health': health,
                            'active_event_id': active['event_id'] if active else None,
                            'occurrence_status': occurrence['status']})
            if running_seen and done_seen and time.monotonic() - started >= 20:
                break
            time.sleep(0.4)
        assert running_seen and done_seen, 'Scheduled event did not run and complete'
        assert samples[-1]['health']['channels'][0]['state'] == 'on_air'
    finally:
        client.ok('POST', '/api/control/1/process', {'command': 'stop'})
    playlist = Path(channel['public']) / 'live/maintenance.m3u8'
    assert playlist.is_file(), 'Missing actual HLS rendition'
    # Freeze the stopped writer's segment list for finite offline analysis.
    # Every referenced TS is unchanged actual output; only ENDLIST is added.
    analysis = playlist.with_name('maintenance-analysis.m3u8')
    manifest = playlist.read_text()
    analysis.write_text(manifest + ('\n#EXT-X-ENDLIST\n' if '#EXT-X-ENDLIST' not in manifest else ''))
    playlist = analysis
    probe = json.loads(media_command(args.container, 'ffprobe', '-v', 'error', '-show_streams',
                                    '-show_format', '-of', 'json', playlist, capture=True))
    video = next(stream for stream in probe['streams'] if stream['codec_type'] == 'video')
    audio = next(stream for stream in probe['streams'] if stream['codec_type'] == 'audio')
    assert (video['width'], video['height']) == (640, 360)
    assert int(audio['sample_rate']) == 48000 and audio['channels'] == 2
    # A small decoded central crop identifies the source colors independently
    # of channel graphics near the edges. No generated image substitutes HLS.
    rgb = media_command(args.container, 'ffmpeg', '-hide_banner', '-loglevel', 'error', '-i', playlist,
                        '-map', '0:v:0', '-vf', 'crop=16:16:312:172,format=rgb24',
                        '-f', 'rawvideo', '-', capture=True)
    size = 16 * 16 * 3
    colors = []
    for offset in range(0, len(rgb), size):
        frame = rgb[offset:offset + size]
        red = sum(frame[0::3]) / 256
        blue = sum(frame[2::3]) / 256
        colors.append('red' if red > 150 and blue < 70 else 'blue' if blue > 150 and red < 70 else 'other')
    red_frames = [index for index, color in enumerate(colors) if color == 'red']
    assert len(red_frames) >= 25, 'Scheduled red source absent from HLS'
    before = [i for i in range(red_frames[0]) if colors[i] == 'blue']
    after = [i for i in range(red_frames[-1] + 1, len(colors)) if colors[i] == 'blue']
    assert len(before) >= 25 and len(after) >= 25, 'Missing blue programme before/after event'
    captures = []
    for name, index in [('before', before[len(before) // 2]),
                        ('event', red_frames[len(red_frames) // 2]), ('after', after[len(after) // 2])]:
        filename = f'output-hls-{name}.jpg'
        media_command(args.container, 'ffmpeg', '-hide_banner', '-loglevel', 'error', '-y',
                      '-i', playlist, '-vf', f'select=eq(n\\,{index})', '-frames:v', '1',
                      '-update', '1', '-q:v', '2', artifacts / filename)
        captures.append({'file': filename, 'decoded_frame': index})
    log = log_path.read_bytes()[log_offset:].decode()
    assert '[ERROR]' not in log, 'Scoped channel log contains ERROR'
    (artifacts / 'hls-channel-log.txt').write_text(log)
    (artifacts / 'hls-health.json').write_text(json.dumps(samples, indent=2) + '\n')
    segments = [playlist.parent / line for line in playlist.read_text().splitlines()
                if line and not line.startswith('#')]
    evidence = {'status': 'PASS', 'event_id': event['id'], 'event_duration': event['duration'],
                'running_observed': running_seen, 'done_observed': done_seen,
                'playout_mode': 'playlist', 'resume_mode': 'continue',
                'playlist_day_start': programme_start.strftime('%H:%M:%S'),
                'video': {'codec': video['codec_name'], 'width': video['width'], 'height': video['height'],
                          'frame_rate': video['r_frame_rate']},
                'audio': {'codec': audio['codec_name'], 'rate': audio['sample_rate'], 'channels': audio['channels']},
                'decoded_frames': len(colors), 'red_frames': len(red_frames),
                'blue_before': len(before), 'blue_after': len(after), 'captures': captures,
                'segment_sha256': {p.name: sha256(p) for p in segments}, 'channel_errors': 0,
                'scope': 'Actual synthetic HLS/scheduler interruption and programme return; not full frame parity.'}
    (artifacts / 'hls-evidence.json').write_text(json.dumps(evidence, indent=2) + '\n')
    print(f"PASS scheduled HLS interruption/return, {len(colors)} decoded frames, {len(red_frames)} event frames")


if __name__ == '__main__':
    main()
