#!/usr/bin/env python3
"""Verify archived original HLS packets and fire-test execution receipts."""

import argparse
import json
import math
from pathlib import Path
import struct
import subprocess
import sys

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / 'qa/gstreamer'))
from integrated_checks import transport_metrics


def clip_lshape_frames(root):
    """Inspect the short marked clip by decoded content, independent of HLS polling latency."""
    segments = json.loads((root / 'archive/segments.json').read_text())
    matches = []
    start = 0.0

    for segment in [item for item in segments if item['generation'] == 0]:
        end = start + segment['duration']

        if end < 238 or start > 256:
            start = end
            continue

        filename = root / 'archive' / segment['file']
        frames = json.loads(subprocess.check_output([
            'ffprobe', '-v', 'error', '-select_streams', 'v:0', '-show_frames',
            '-show_entries', 'frame=best_effort_timestamp_time', '-of', 'json', str(filename),
        ], timeout=30))['frames']
        raw = subprocess.check_output(['ffmpeg', '-v', 'error', '-i', str(filename),
            '-pix_fmt', 'rgb24', '-f', 'rawvideo', 'pipe:1'], timeout=30)
        frame_size = 640 * 360 * 3

        for number in range(len(raw) // frame_size):
            frame = memoryview(raw)[number * frame_size:(number + 1) * frame_size]

            def count(x, y, width, height, colour):
                return sum(max(abs(frame[(row * 640 + column) * 3 + component] - value)
                    for component, value in enumerate(colour)) <= 25
                    for row in range(y, y + height) for column in range(x, x + width))

            art = count(500, 100, 20, 20, [40, 80, 24])
            video = count(4, 130, 16, 16, [0, 0, 255])
            top_border = count(4, 122, 16, 2, [40, 80, 24])

            if art >= 390 and video >= 250 and top_border == 32:
                matches.append({'segment': segment['file'], 'frame': number,
                    'pts': float(frames[number]['best_effort_timestamp_time']),
                    'art_pixels': art, 'video_pixels': video})

        start = end

    consecutive = bool(matches) and all(abs(after['pts'] - before['pts'] - .04) < .001
        for before, after in zip(matches, matches[1:]))
    result = {'passed': consecutive and len(matches) >= 25, 'stable_frames': len(matches),
        'first': matches[0] if matches else None, 'last': matches[-1] if matches else None,
        'note': 'Exact artwork and lower-left video regions during the marked clip; not transition parity certification.'}

    if result['passed']:
        middle = matches[len(matches) // 2]
        subprocess.run(['ffmpeg', '-v', 'error', '-y', '-i', str(root / 'archive' / middle['segment']),
            '-vf', f"select=eq(n\\,{middle['frame']})", '-frames:v', '1', '-q:v', '2',
            str(root / 'captures/output-fire-clip-lshape-aligned.jpg')], check=True, timeout=30)
        result['selected'] = middle

    return result


def analyse(root):
    report = json.loads((root / 'report.json').read_text())
    samples = [json.loads(line) for line in (root / 'samples.jsonl').read_text().splitlines()]
    ui = json.loads((root / 'ui-actions.json').read_text())
    ready = json.loads((root / 'ui-ready.json').read_text())
    plan = json.loads((root / 'ready.json').read_text())
    observations = [json.loads(line) for line in (root / 'observations.jsonl').read_text().splitlines()]
    observations = [sample for sample in observations if 'error' not in sample]
    observed = {str(value) for value in report['admitted']}
    # Graphics events have a separate ID namespace and never become programme events.
    required_events = {str(value) for value in report['events'][:-2]} | {str(ready['live_event'])}
    checks = {
        'runner_completed': report['seconds_requested'] >= 900 and samples[-1]['elapsed'] >= 898,
        'all_programme_events_admitted': required_events <= observed,
        'browser_controls': {item['action'] for item in ui['receipts']} == {
            'logo-hidden-in-browser', 'logo-shown-in-browser', 'gc-taken-in-browser'},
        'no_browser_page_errors': not ui['errors'],
        'real_output_captures': len(report['captures']) == 20,
        'no_fatal_log_lines': not report['fatal_log_lines'],
        'no_error_log_lines': not report['error_log_lines'],
        'observed_picture_motion': report['picture']['changes'] > 100,
        'all_native_actions': len(report['actions']) == 23,
        'scheduled_graphic_entered': any(
            150 <= sample['elapsed'] < 185 and any(item['template_id'] == plan['notice_id']
                for item in sample['on_air']) for sample in observations),
        'scheduled_graphic_left': any(
            195 <= sample['elapsed'] < 210 and all(item['template_id'] != plan['notice_id']
                for item in sample['on_air']) for sample in observations),
        'manual_gc_and_scheduled_graphic_overlap': any(
            {plan['gc_id'], plan['notice_id']} <= {item['template_id'] for item in sample['on_air']}
            for sample in observations),
        'manual_gc_out_preserves_scheduled_notice': any(
            167 <= sample['elapsed'] < 185
            and plan['notice_id'] in {item['template_id'] for item in sample['on_air']}
            and plan['gc_id'] not in {item['template_id'] for item in sample['on_air']}
            for sample in observations),
        'permanent_models_after_restart': any(
            680 <= sample['elapsed'] < 700 and {'Fire logo', 'Fire clock', 'Fire crawl', 'Fire GIF'}
                <= {model['template']['name'] for model in sample['channel'] if model['on']}
            for sample in observations),
        'updated_data_after_restart': any(
            680 <= sample['elapsed'] < 700 and any(source['name'] == 'Fire prices'
                and source['rows'] == [{'price': '5.20'}] and source['error'] is None
                for source in sample['sources']) for sample in observations),
        'short_graphics_expired': any(sample['elapsed'] >= 890 and not sample['on_air'] for sample in observations),
        'healthy_channel_outside_restart': all(
            sample['health']['status'] == 'ok'
            and any(channel['id'] == 1 and channel['state'] == 'on_air'
                for channel in sample['health']['channels'])
            for sample in samples if sample['elapsed'] >= 15 and not 630 <= sample['elapsed'] < 675),
    }
    generations = []
    audio_checks = []
    notice_checks = []
    marked_clip = clip_lshape_frames(root)
    checks['clip_lshape_decoded_interval'] = marked_clip['passed']
    capture_rechecks = [incident for incident in report['incidents'] if marked_clip['passed']
        and incident.get('what') == 'clip-lshape: clip artwork (0 pixels, minimum 200)']
    checks['no_unexplained_runtime_incidents'] = all(incident in capture_rechecks for incident in report['incidents'])

    for name in ['lshape-and-gc', 'commercial']:
        capture = next((item for item in report['captures'] if item['name'] == name), None)

        if capture is None:
            checks[name + '_logo_suppressed'] = False
            continue

        raw = subprocess.check_output(['ffmpeg', '-v', 'error', '-i', str(root / 'archive' / capture['segment']),
            '-vf', r'select=eq(n\,25)', '-frames:v', '1', '-pix_fmt', 'rgb24', '-f', 'rawvideo', 'pipe:1'], timeout=30)
        orange = sum(max(abs(raw[(row * 640 + column) * 3 + component] - colour)
            for component, colour in enumerate([224, 160, 32])) <= 25
            for row in range(16, 52) for column in range(552, 620))
        checks[name + '_logo_suppressed'] = orange == 0

    for name, should_exist in [('full-screen-notice', True), ('notice-ended', False)]:
        capture = next((item for item in report['captures'] if item['name'] == name), None)

        if capture is None:
            checks[name + '_notice_pixels'] = False
            continue

        raw = subprocess.check_output([
            'ffmpeg', '-v', 'error', '-i', str(root / 'archive' / capture['segment']),
            '-vf', r'select=eq(n\,25)', '-frames:v', '1', '-pix_fmt', 'rgb24', '-f', 'rawvideo', 'pipe:1',
        ], timeout=30)
        pixels = sum(max(abs(raw[(row * 640 + column) * 3 + component] - colour)
            for component, colour in enumerate([208, 32, 96])) <= 25
            for row in range(80, 88) for column in range(70, 110))
        passed = pixels >= 200 if should_exist else pixels == 0
        checks[name + '_notice_pixels'] = passed
        notice_checks.append({'stage': name, 'matched_pixels': pixels, 'expected_present': should_exist})

    for capture in report['captures']:
        expected = 880 if capture['name'] in [
            'srt-live', 'srt-recovered', 'lshape-and-gc', 'full-screen-notice',
            'notice-ended', 'rtmp-scene', 'rtmp-fullscreen', 'udp-live',
            'udp-recovered', 'hls-live',
        ] else 550 if capture['name'] == 'commercial' else None

        if expected is None:
            continue

        raw = subprocess.check_output([
            'ffmpeg', '-v', 'error', '-i',
            str(root / 'archive' / capture['segment']), '-ss', '0.8', '-t', '0.25',
            '-vn', '-ac', '1', '-ar', '48000', '-f', 'f32le', 'pipe:1',
        ], timeout=30)
        pcm = struct.unpack('<' + 'f' * (len(raw) // 4), raw)
        rms = math.sqrt(sum(value * value for value in pcm) / max(1, len(pcm)))
        energies = {}

        for frequency in [440, 550, 660, 880]:
            coefficient = 2 * math.cos(2 * math.pi * frequency / 48000)
            previous, older = 0.0, 0.0

            for value in pcm:
                current = value + coefficient * previous - older
                older, previous = previous, current

            energies[frequency] = previous * previous + older * older - coefficient * previous * older

        db = 20 * math.log10(max(rms, 1e-10))
        dominant = max(energies, key=energies.get)
        passed = len(pcm) >= 10000 and db > -45 and dominant == expected
        checks[capture['name'] + '_audio'] = passed
        audio_checks.append({'stage': capture['name'], 'rms_dbfs': db,
            'expected_hz': expected, 'dominant_hz': dominant, 'samples': len(pcm), 'passed': passed})

    for playlist in sorted((root / 'archive').glob('g*.m3u8')):
        probed = subprocess.run([
            'ffprobe', '-v', 'error', '-show_streams', '-show_packets',
            '-of', 'json', str(playlist),
        ], capture_output=True, check=True, timeout=90)
        data = json.loads(probed.stdout)
        timing = []

        for stream in data['streams']:
            if stream['codec_type'] not in ['audio', 'video']:
                continue

            packets = [packet for packet in data['packets'] if packet['stream_index'] == stream['index']]
            dts = [float(packet['dts_time']) for packet in packets if 'dts_time' in packet]
            deltas = [after - before for before, after in zip(dts, dts[1:])]
            timing.append({
                'type': stream['codec_type'], 'codec': stream['codec_name'],
                'packets': len(packets), 'first_dts': dts[0], 'last_dts': dts[-1],
                'minimum_step': min(deltas), 'maximum_step': max(deltas),
                'monotonic': all(value > 0 for value in deltas),
            })

        decoded = subprocess.run([
            'ffmpeg', '-v', 'error', '-xerror', '-i', str(playlist),
            '-map', '0:v:0', '-map', '0:a:0', '-f', 'null', '-',
        ], capture_output=True, timeout=180)
        (root / f'{playlist.stem}-decode.log').write_bytes(decoded.stderr)
        # Inspect the original TS bytes; remuxing would conceal transport discontinuities.
        joined = root / 'archive' / '.transport-analysis.ts'

        try:
            with joined.open('wb') as target:
                for line in playlist.read_text().splitlines():
                    if line and not line.startswith('#'):
                        target.write((playlist.parent / line).read_bytes())

            transport = transport_metrics(joined)
            intra_segment = []

            for line in playlist.read_text().splitlines():
                if line and not line.startswith('#'):
                    metrics = transport_metrics(playlist.parent / line)
                    intra_segment.extend({'segment': line, **error} for error in metrics['continuity_errors'])

            # splitmuxsink creates a fresh TS muxer for each HLS file. Its CC
            # restarts at file boundaries are distinct from dropped TS packets
            # inside a segment; preserve their count and validate that scope.
            transport['cross_file_counter_restarts'] = len(transport['continuity_errors'])
            transport['continuity_errors'] = intra_segment
        finally:
            joined.unlink(missing_ok=True)

        generations.append({'playlist': playlist.name, 'timing': timing,
            'decodes': decoded.returncode == 0, 'transport': transport})
        checks[playlist.stem + '_decodes'] = decoded.returncode == 0
        checks[playlist.stem + '_dts'] = len(timing) == 2 and all(
            item['monotonic'] and item['maximum_step'] <= .12 for item in timing)
        checks[playlist.stem + '_av_tail'] = len(timing) == 2 and abs(timing[0]['last_dts'] - timing[1]['last_dts']) <= .25
        checks[playlist.stem + '_transport'] = not transport['continuity_errors'] and not transport['transport_errors']

    checks['restart_generations'] = len(generations) == 2
    status = ('PASS_WITH_CAPTURE_RECHECK' if capture_rechecks else 'PASS') if all(checks.values()) else 'FAIL'
    result = {'status': status, 'raw_runner_status': report['status'], 'checks': checks,
        'generations': generations, 'audio_checks': audio_checks, 'notice_checks': notice_checks,
        'resources': {'observation_start': min(sample['elapsed'] for sample in observations),
            'observation_end': max(sample['elapsed'] for sample in observations),
            'peak_backend_rss_mib': max(sample['resources']['peak_rss_kib'] for sample in observations) / 1024,
            'peak_sampled_cpu_one_core_percent': max(sample['resources']['cpu_one_core_percent'] or 0 for sample in observations)},
        'runner_incidents': report['incidents'],
        'marked_clip': marked_clip, 'capture_rechecks': capture_rechecks,
        'log_errors_for_review': report['error_log_lines'],
        'scope': 'Local synthetic inputs and HLS output; not a 24-hour or physical hardware qualification.'}
    (root / 'analysis.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))
    return 0 if result['status'] in ['PASS', 'PASS_WITH_CAPTURE_RECHECK'] else 1


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('root', type=Path)
    arguments = parser.parse_args()
    raise SystemExit(analyse(arguments.root))
