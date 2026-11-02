"""Observers for disposable NoAr HLS and unmodified MPEG-TS output."""

import json
import datetime as dt
import re
from bisect import bisect_right
from importlib.metadata import version
from pathlib import Path
import socket
import subprocess
import threading
import time
from urllib.parse import urljoin, urlsplit
from urllib.request import urlopen


class UdpCapture:
    """Keep original TS bytes: relays and remuxers can discard SCTE-35."""

    def __init__(self, path):
        self.socket = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        self.socket.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 2 * 1024 * 1024)
        self.socket.bind(('127.0.0.1', 29910))
        self.socket.settimeout(0.1)
        self.observations = []
        self.errors = []
        self.stop_event = threading.Event()
        self.thread = threading.Thread(target=self.receive, args=(path,), daemon=True)
        self.thread.start()

    def receive(self, path):
        with path.open('wb') as output:
            while not self.stop_event.is_set():
                try:
                    data = self.socket.recv(65536)
                except socket.timeout:
                    continue
                except OSError as error:
                    self.errors.append(str(error))
                    return
                output.write(data)
                self.observations.append(time.monotonic())

    def stop(self):
        self.stop_event.set()
        self.thread.join(timeout=2)
        self.socket.close()


def probe(path):
    result = subprocess.run(['ffprobe', '-v', 'error', '-show_streams', '-show_packets',
                             '-of', 'json', str(path)], capture_output=True, check=True, timeout=30)
    return json.loads(result.stdout)


def decodes(path):
    result = subprocess.run(['ffmpeg', '-v', 'error', '-xerror', '-i', str(path),
                             '-map', '0:v:0', '-map', '0:a:0', '-f', 'null', '-'],
                            capture_output=True, timeout=30)
    return result.returncode == 0


