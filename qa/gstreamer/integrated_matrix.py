#!/usr/bin/env python3
"""Run the seven output modes against four external Scheduler input protocols.

Uses the same staging opt-in, disposable binary and dedicated MediaMTX guards
as integrated_smoke.py. This is a bounded smoke matrix, not all 28 pairings.
"""

import argparse
import datetime as dt
import json
import os
from pathlib import Path
import subprocess
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--recovery', action='store_true', help='Interrupt each synthetic input for four seconds')
    args = parser.parse_args()
    if os.environ.get('NOAR_GST_STAGING') != '1':
        raise SystemExit('Refusing to run without NOAR_GST_STAGING=1')
    qa = Path(__file__).resolve().parent
    directory = qa/'artifacts'/('matrix-' + dt.datetime.now(dt.timezone.utc).strftime('%Y%m%dT%H%M%S%fZ'))
    directory.mkdir(parents=True)
    cases = [('udp', False, 'srt'), ('udp', True, 'udp'),
             ('srt', False, 'rtmp'), ('srt', True, 'srt'),
             ('hls', False, 'hls'), ('hls', True, 'srt'),
             ('rtmp', False, 'rtmp')]
    results = []
    for output, cues, live in cases:
        name = output + ('-scte35' if cues else '-normal')
        env = dict(os.environ, NOAR_GST_TEST_OUTPUT=output, NOAR_GST_TEST_SCTE35=str(int(cues)),
                   NOAR_GST_TEST_INPUT=live, NOAR_GST_TEST_SCHEDULE='automatic',
                   NOAR_GST_TEST_RECOVERY=str(int(args.recovery)))
        with (directory/(name + '.log')).open('w') as log:
            process = subprocess.run([sys.executable, str(qa/'integrated_smoke.py')], env=env,
                                     stdout=log, stderr=subprocess.STDOUT)
        try:
            execution = json.loads((directory/(name + '.log')).read_text())
            report = execution['report']
            result = {'case': name, 'input': live, 'status': report['status'],
                      'artifacts': str(Path(execution['artifacts']).relative_to(qa)),
                      'failed_checks': [key for key, value in report['checks'].items() if value != 'PASS']}
            if process.returncode and result['status'] == 'PASS':
                result['status'] = 'FAIL'
        except (ValueError, KeyError):
            result = {'case': name, 'input': live, 'status': 'FAIL',
                      'error': 'Execution did not produce a valid report; inspect its case log'}
        results.append(result)
        print(json.dumps(result), flush=True)
        (directory/'report.json').write_text(json.dumps({'cases': results, 'recovery': args.recovery,
            'status': 'PASS' if len(results) == len(cases) and all(item['status'] == 'PASS' for item in results)
                      else 'FAIL' if any(item['status'] != 'PASS' for item in results) else 'NOT_RUN'}, indent=2))
    raise SystemExit(0 if all(item['status'] == 'PASS' for item in results) else 1)


if __name__ == '__main__':
    main()
