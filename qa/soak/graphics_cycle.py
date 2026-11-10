"""Synthetic model API workload for the disposable soak, with durable receipts.

Graphics deliberately avoid the soak's video-only ROI (x=10..45%, y=15..50%).
Receipts establish API behavior, not rendered artwork correctness.
"""

import datetime as dt
import json
import struct
import time
import zlib


def write_png(path, color, width=80, height=48):
    def chunk(kind, data):
        return struct.pack('!I', len(data)) + kind + data + struct.pack('!I', zlib.crc32(kind + data))

    rows = (b'\x00' + bytes(color) * width) * height
    path.write_bytes(b'\x89PNG\r\n\x1a\n'
                     + chunk(b'IHDR', struct.pack('!2I5B', width, height, 8, 2, 0, 0, 0))
                     + chunk(b'IDAT', zlib.compress(rows)) + chunk(b'IEND', b''))


class GraphicsCycler:
    def __init__(self, request, directory, media, clock=time.monotonic):
        self.request, self.directory, self.media = request, directory, media
        self.clock, self.started = clock, clock()
        self.steps = self.receipts = 0
        self.models = {}
        self.action = 'setup'

    def call(self, path, data=None, method=None):
        code, response = self.request(path, data, method)
        receipt = {'elapsed': round(self.clock() - self.started, 3),
                   'utc': dt.datetime.now(dt.timezone.utc).isoformat(), 'action': self.action,
                   'method': method or 'GET', 'path': path, 'request': data,
                   'status': code, 'response': response}
        with (self.directory / 'graphics-receipts.jsonl').open('a') as handle:
            handle.write(json.dumps(receipt) + '\n')
        self.receipts += 1
        if not 200 <= code < 300:
            raise RuntimeError(f'Graphics {method or "GET"} {path}: {code} {response}')
        return response

    def model(self, role, layers, assign=False):
        model = self.call('/api/graphics/1/templates', {
            'name': 'Soak ' + role, 'group': 'Soak', 'duration': 0,
            'always_on': False, 'layers': layers}, 'POST')
        if assign:
            model = self.call(f'/api/graphics/1/templates/{model["id"]}/assignment', {
                'role': role, 'control_layer_id': 'art' if role != 'ticker' else None}, 'PUT')
        self.models[role] = model

    def setup(self):
        for name, color in [('soak-logo-a.png', (240, 150, 20)),
                            ('soak-logo-b.png', (20, 180, 240)),
                            ('soak-art.png', (20, 40, 100))]:
            write_png(self.media / name, color)
        self.model('logo', [{'id': 'art', 'kind': 'image', 'image': 'soak-logo-a.png',
                            'x': 84, 'y': 4, 'width': 12.5, 'height': 13.3333333333}], assign=True)
        self.model('lshape', [
            {'id': 'art', 'kind': 'image', 'image': 'soak-art.png',
             'x': 0, 'y': 0, 'width': 100, 'height': 100},
            {'id': 'picture', 'kind': 'video', 'x': 0, 'y': 0, 'width': 75, 'height': 75}], assign=True)
        self.model('ticker', [{'id': 'crawl', 'kind': 'text',
                              'text': 'SOAK | Continuous scrolling ticker | 0',
                              'x': 5, 'y': 91, 'width': 90, 'height': 8, 'font_size': 4,
                              'fill': '#ffffff', 'scroll': {'direction': 'left', 'speed': 80, 'gap': 80}}],
                   assign=True)
        self.model('gc', [
            {'id': 'panel', 'kind': 'box', 'fill': '#a02020',
             'x': 8, 'y': 80, 'width': 60, 'height': 10},
            {'id': 'caption', 'kind': 'text', 'text': 'SOAK {cycle}', 'font_size': 5,
             'x': 10, 'y': 81, 'width': 56, 'height': 8}])

    def control(self, role, on):
        """Put a channel model on or off air with the take/out commands, then
        check the channel state reports it."""
        model_id = self.models[role]['id']
        if on:
            taken = self.call('/api/graphics/1/take', {'template_id': model_id, 'duration': 0}, 'POST')
            if taken.get('state') != 'on_air':
                raise RuntimeError(f'Graphics {role} take was not acknowledged: {taken}')
        else:
            self.call('/api/graphics/1/out', {'template_id': model_id}, 'POST')
        views = self.call('/api/graphics/1/channel')
        view = next((view for view in views if view['template']['id'] == model_id), None)
        if view is None or view['on'] != on or (on and not view['ready']):
            raise RuntimeError(f'Graphics {role} did not acknowledge requested state {on}: {view}')

    def update(self, role, edit):
        path = f'/api/graphics/1/templates/{self.models[role]["id"]}'
        model = self.call(path)
        edit(model['layers'][0])
        saved = self.call(path, model, 'PUT')
        if saved['layers'][0] != model['layers'][0]:
            raise RuntimeError(f'Graphics {role} edit did not persist')
        self.models[role] = saved

    def tick(self):
        phase = self.steps % 8
        self.action = f'step-{self.steps + 1}-phase-{phase}'
        if phase == 0:
            self.control('logo', True)
            self.control('ticker', True)
        elif phase == 1:
            taken = self.call('/api/graphics/1/take', {
                'template_id': self.models['gc']['id'], 'duration': 0,
                'fields': {'cycle': str(self.steps // 8 + 1)}}, 'POST')
            views = self.call('/api/graphics/1/on-air')
            if taken.get('state') != 'on_air' or not any(
                    view['take_id'] == taken.get('take_id') for view in views):
                raise RuntimeError('Soak GC take was not present in on-air state')
        elif phase == 2:
            self.control('lshape', True)
        elif phase == 3:
            # A playout restart may already have cleared a temporary take.
            for view in self.call('/api/graphics/1/on-air'):
                if view['template_id'] == self.models['gc']['id']:
                    self.call('/api/graphics/1/out', {'take_id': view['take_id']}, 'POST')
        elif phase == 4:
            image = 'soak-logo-b.png' if (self.steps // 8) % 2 == 0 else 'soak-logo-a.png'
            self.update('logo', lambda layer: layer.update(image=image))
        elif phase == 5:
            self.control('lshape', False)
        elif phase == 6:
            self.control('ticker', False)
            self.control('logo', False)
        else:
            self.update('ticker', lambda layer: layer.update(text=f'SOAK | Updated scrolling ticker | {self.steps}'))
            self.control('ticker', True)
            self.control('logo', True)
        self.call('/api/graphics/1/channel')
        self.steps += 1

    def snapshot(self):
        return {'steps': self.steps, 'receipts': self.receipts,
                'model_ids': {role: model['id'] for role, model in self.models.items()},
                'receipts_file': 'graphics-receipts.jsonl',
                'scope': 'API actions/state and independent video motion; no artwork-pixel assertions'}