class HlsObserver:
    def __init__(self, base, directory, limit=4):
        self.base = base
        self.directory = directory
        self.directory.mkdir()
        self.limit = limit
        self.snapshots = []
        self.segments = {'stream': {}, 'low': {}}
        self.failures = []
        self.target_duration = 2
        self.last_poll = 0
        self.master = None

    def fetch(self, uri):
        url = urljoin(self.base, uri)
        if not url.startswith(self.base) or urlsplit(url).netloc != urlsplit(self.base).netloc:
            raise ValueError('HLS reference escaped the dedicated staging origin')
        with urlopen(url, timeout=5) as response:
            return response.read()

    def poll(self):
        if time.monotonic() - self.last_poll < 0.6:
            return
        self.last_poll = time.monotonic()
        try:
            if self.master is None:
                self.master = self.fetch('master.m3u8').decode()
                (self.directory/'master.m3u8').write_text(self.master)
            for name in self.segments:
                text = self.fetch(name + '.m3u8').decode()
                lines = text.splitlines()
                sequence = int(next(line.split(':', 1)[1] for line in lines
                                    if line.startswith('#EXT-X-MEDIA-SEQUENCE:')))
                target = int(next(line.split(':', 1)[1] for line in lines
                                  if line.startswith('#EXT-X-TARGETDURATION:')))
                self.target_duration = max(target, self.target_duration)
                uris = [line for line in lines if line and not line.startswith('#')]
                self.snapshots.append({'rendition': name, 'sequence': sequence,
                                       'target_duration': target, 'segments': uris, 'playlist': text,
                                       'observed_at': time.monotonic()})
                for index, uri in enumerate(uris, sequence):
                    if index not in self.segments[name]:
                        data = self.fetch(uri)
                        if not data or len(data) % 188 or data[0] != 0x47:
                            raise ValueError('HLS segment is not an aligned transport stream')
                        path = self.directory/f'{name}-{index:06d}.ts'
                        path.write_bytes(data)
                        self.segments[name][index] = path
        except Exception as error:
            self.failures.append(str(error))

    def finish(self, verify):
        (self.directory/'snapshots.json').write_text(json.dumps(self.snapshots, indent=2))
        checks = {'hls_http_references': bool(self.snapshots) and not self.failures,
                  'hls_master_renditions': bool(self.master) and all(
                      value in self.master for value in ['RESOLUTION=640x360', 'RESOLUTION=320x180',
                                                        'stream.m3u8', 'low.m3u8'])}
        metrics = {}
        for name, segments in self.segments.items():
            snapshots = [item for item in self.snapshots if item['rendition'] == name]
            sequences = [item['sequence'] for item in snapshots]
            indices = sorted(segments)
            checks[name + '_rolling_window'] = bool(sequences) and max(sequences) > 0 and all(
                a <= b for a, b in zip(sequences, sequences[1:])) and all(
                len(item['segments']) <= self.limit for item in snapshots)
            checks[name + '_no_missing_segments'] = len(indices) >= 10 and indices == list(
                range(indices[0], indices[-1] + 1))
            if not indices:
                continue
            joined = self.directory/(name + '-all.ts')
            with joined.open('wb') as output:
                for index in indices:
                    output.write(segments[index].read_bytes())
            info = probe(joined)
            packets = verify.packet_metrics(info)
            markers = verify.decode_markers(joined)
            video = next(stream for stream in info['streams'] if stream['codec_type'] == 'video')
            checks[name + '_resolution'] = (video['width'], video['height']) == (
                (640, 360) if name == 'stream' else (320, 180))
            checks[name + '_timestamps'] = all(packets[kind]['dts_regressions'] == 0 and
                packets[kind]['maximum_gap_seconds'] <= (0.08 if kind == 'video' else 0.1)
                for kind in ['video', 'audio'])
            checks[name + '_decode'] = decodes(joined)
            checks[name + '_live_transitions'] = all(markers[kind + '_sequence'].count('live') >= 3
                                                    for kind in ['video', 'audio'])
            metrics[name] = {'segments': len(indices), 'packets': packets, 'markers': markers}
        return checks, {'renditions': metrics, 'http_errors': self.failures,
                        'maximum_target_duration': self.target_duration}

    def scte35(self, requests, preroll=6):
        from threefive import Cue
        checks, metrics, identities = {}, {}, {}
        for name, segments in self.segments.items():
            joined = self.directory/(name + '-all.ts')
            first_video = {index: next(packet for packet in probe(path)['packets']
                                      if packet.get('codec_type') == 'video')
                           for index, path in segments.items()}
            native_checks, native = scte35_metrics(joined, requests, preroll=preroll)
            checks.update({name + '_' + key: value for key, value in native_checks.items()})
            snapshots = [item for item in self.snapshots if item['rendition'] == name]
            ranges, ids, malformed, matched, legacy = {}, set(), [], 0, set()
            cancelled_visible = False
            for snapshot in snapshots:
                text = snapshot['playlist']
                for line in text.splitlines():
                    if not line.startswith('#EXT-X-DATERANGE:'):
                        continue
                    attrs = dict((key, value.strip('"')) for key, value in
                                 re.findall(r'([A-Z0-9-]+)=("[^"]*"|[^,]*)', line.split(':', 1)[1]))
                    try:
                        cue_bytes = bytes.fromhex(attrs['SCTE35-OUT'][2:])
                        cue = Cue(cue_bytes)
                        if not cue.decode() or crc32_mpeg2(cue_bytes):
                            raise ValueError('Invalid DATERANGE SCTE35-OUT')
                        event = cue.command.splice_event_id
                        cancelled_visible |= snapshot is snapshots[-1] and any(request['event_id'] == event
                            and request.get('cancelled_at', float('inf')) < snapshot['observed_at']
                            for request in requests)
                        ids.add((event, attrs['ID']))
                        ranges[event] = attrs
                        if 'SCTE35-IN' in attrs:
                            incoming = Cue(bytes.fromhex(attrs['SCTE35-IN'][2:]))
                            if not incoming.decode() or incoming.command.splice_event_id != event:
                                raise ValueError('Mismatched DATERANGE SCTE35-IN')
                            start, end = [dt.datetime.fromisoformat(attrs[key]) for key in ['START-DATE', 'END-DATE']]
                            if abs((end - start).total_seconds() - float(attrs['DURATION'])) > 0.001:
                                raise ValueError('DATERANGE dates differ from actual duration')
                    except Exception as error:
                        malformed.append(str(error))
                pending_date, pending_cue = None, None
                for line in text.splitlines():
                    if line.startswith('#EXT-X-PROGRAM-DATE-TIME:'):
                        pending_date = dt.datetime.fromisoformat(line.split(':', 1)[1])
                    elif line.startswith('#EXT-X-CUE-OUT:'):
                        pending_cue = 'out'
                    elif line == '#EXT-X-CUE-IN':
                        pending_cue = 'in'
                    elif line and not line.startswith('#'):
                        if pending_cue:
                            legacy.add((line, pending_cue))
                        if pending_date:
                            index = snapshot['sequence'] + snapshot['segments'].index(line)
                            segment = segments.get(index)
                            if segment:
                                first = first_video[index]
                                for attrs in ranges.values():
                                    for field, section in [('START-DATE', 'SCTE35-OUT'), ('END-DATE', 'SCTE35-IN')]:
                                        if field in attrs and dt.datetime.fromisoformat(attrs[field]) == pending_date:
                                            cue = Cue(bytes.fromhex(attrs[section][2:])); cue.decode()
                                            pts = float(first['pts_time'])
                                            if abs(pts - cue.command.pts_time) < 0.001 and 'K' in first['flags']:
                                                matched += 1
                        pending_cue, pending_date = None, None
            expected = {request['event_id'] for request in requests if not request.get('cancelled')}
            cancelled = {request['event_id'] for request in requests if request.get('cancelled')}
            checks[name + '_daterange_payloads'] = bool(ranges) and not malformed
            checks[name + '_daterange_pairs'] = all(event in ranges and 'SCTE35-IN' in ranges[event] for event in expected)
            checks[name + '_cancel_not_in_playlist'] = not cancelled_visible
            checks[name + '_splice_segment_boundaries'] = matched >= 4
            checks[name + '_legacy_out_in'] = sum(kind == 'out' for _, kind in legacy) == 2 and sum(kind == 'in' for _, kind in legacy) == 2
            identities[name] = {(event, identity) for event, identity in ids if event not in cancelled}
            metrics[name] = {'native': native, 'date_ranges': ranges, 'errors': malformed,
                             'boundary_matches': matched, 'legacy_markers': len(legacy)}
        checks['hls_shared_splice_ids'] = identities.get('stream') == identities.get('low') and bool(identities.get('stream'))
        return checks, metrics


