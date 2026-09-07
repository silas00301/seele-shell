#!/usr/bin/env python3
"""Local-only HTTP tests; never consult a user's Home Assistant connection."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from unittest.mock import patch

SOURCE = Path(sys.argv.pop(1)).resolve()
spec = importlib.util.spec_from_file_location('ha', SOURCE)
ha = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ha)
TOKEN = 'test-private-token-never-in-output'


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def do_GET(self):
        self.server.requests.append((self.path, self.headers.get('Authorization'), None))
        if self.server.delay:
            time.sleep(self.server.delay)
        self.send_response(self.server.code)
        if self.server.code == 302:
            self.send_header('Location', self.server.url + '/redirect-target')
        self.end_headers()
        try:
            self.wfile.write(self.server.response)
        except (BrokenPipeError, ConnectionResetError):
            pass

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        self.server.requests.append((self.path, self.headers.get('Authorization'), body))
        self.send_response(200)
        self.end_headers()
        self.wfile.write(b'[]')


class HomeAssistant(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.path = Path(self.temp.name) / 'connection.json'
        self.env = patch.dict(os.environ, {'SEELE_HOME_ASSISTANT_CONFIG': str(self.path)})
        self.env.start()
        self.server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        self.server.url = 'http://127.0.0.1:' + str(self.server.server_port)
        self.server.requests = []
        self.server.code = 200
        self.server.delay = 0
        self.server.response = json.dumps([
            {'entity_id': 'light.desk', 'state': 'on', 'attributes': {'friendly_name': 'Desk'}},
            {'entity_id': 'sensor.temperature', 'state': '21', 'attributes': {'unit_of_measurement': '°C', 'private_attribute': TOKEN}},
            {'entity_id': 'lock.door', 'state': 'locked'},
            {'entity_id': 'sensor.secret', 'state': TOKEN},
        ]).encode()
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        self.config = {'url': self.server.url, 'token': TOKEN, 'entities': ['light.desk', 'sensor.temperature', 'lock.door', 'switch.missing']}

    def tearDown(self):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join()
        self.env.stop()
        self.temp.cleanup()

    def write(self):
        self.path.write_text(json.dumps(self.config))
        self.path.chmod(0o600)

    def test_missing_is_disabled_without_requests(self):
        self.assertEqual(ha.run(['status']), {'configured': False, 'connected': False, 'entities': [], 'error': ''})
        self.assertEqual(self.server.requests, [])

    def test_private_file_enforced(self):
        self.write()
        self.path.chmod(0o644)
        with self.assertRaisesRegex(ha.Problem, '0600'):
            ha.load_config()
        self.path.chmod(0o600)
        target = self.path.with_suffix('.real')
        self.path.rename(target)
        self.path.symlink_to(target)
        with self.assertRaises(ha.Problem):
            ha.load_config()

    def test_invalid_config_rejected(self):
        for key, values in {'url': ['https://user:pass@host', 'https://host?q=secret', 'https://host/#secret', 'ftp://host', 'http://host:bad'], 'token': ['', 'x\ny'], 'entities': [['light.desk', 'light.desk'], ['../lock.door'], ['sensor.x'] * 33]}.items():
            original = self.config[key]
            for value in values:
                self.config[key] = value
                self.write()
                with self.assertRaises(ha.Problem):
                    ha.load_config()
            self.config[key] = original

    def test_selected_states_and_permissions(self):
        self.write()
        result = ha.run(['status'])
        entries = result['entities']
        self.assertTrue(result['connected'])
        self.assertEqual([e['entity_id'] for e in entries], self.config['entities'])
        self.assertEqual([e['controllable'] for e in entries], [True, False, False, False])
        self.assertFalse(entries[-1]['available'])
        self.assertEqual(entries[1]['unit'], '°C')
        self.assertNotIn(TOKEN, json.dumps(result))
        self.assertNotIn('private_attribute', json.dumps(result))
        self.assertEqual(self.server.requests, [('/api/states', 'Bearer ' + TOKEN, None)])

    def test_display_redacts_token_and_controls(self):
        self.config['entities'] = [{'entity_id': 'light.desk', 'name': TOKEN + '\nDesk'}]
        self.write()
        self.assertEqual(ha.run(['status'])['entities'][0]['name'], '[redacted]Desk')

    def test_explicit_safe_services(self):
        for domain in ['light', 'switch', 'input_boolean']:
            self.config['entities'] = [domain + '.desk']
            self.write()
            for state in ['on', 'off']:
                ha.run(['set', domain + '.desk', state])
                self.assertEqual(self.server.requests[-2], ('/api/services/' + domain + '/turn_' + state, 'Bearer ' + TOKEN, {'entity_id': domain + '.desk'}))

    def test_controls_require_allowlist_and_explicit_intent(self):
        self.config['entities'].append('alarm_control_panel.home')
        self.write()
        for args in [['set', 'lock.door', 'off'], ['set', 'alarm_control_panel.home', 'off'], ['set', 'light.other', 'on'], ['set', 'light.desk', 'toggle'], ['service', 'lock.unlock', '{}']]:
            with self.assertRaises(ha.Problem):
                ha.run(args)
        self.assertEqual(self.server.requests, [])

    def test_http_errors_do_not_leak_server_body(self):
        self.write()
        self.server.code = 401
        self.server.response = TOKEN.encode()
        result = subprocess.run([sys.executable, str(SOURCE), 'status'], text=True, capture_output=True, timeout=3)
        self.assertEqual(result.returncode, 1)
        self.assertEqual(result.stderr, '')
        self.assertNotIn(TOKEN, result.stdout)
        self.assertIn('Access was denied', json.loads(result.stdout)['error'])

    def test_redirect_never_receives_authorization(self):
        self.write()
        self.server.code = 302
        with self.assertRaisesRegex(ha.Problem, 'redirected'):
            ha.run(['status'])
        self.assertEqual(len(self.server.requests), 1)

    def test_response_size_and_shape_bounded(self):
        self.write()
        for response in [b'not json', b'{}', b'[' + b' ' * ha.MAX_RESPONSE]:
            self.server.response = response
            with self.assertRaises(ha.Problem):
                ha.run(['status'])

    def test_request_timeout(self):
        self.write()
        self.server.delay = .2
        with patch.object(ha, 'REQUEST_TIMEOUT', .02):
            with self.assertRaisesRegex(ha.Problem, 'unreachable'):
                ha.run(['status'])


unittest.main()
