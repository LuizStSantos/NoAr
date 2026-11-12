#!/usr/bin/env python3
"""Run browser writes against a fresh local server, never an existing instance.

Requires npm ci in the repository and qa/e2e, plus a local runtime image or
--native with a binary and installed libraries. --binary also works with an image.
The frontend always comes from this checkout: served by Vite, or with
--embedded by the backend itself, as the Docker image serves it, with its
security headers. No playout is started.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import tempfile
import time
import urllib.error
import urllib.request
import uuid

ROOT = Path(__file__).resolve().parents[2]


def wait_for(url: str, process: subprocess.Popen, *, setup: bool = False) -> None:
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    deadline = time.monotonic() + 60
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RuntimeError(f'Staging process exited with {process.returncode}; inspect its log.')
        try:
            with opener.open(url, timeout=2) as response:
                if response.status == 200:
                    if setup and not json.load(response).get('required'):
                        raise RuntimeError('Refusing an already initialized server.')
                    return
        except (urllib.error.URLError, TimeoutError, ConnectionError):
            pass
        time.sleep(0.2)
    raise RuntimeError(f'Staging did not become ready: {url}')


def read_api(path: str, token: str) -> object:
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    request = urllib.request.Request(f'http://127.0.0.1:8787{path}',
                                     headers={'Authorization': f'Bearer {token}'})
    with opener.open(request, timeout=10) as response:
        return json.load(response)


def stop_backend(process: subprocess.Popen, docker: list[str], name: str,
                 env: dict[str, str], native: bool) -> None:
    if native:
        process.terminate()
    else:
        subprocess.run([*docker, 'stop', '--time', '10', name], env=env,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
    try:
        process.wait(timeout=15)
    except subprocess.TimeoutExpired:
        process.terminate()
        process.wait(timeout=5)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument('--image', help='Local NoAr runtime image (not pulled automatically)')
    mode.add_argument('--native', action='store_true', help='Run a local binary with installed native libraries')
    parser.add_argument('--binary', type=Path, help='Current Linux binary to mount over the image entrypoint')
    parser.add_argument('--chromium', help='Optional installed Chromium executable')
    parser.add_argument('--embedded', action='store_true',
                        help='Test the interface the backend serves itself; needs a binary built '
                             'with embed_frontend and without debug assertions')
    args = parser.parse_args()
    # Fixed ports match vite.config.ts; fail instead of reusing another server.
    for port in (8787,) if args.embedded else (8787, 5757):
        with socket.socket() as check:
            check.bind(('127.0.0.1', port))
    binary = args.binary.resolve() if args.binary else None
    if binary and not binary.is_file():
        parser.error('--binary must name an existing Linux executable')
    if args.native and binary is None:
        parser.error('--native requires --binary')
    work = ROOT / 'work'
    work.mkdir(exist_ok=True)
    artifacts = Path(tempfile.mkdtemp(prefix='e2e-', dir=work))
    data = artifacts / 'data'
    for folder in ('db', 'media', 'playlists', 'public', 'logs'):
        (data / folder).mkdir(parents=True)
    shutil.copyfile(ROOT / 'tests_assets/storage/media_mix/ad.mp4', data / 'media/e2e-clip.mp4')
    for filename in ('logo.png', 'guide-logo.png'):
        shutil.copyfile(ROOT / 'tests_assets/storage/logo.png', data / 'media' / filename)
    env = dict(os.environ)
    # Select only the local managed daemon, preserving registry/proxy settings.
    for key in ('DOCKER_HOST', 'DOCKER_CONTEXT', 'DOCKER_TLS', 'DOCKER_TLS_VERIFY', 'DOCKER_CERT_PATH'):
        env.pop(key, None)
    docker = ['docker', '--host=unix:///var/run/docker.sock']
    if args.image:
        subprocess.run([*docker, 'image', 'inspect', args.image], env=env, check=True, stdout=subprocess.DEVNULL)
    name = f'noar-e2e-{uuid.uuid4().hex[:12]}'
    command = [*docker, 'run', '--rm', '--name', name, '--user', f'{os.getuid()}:{os.getgid()}',
               '--publish', '127.0.0.1:8787:8787',
               '--mount', f'type=bind,src={data},dst=/e2e-data',
               '--env', 'DB=/e2e-data/db/noar.db', '--env', 'TZ=America/Sao_Paulo']
    for key, folder in [('STORAGE', 'media'), ('PLAYLISTS', 'playlists'), ('PUBLIC', 'public'), ('LOGS', 'logs')]:
        command += ['--env', f'NOAR_SETUP_{key}=/e2e-data/{folder}']
    if binary:
        command += ['--mount', f'type=bind,src={binary},dst=/e2e-noar,readonly', '--entrypoint', '/e2e-noar']
    elif args.image:
        # Avoid the production timezone entrypoint writing /etc as a test user.
        command += ['--entrypoint', '/usr/local/bin/noar']
    if args.native:
        env['DB'] = str(data / 'db/noar.db')
        env['TZ'] = 'America/Sao_Paulo'
        for key, folder in [('STORAGE', 'media'), ('PLAYLISTS', 'playlists'), ('PUBLIC', 'public'), ('LOGS', 'logs')]:
            env[f'NOAR_SETUP_{key}'] = str(data / folder)
        command = [str(binary), '--listen', '127.0.0.1:8787', '--log-to-console']
    else:
        command += [args.image, '--listen', '0.0.0.0:8787', '--log-to-console']
    backend = frontend = None
    print(f'Disposable staging evidence: {artifacts}', flush=True)
    try:
        with (artifacts / 'backend.log').open('w') as back_log, (artifacts / 'frontend.log').open('w') as front_log:
            backend = subprocess.Popen(command, env=env, stdout=back_log, stderr=subprocess.STDOUT)
            wait_for('http://127.0.0.1:8787/api/setup', backend, setup=True)
            if args.embedded:
                base_url = 'http://127.0.0.1:8787'
                wait_for(f'{base_url}/login', backend)
            else:
                base_url = 'http://127.0.0.1:5757'
                frontend = subprocess.Popen([str(ROOT / 'node_modules/.bin/vite'), '--strictPort'], cwd=ROOT,
                                            stdout=front_log, stderr=subprocess.STDOUT)
                wait_for(f'{base_url}/login', frontend)
            test_env = {**env, 'NOAR_E2E_STAGING': '1', 'NOAR_E2E_WRITES': '1',
                        'NOAR_E2E_DISPOSABLE': '1', 'NOAR_E2E_BASE_URL': base_url,
                        'NOAR_E2E_EMBEDDED': '1' if args.embedded else '0',
                        'NOAR_E2E_STATE_PATH': str(artifacts / 'session.json'),
                        'NOAR_E2E_STORAGE': str(data / 'media') if args.native else '/e2e-data/media'}
            if args.chromium:
                test_env['NOAR_E2E_CHROMIUM'] = args.chromium
            result = subprocess.run(['npm', 'test'], cwd=ROOT / 'qa/e2e', env=test_env, check=False)
            if result.returncode:
                return result.returncode
            session = json.loads((artifacts / 'session.json').read_text())
            token = next(item['value'] for origin in session['origins']
                         for item in origin['localStorage'] if item['name'] == 'token')
            playlists = list((data / 'playlists').rglob('*.json'))
            if len(playlists) != 2:
                raise RuntimeError('Expected both browser-saved playlists before restart.')
            dates = sorted(json.loads(path.read_text())['date'] for path in playlists)
            paths = ['/api/schedule/1/events', *[f'/api/playlist/1?date={date}' for date in dates]]
            before = {path: read_api(path, token) for path in paths}
            events = before['/api/schedule/1/events']
            if not any(event['title'] == 'E2E restart persistence' for event in events):
                raise RuntimeError('Missing browser-created event before restart.')
            stop_backend(backend, docker, name, env, args.native)
            backend = subprocess.Popen(command, env=env, stdout=back_log, stderr=subprocess.STDOUT)
            wait_for('http://127.0.0.1:8787/api/setup', backend)
            after = {path: read_api(path, token) for path in paths}
            if before != after:
                raise RuntimeError('Stored playlists or scheduled events changed across backend restart.')
            (artifacts / 'restart-result.json').write_text(json.dumps({
                'status': 'PASS', 'playlist_dates': dates,
                'scheduled_events': len(events), 'checks': paths,
            }, indent=2) + '\n')
            print('PASS: both playlists and the scheduled event survive a backend restart.', flush=True)
            return 0
    finally:
        if frontend is not None:
            frontend.terminate()
            try:
                frontend.wait(timeout=10)
            except subprocess.TimeoutExpired:
                frontend.kill()
                frontend.wait()
        if backend is not None:
            stop_backend(backend, docker, name, env, args.native)
        # Retain logs, but never retain operator tokens or the disposable DB.
        shutil.rmtree(data)
        (artifacts / 'session.json').unlink(missing_ok=True)


if __name__ == '__main__':
    raise SystemExit(main())