def crc32_mpeg2(data):
    crc = 0xFFFFFFFF
    for byte in data:
        crc ^= byte << 24
        for _ in range(8):
            crc = ((crc << 1) ^ (0x04C11DB7 if crc & 0x80000000 else 0)) & 0xFFFFFFFF
    return crc


def ts_payload(packet):
    control = (packet[3] >> 4) & 3
    if control not in (1, 3):
        return b''
    start = 4 if control == 1 else 5 + packet[4]
    return packet[start:]


def transport_metrics(path):
    data = path.read_bytes()
    if len(data) % 188:
        raise ValueError('UDP transport is not aligned to 188-byte packets')
    last = {}
    pids = set()
    errors = []
    transport_errors = 0
    for index in range(0, len(data), 188):
        packet = data[index:index + 188]
        if packet[0] != 0x47:
            raise ValueError('UDP transport lost packet synchronization')
        transport_errors += bool(packet[1] & 0x80)
        pid = (packet[1] & 31) << 8 | packet[2]
        pids.add(pid)
        if pid == 8191:
            continue
        if packet[3] & 0x20 and packet[4] and packet[5] & 0x80:
            last.pop(pid, None)
        if not packet[3] & 0x10:
            continue
        counter = packet[3] & 15
        if pid in last and counter != (last[pid] + 1) % 16:
            errors.append({'packet': index // 188, 'pid': pid, 'previous': last[pid], 'actual': counter})
        last[pid] = counter
    return {'packets': len(data) // 188, 'pids': sorted(pids),
            'continuity_errors': errors, 'transport_errors': transport_errors}


def sections(packets, pid):
    pending = bytearray()
    for index, packet in enumerate(packets):
        if ((packet[1] & 31) << 8 | packet[2]) != pid:
            continue
        payload = ts_payload(packet)
        if not payload:
            continue
        if packet[1] & 0x40:
            pointer = payload[0]
            if pointer and pending:
                pending.extend(payload[1:1 + pointer])
                length = 3 + ((pending[1] & 15) << 8 | pending[2])
                if len(pending) >= length:
                    yield index, bytes(pending[:length])
            pending = bytearray(payload[1 + pointer:])
        else:
            pending.extend(payload)
        while len(pending) >= 3 and pending[0] != 0xFF:
            length = 3 + ((pending[1] & 15) << 8 | pending[2])
            if len(pending) < length:
                break
            yield index, bytes(pending[:length])
            del pending[:length]


def scte35_metrics(path, requests, pid=500, preroll=2):
    # Optional, independent standards decoder; not the NoAr encoder's parser.
    from threefive import Cue
    data = path.read_bytes()
    packets = [data[index:index + 188] for index in range(0, len(data), 188)]
    if any(len(packet) != 188 or packet[0] != 0x47 for packet in packets):
        raise ValueError('Unaligned original SCTE-35 capture')
    pat = next(section for _, section in sections(packets, 0) if section[0] == 0)
    pmt_pid = (pat[10] & 31) << 8 | pat[11]
    pmt_sections = [section for _, section in sections(packets, pmt_pid) if section[0] == 2]
    types = {}
    for section in pmt_sections:
        offset = 12 + ((section[10] & 15) << 8 | section[11])
        while offset + 5 <= len(section) - 4:
            types[(section[offset + 1] & 31) << 8 | section[offset + 2]] = section[offset]
            offset += 5 + ((section[offset + 3] & 15) << 8 | section[offset + 4])
    video_pid = next(key for key, value in types.items() if value == 0x1B)
    video_pts = {}
    for index, packet in enumerate(packets):
        if ((packet[1] & 31) << 8 | packet[2]) != video_pid or not packet[1] & 0x40:
            continue
        payload = ts_payload(packet)
        if len(payload) < 14 or payload[:3] != b'\x00\x00\x01' or not payload[7] & 0x80:
            continue
        pts = ((payload[9] >> 1 & 7) << 30 | payload[10] << 22 |
               (payload[11] >> 1) << 15 | payload[12] << 7 | payload[13] >> 1)
        key = bool(packet[3] & 0x20 and packet[4] and packet[5] & 0x40)
        video_pts[pts] = (index, key)
    decoded = []
    decode_errors = []
    all_crc_valid = True
    lengths_valid = True
    for index, section in sections(packets, pid):
        all_crc_valid &= crc32_mpeg2(section) == 0
        try:
            cue = Cue(section)
            result = cue.get()
            if not result or result['info_section']['splice_command_type'] != 5:
                raise ValueError('Independent decoder did not find splice_insert')
        except Exception as error:
            decode_errors.append(str(error))
            lengths_valid = False
            continue
        command = result['command']
        lengths_valid &= cue.info_section.splice_command_length == cue.command.command_length
        decoded.append({'packet': index, **command})
    checks = {'scte35_pid_and_registration': types.get(pid) == 0x86 and any(b'\x05\x04CUEI' in pmt for pmt in pmt_sections),
              'scte35_pmt_crc': len(pmt_sections) >= 10 and all(crc32_mpeg2(pmt) == 0 for pmt in pmt_sections),
              'scte35_section_crc': bool(decoded) and all_crc_valid,
              'scte35_command_length': bool(decoded) and lengths_valid,
              'scte35_independent_decode': bool(decoded) and not decode_errors}
    counters = [packet[3] & 15 for packet in packets
                if ((packet[1] & 31) << 8 | packet[2]) == pid and packet[3] & 0x10]
    checks['scte35_continuity'] = bool(counters) and all((b - a) % 16 == 1
                                                      for a, b in zip(counters, counters[1:]))
    video_order = sorted((index, pts) for pts, (index, _) in video_pts.items())
    video_indices = [index for index, _ in video_order]
    for request in requests:
        event = request['event_id']
        messages = [item for item in decoded if item['splice_event_id'] == event]
        if request.get('cancelled'):
            checks['scte35_cancel'] = any(item['splice_event_cancel_indicator'] for item in messages)
            continue
        out = [item for item in messages if not item['splice_event_cancel_indicator'] and item['out_of_network_indicator']]
        incoming = [item for item in messages if not item['splice_event_cancel_indicator'] and not item['out_of_network_indicator']]
        checks[f'scte35_{event}_copies'] = len(out) >= 3 and len(incoming) >= 3
        if not out or not incoming:
            continue
        ticks = lambda item: round(item['pts_time'] * 90_000) % (1 << 33)
        start, end = ticks(out[0]), ticks(incoming[0])
        checks[f'scte35_{event}_duration'] = ((end - start) % (1 << 33)) == 540_000 and all(
            item['break_auto_return'] and abs(item['break_duration'] - 6) < 0.001 for item in out)
        checks[f'scte35_{event}_pts_and_idr'] = all(
            pts in video_pts and video_pts[pts][1] for pts in [start, end])
        checks[f'scte35_{event}_preroll'] = all(
            item['packet'] < video_pts.get(ticks(item), (-1, False))[0] for item in [out[0], incoming[0]])
        leads = []
        for item in [out[0], incoming[0]]:
            at = bisect_right(video_indices, item['packet']) - 1
            if at >= 0:
                leads.append(((ticks(item) - video_order[at][1]) % (1 << 33)) / 90_000)
        checks[f'scte35_{event}_preroll_seconds'] = len(leads) == 2 and all(preroll - 0.2 <= lead <= preroll + 0.2 for lead in leads)
    return checks, {'pid': pid, 'commands': decoded, 'pmt_pid': pmt_pid,
                    'decoder': 'threefive ' + version('threefive'), 'decode_errors': decode_errors, 'raw_capture': True}
