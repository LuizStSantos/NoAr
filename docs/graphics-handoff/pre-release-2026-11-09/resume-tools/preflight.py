from pathlib import Path
import json
import urllib.request
root = Path('/workspace/noar-cloud/review-20261109/browser')
opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
with opener.open('http://127.0.0.1:8787/api/setup') as response:
    setup = json.load(response)
assert setup['required'] is True, 'Fresh, uninitialized staging is required.'
assert setup['settings']['storage'] == str(root / 'data/media'), 'Storage is not the runner-owned fixture.'
state = root / 'session.json'
if state.exists():
    from datetime import datetime, timezone
    archive = root / ('session-before-fresh-' + datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ') + '.json')
    state.rename(archive)
print('PASS: fresh setup guard and runner-owned storage.')
