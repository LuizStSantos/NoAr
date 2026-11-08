#!/usr/bin/env python3
"""Record graphics state and Linux process resources in the owned fire fixture."""

import argparse
import json
import os
from pathlib import Path
import time
import urllib.request


def observe(root):
    if not (root / '.qa-fire-owned').is_file():
        raise RuntimeError('An explicitly owned fixture is required')

    plan = json.loads((root / 'ready.json').read_text())
    auth = json.loads((root / 'auth.json').read_text())
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    token = ''

    def request(endpoint, body=None):
        headers = {'Content-Type': 'application/json'}

        if token:
            headers['Authorization'] = 'Bearer ' + token

        payload = None if body is None else json.dumps(body).encode()
        with opener.open(urllib.request.Request(plan['base'] + endpoint, data=payload, headers=headers), timeout=10) as response:
            return json.load(response)

    token = request('/auth/login', auth)['access']
    channel = request('/api/channel/1')

    if channel['storage'] != str(root / 'media'):
        raise RuntimeError('Refusing to observe a different installation')

    pid = None

    for process in Path('/proc').iterdir():
        if not process.name.isdigit():
            continue

        try:
            environment = (process / 'environ').read_bytes().split(b'\0')
            command = (process / 'cmdline').read_bytes().split(b'\0')

            if ('DB=' + str(root / 'db/noar.db')).encode() in environment and command[0].endswith(b'/noar'):
                pid = process
                break
        except (PermissionError, FileNotFoundError, ProcessLookupError):
            continue

    if pid is None:
        raise RuntimeError('The owned backend process is unavailable')

    previous = None
    with (root / 'observations.jsonl').open('w') as target:
        while not (root / 'report.json').exists() and time.time() < plan['epoch'] + 930:
            try:
                now = time.monotonic()
                stats = (pid / 'stat').read_text().split()
                ticks = int(stats[13]) + int(stats[14])
                status = dict(line.split(':', 1) for line in (pid / 'status').read_text().splitlines())
                cpu = None if previous is None else 100 * (ticks - previous[1]) / os.sysconf('SC_CLK_TCK') / (now - previous[0])
                previous = (now, ticks)
                sample = {
                    'elapsed': time.time() - plan['epoch'],
                    'resources': {'rss_kib': int(status['VmRSS'].split()[0]),
                        'peak_rss_kib': int(status['VmHWM'].split()[0]),
                        'threads': int(status['Threads']), 'cpu_one_core_percent': cpu,
                        'open_files': len(list((pid / 'fd').iterdir()))},
                    'on_air': request('/api/graphics/1/on-air'),
                    'channel': request('/api/graphics/1/channel'),
                    'sources': request('/api/graphics/1/sources'),
                }
                target.write(json.dumps(sample) + '\n')
                target.flush()
            except (OSError, ValueError) as error:
                target.write(json.dumps({'elapsed': time.time() - plan['epoch'], 'error': str(error)}) + '\n')
                target.flush()

            time.sleep(5)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('root', type=Path)
    arguments = parser.parse_args()
    observe(arguments.root.resolve())
