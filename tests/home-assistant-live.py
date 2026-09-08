"""Exercise the production worker against private HTTP/WebSocket and keyring fixtures."""
import asyncio
import importlib.util
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

from aiohttp import web, WSMessageTypeError

SOURCE = Path(sys.argv.pop(1)).resolve()
sys.path.insert(0, str(SOURCE.parent))
spec = importlib.util.spec_from_file_location('ha', SOURCE)
ha = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ha)
from home_assistant_live import Live

TOKEN = 'private-fixture-token'


class Worker(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.path = Path(self.temp.name) / 'connection.json'
        self.env = patch.dict(os.environ, {'SEELE_HOME_ASSISTANT_CONFIG': str(self.path)})
        self.env.start()
        self.messages = []
        self.calls = []
        self.sockets = []
        self.confirm = True
        self.allowed = True
        self.state = {'entity_id': 'light.desk', 'state': 'off', 'attributes': {
            'friendly_name': 'Desk', 'supported_color_modes': ['color_temp'],
            'brightness': 128, 'color_temp_kelvin': 3000,
            'min_color_temp_kelvin': 2200, 'max_color_temp_kelvin': 6000,
            'private_attribute': TOKEN}}
        app = web.Application()
        app.router.add_get('/api/websocket', self.websocket)
        app.router.add_get('/api/', self.api)
        self.runner = web.AppRunner(app)
        await self.runner.setup()
        site = web.TCPSite(self.runner, '127.0.0.1', 0)
        await site.start()
        self.url = 'http://127.0.0.1:' + str(site._server.sockets[0].getsockname()[1])
        self.worker = Live(ha, self.messages.append)
        self.worker.config = {'url':self.url,'token':TOKEN,'entities':[{'entity_id':'light.desk','favorite':True,'name':'','room':''}], 'summary':'light.desk'}

    async def asyncTearDown(self):
        if self.worker.connection:
            self.worker.connection.cancel()
            await asyncio.gather(self.worker.connection, return_exceptions=True)
        for socket in self.sockets:
            await socket.close()
        await self.runner.cleanup()
        self.env.stop()
        self.temp.cleanup()

    async def api(self, request):
        if request.headers.get('Authorization') != 'Bearer ' + TOKEN or not self.allowed:
            return web.Response(status=401, text=TOKEN)
        return web.json_response({'message':'API running'})

    async def websocket(self, request):
        socket = web.WebSocketResponse()
        await socket.prepare(request)
        self.sockets.append(socket)
        await socket.send_json({'type':'auth_required'})
        try:
            auth = await socket.receive_json()
        except WSMessageTypeError:
            return socket
        self.assertEqual(auth['access_token'], TOKEN)
        await socket.send_json({'type':'auth_ok'})
        async for message in socket:
            data = json.loads(message.data)
            kind = data['type']
            result = None
            if kind == 'get_states':
                result = [self.state]
            elif kind == 'config/area_registry/list':
                result = [{'area_id':'office','name':'Office'}]
            elif kind == 'config/device_registry/list':
                result = [{'id':'lamp','area_id':'office'}]
            elif kind == 'config/entity_registry/list':
                result = [{'entity_id':'light.desk','device_id':'lamp','area_id':None}]
            elif kind == 'call_service':
                self.calls.append(data)
            await socket.send_json({'id':data['id'],'type':'result','success':True,'result':result})
            if kind == 'call_service' and self.confirm:
                self.state['state'] = 'off' if data['service'] == 'turn_off' else 'on'
                fields = data['service_data']
                if 'percentage' in fields:
                    self.state['attributes']['percentage'] = fields['percentage']
                if 'brightness_pct' in fields:
                    self.state['attributes']['brightness'] = round(fields['brightness_pct'] * 255 / 100)
                if 'color_temp_kelvin' in fields:
                    self.state['attributes']['color_temp_kelvin'] = fields['color_temp_kelvin']
                await self.event(socket)
        return socket

    async def event(self, socket=None):
        await (socket or self.sockets[-1]).send_json({'type':'event','event':{'data':{'entity_id':self.state['entity_id'],'new_state':self.state}}})

    async def until(self, predicate):
        async with asyncio.timeout(4):
            while not predicate():
                await asyncio.sleep(.01)

    async def start(self):
        await self.worker.restart()
        await self.until(lambda:self.worker.connected)

    async def test_live_state_controls_and_private_projection(self):
        await self.start()
        self.assertEqual(self.worker.entry('light.desk')['room'], 'Office')
        self.assertTrue(self.worker.entry('light.desk')['dimmable'])
        self.assertNotIn(TOKEN, json.dumps(self.messages))
        self.assertNotIn('private_attribute', json.dumps(self.messages))
        await self.worker.control({'entity_id':'light.desk','desired':{'brightness':70}})
        self.assertEqual(self.calls[-1]['service_data'], {'brightness_pct':70})
        self.assertEqual(self.worker.pending,{})
        await self.worker.control({'entity_id':'light.desk','desired':{'kelvin':4500}})
        self.assertEqual(self.calls[-1]['service_data'], {'color_temp_kelvin':4500})
        self.state['state'] = 'off'
        await self.event()
        await self.until(lambda:self.worker.entry('light.desk')['state']=='off')
        for desired in ({'kelvin':9000},{'brightness':101},{'state':'toggle'},{'service':'unlock'}):
            with self.assertRaises(ha.Problem):
                await self.worker.control({'entity_id':'light.desk','desired':desired})

    async def test_acknowledgement_waits_for_device_and_reconnects(self):
        await self.start()
        self.confirm = False
        task = asyncio.create_task(self.worker.control({'entity_id':'light.desk','desired':{'state':'on'}}))
        await self.until(lambda:len(self.calls)>0)
        self.assertFalse(task.done(), 'service success is not device acknowledgement')
        with self.assertRaises(ha.Problem):
            await self.worker.control({'entity_id':'light.desk','desired':{'state':'off'}})
        self.state['state'] = 'on'
        await self.event()
        await task
        await self.sockets[-1].close()
        await self.until(lambda:not self.worker.connected)
        self.assertEqual(self.worker.entry('light.desk')['state'],'on')
        await self.until(lambda:self.worker.connected and len(self.sockets)>1)

    async def test_setup_keyring_and_preferences(self):
        with patch.object(ha,'secret',return_value='') as keyring:
            await self.worker.setup({'url':self.url,'token':TOKEN})
            keyring.assert_called_once_with('store', self.url, TOKEN)
        disk = self.path.read_text()
        self.assertNotIn(TOKEN,disk)
        self.assertEqual(self.path.stat().st_mode & 0o777,0o600)
        self.assertNotIn(TOKEN,json.dumps(self.messages))
        await self.worker.preferences({'entities':[{'entity_id':'light.desk','name':'Reading','room':'Study','favorite':True}], 'summary':'light.desk'})
        saved = json.loads(self.path.read_text())
        self.assertEqual(saved['entities'][0]['room'],'Study')
        self.assertEqual(saved['summary'],'light.desk')
        with patch.object(ha,'secret',return_value=TOKEN) as keyring:
            config = ha.load_config()
            self.assertEqual(config['token'],TOKEN)
            keyring.assert_called_once_with('lookup',self.url)

    async def test_failed_setup_preserves_previous_connection(self):
        ha.save_config(self.worker.config)
        before = self.path.read_text()
        with patch.object(ha,'secret',side_effect=ha.Problem('Keyring locked')):
            with self.assertRaises(ha.Problem):
                await self.worker.setup({'url':self.url,'token':TOKEN})
        self.assertEqual(self.path.read_text(),before)
        self.allowed = False
        with patch.object(ha,'secret') as keyring:
            with self.assertRaises(ha.Problem):
                await self.worker.setup({'url':self.url,'token':TOKEN})
            keyring.assert_not_called()

    async def test_legacy_migration_is_transactional(self):
        self.path.write_text(json.dumps(self.worker.config))
        self.path.chmod(0o600)
        with patch.object(ha,'secret',side_effect=ha.Problem('Keyring locked')):
            await self.worker.bootstrap()
        self.assertIn(TOKEN,self.path.read_text())
        self.assertFalse(self.worker.connected)
        with patch.object(ha,'secret',return_value=''):
            await self.worker.bootstrap()
        self.assertNotIn(TOKEN,self.path.read_text())

    async def test_fan_power_and_speed(self):
        self.state = {'entity_id':'fan.desk','state':'off','attributes':{'supported_features':49,'percentage':0,'percentage_step':25}}
        self.worker.config['entities'] = [{'entity_id':'fan.desk'}]
        await self.start()
        self.assertTrue(self.worker.entry('fan.desk')['controllable'])
        await self.worker.control({'entity_id':'fan.desk','desired':{'state':'on'}})
        self.assertEqual((self.calls[-1]['domain'],self.calls[-1]['service']),('fan','turn_on'))
        await self.worker.control({'entity_id':'fan.desk','desired':{'percentage':50}})
        self.assertEqual((self.calls[-1]['service'],self.calls[-1]['service_data']),('set_percentage',{'percentage':50}))
        await self.worker.control({'entity_id':'fan.desk','desired':{'state':'off'}})
        self.assertEqual(self.calls[-1]['service'],'turn_off')
        for desired in ({'percentage':101},{'percentage':-1},{'brightness':50},{'percentage':50,'state':'on'}):
            with self.assertRaises(ha.Problem):
                await self.worker.control({'entity_id':'fan.desk','desired':desired})
        self.worker.states['fan.desk']['attributes']['supported_features'] = 48
        with self.assertRaises(ha.Problem):
            await self.worker.control({'entity_id':'fan.desk','desired':{'percentage':50}})

    async def test_worker_protocol_and_eof(self):
        process = await asyncio.create_subprocess_exec(sys.executable, str(SOURCE), "watch",
            stdin=asyncio.subprocess.PIPE, stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE)
        try:
            message = {"action":"setup","request":7,"url":"ftp://invalid","token":TOKEN}
            process.stdin.write((json.dumps(message) + "\n").encode())
            await process.stdin.drain()
            output = []
            async with asyncio.timeout(4):
                while True:
                    line = await process.stdout.readline()
                    output.append(line.decode())
                    if json.loads(line).get("request") == 7:
                        break
                process.stdin.close()
                remainder, errors = await process.communicate()
            self.assertEqual(process.returncode, 0)
            self.assertEqual(errors, b"")
            self.assertNotIn(TOKEN, "".join(output) + remainder.decode())
        finally:
            if process.returncode is None:
                process.kill()
                await process.wait()


class Keyring(unittest.TestCase):
    def test_token_only_travels_on_stdin(self):
        with patch.object(ha.subprocess,'run') as run:
            run.return_value.returncode = 0
            run.return_value.stdout = ''
            ha.secret('store','https://home.test',TOKEN)
            args, kwargs = run.call_args
            self.assertNotIn(TOKEN,str(args))
            self.assertEqual(kwargs['input'],TOKEN)
            self.assertTrue(kwargs['capture_output'])


unittest.main()
