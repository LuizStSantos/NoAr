#!/usr/bin/env python3
"""Isolate a native decoder's response to changing UDP MPEG-TS tracks."""

import argparse
import json
from pathlib import Path
import socket
import subprocess
import time


def exercise(factory, root):
    if not (root / '.qa-fire-owned').is_file():
        raise RuntimeError('Use an explicitly owned fixture directory')

    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as check:
        check.bind(('127.0.0.1', 25004))

    log = root / f'udp-tracks-{factory}.log'
    phases = []

    with log.open('w') as output:
        receiver = subprocess.Popen([
            'gst-launch-1.0', '-q', factory, 'uri=udp://127.0.0.1:25004', 'name=decode',
            'decode.', '!', 'queue', '!', 'video/x-raw', '!', 'fakesink', 'sync=false',
            'decode.', '!', 'queue', '!', 'audio/x-raw', '!', 'fakesink', 'sync=false',
        ], stdout=output, stderr=subprocess.STDOUT)

        try:
            time.sleep(.5)

            for audio_only in [False, True, False]:
                command = ['ffmpeg', '-nostdin', '-v', 'error', '-re']

                if not audio_only:
                    command += ['-f', 'lavfi', '-i', 'testsrc2=size=320x180:rate=25']

                command += ['-f', 'lavfi', '-i', 'sine=frequency=880:sample_rate=48000', '-t', '4']

                if not audio_only:
                    command += ['-c:v', 'libx264', '-preset', 'ultrafast', '-tune', 'zerolatency', '-g', '25', '-threads', '2']

                command += ['-c:a', 'aac', '-ac', '2', '-f', 'mpegts', 'udp://127.0.0.1:25004?pkt_size=1316']
                subprocess.run(command, check=True, stdout=output, stderr=subprocess.STDOUT, timeout=15)
                phases.append({'audio_only': audio_only, 'receiver_exit': receiver.poll()})

                if receiver.poll() is not None:
                    break
        finally:
            exit_before_cleanup = receiver.poll()

            if exit_before_cleanup is None:
                receiver.terminate()

                try:
                    receiver.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    receiver.kill()
                    receiver.wait(timeout=5)

    version = subprocess.check_output(['gst-inspect-1.0', '--version'], text=True).splitlines()[0]
    result = {'factory': factory, 'native_version': version, 'phases': phases, 'exit_before_cleanup': exit_before_cleanup,
        'survived': len(phases) == 3 and exit_before_cleanup is None}
    (root / f'udp-tracks-{factory}.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result))
    return 0 if result['survived'] else 1


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('factory', choices=['uridecodebin', 'uridecodebin3'])
    parser.add_argument('root', type=Path)
    arguments = parser.parse_args()
    raise SystemExit(exercise(arguments.factory, arguments.root.resolve()))
