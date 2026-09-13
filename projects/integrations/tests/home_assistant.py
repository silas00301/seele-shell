#!/usr/bin/env python3
"""Real Rust binaries against private HTTP/WebSocket and keyring fixtures only."""
import asyncio
import json
import os
from pathlib import Path
import signal
import subprocess
import threading
import sys
import tempfile
import unittest
from aiohttp import web

BINARY = str(Path(sys.argv.pop(1)).resolve())
TOKEN = "private-fixture-token-never-in-output"


class HomeAssistant(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.config_path = self.root / "connection.json"
        self.fake = self.root / "bin"
        self.fake.mkdir()
        keyring = self.fake / "secret-tool"
        keyring.write_text(f"#!{sys.executable}\nimport json,os,sys,time\nfrom pathlib import Path\np=Path(os.environ['FIXTURE_KEYRING'])\na=sys.argv[1:]\nif os.environ.get('FIXTURE_KEYRING_HOLD'):\n p.write_text(json.dumps({{'pid':os.getpid()}}));time.sleep(60)\nif os.environ.get('FIXTURE_KEYRING_FAIL') or (p.parent/'keyring-fail').exists():sys.exit(1)\nif a[0]=='store':\n t=sys.stdin.read();p.write_text(json.dumps({{'args':a,'stdin':t}}))\nelse: print({TOKEN!r})\n")
        keyring.chmod(0o700)
        self.env = {**os.environ, "SEELE_HOME_ASSISTANT_CONFIG": str(self.config_path), "FIXTURE_KEYRING": str(self.root / "keyring.json"), "PATH": str(self.fake) + os.pathsep + os.environ["PATH"], "HTTP_PROXY":"http://127.0.0.1:1", "HTTPS_PROXY":"http://127.0.0.1:1"}
        self.states = [{"entity_id":"light.desk", "state":"off", "attributes":{"friendly_name":"Desk", "supported_color_modes":["color_temp"], "brightness":128, "color_temp_kelvin":3000, "min_color_temp_kelvin":2200, "max_color_temp_kelvin":6000,"private_attribute":TOKEN}}, {"entity_id":"sensor.temperature","state":"21.04","attributes":{"unit_of_measurement":"°C","device_class":"temperature"}}, {"entity_id":"lock.door","state":"locked"}]
        self.calls=[]; self.requests=[]; self.sockets=[]; self.messages=[]; self.worker=None
        self.confirm=True; self.code=200; self.response=None; self.allow=True
        app=web.Application()
        app.router.add_get('/api/websocket',self.websocket)
        app.router.add_get('/api/states',self.http)
        app.router.add_get('/api/',self.http)
        app.router.add_post('/api/services/{domain}/{service}',self.http)
        self.runner=web.AppRunner(app);await self.runner.setup();site=web.TCPSite(self.runner,'127.0.0.1',0);await site.start()
        self.url='http://127.0.0.1:'+str(site._server.sockets[0].getsockname()[1])
        self.config={"url":self.url,"token":TOKEN,"entities":[{"entity_id":"light.desk","name":"","room":"","favorite":True},{"entity_id":"sensor.temperature"},{"entity_id":"lock.door"}],"summary":"sensor.temperature"}

    async def asyncTearDown(self):
        if self.worker:
            if self.worker.returncode is None:self.worker.terminate()
            await asyncio.wait_for(self.worker.wait(),5)
            if self.collector:await self.collector
            self.assertEqual(await self.worker.stderr.read(),b"")
        for socket in self.sockets:await socket.close()
        await self.runner.cleanup();self.temp.cleanup()

    async def http(self,request):
        self.requests.append((request.path,request.headers.get('Authorization'),await request.json() if request.method=='POST' else None))
        if self.code==302:return web.Response(status=302,headers={'Location':self.url+'/redirect'})
        if self.code!=200 or not self.allow:return web.Response(status=self.code if self.code!=200 else 401,text=TOKEN)
        if self.response is not None:return web.Response(body=self.response)
        return web.json_response(self.states if request.path=='/api/states' else {})

    async def websocket(self,request):
        socket=web.WebSocketResponse();await socket.prepare(request);self.sockets.append(socket)
        await socket.send_json({'type':'auth_required'})
        try:auth=await socket.receive_json()
        except Exception:return socket
        self.assertEqual(auth.get('access_token'),TOKEN)
        await socket.send_json({'type':'auth_ok'})
        async for message in socket:
            data=json.loads(message.data);kind=data['type'];result=None
            if kind=='get_states':result=self.states
            elif kind=='config/area_registry/list':result=[{'area_id':'office','name':'Office'}]
            elif kind=='config/device_registry/list':result=[{'id':'lamp','area_id':'office'}]
            elif kind=='config/entity_registry/list':result=[{'entity_id':'light.desk','device_id':'lamp','area_id':None}]
            elif kind=='call_service':self.calls.append(data)
            await socket.send_json({'id':data['id'],'type':'result','success':True,'result':result})
            if kind=='call_service' and self.confirm:
                state=next(s for s in self.states if s['entity_id']==data['target']['entity_id'])
                state['state']='off' if data['service']=='turn_off' else 'on'
                fields=data['service_data']
                if 'brightness_pct' in fields:state['attributes']['brightness']=round(fields['brightness_pct']*255/100)
                if 'color_temp_kelvin' in fields:state['attributes']['color_temp_kelvin']=fields['color_temp_kelvin']
                if 'percentage' in fields:state['attributes']['percentage']=fields['percentage']
                await self.event(state,socket)
        return socket

    async def event(self,state,socket=None):
        await (socket or self.sockets[-1]).send_json({'type':'event','event':{'data':{'entity_id':state['entity_id'],'new_state':state}}})

    def write(self):
        self.config_path.write_text(json.dumps(self.config));self.config_path.chmod(0o600)

    async def command(self,*args):
        p=await asyncio.create_subprocess_exec(BINARY,*args,env=self.env,stdout=asyncio.subprocess.PIPE,stderr=asyncio.subprocess.PIPE)
        stdout,stderr=await asyncio.wait_for(p.communicate(),8)
        self.assertEqual(stderr,b'');self.assertNotIn(TOKEN,stdout.decode());return p.returncode,json.loads(stdout)

    async def until(self,predicate,timeout=5):
        async with asyncio.timeout(timeout):
            while not predicate():await asyncio.sleep(.01)

    async def start(self,write=True):
        if write:self.write()
        self.worker=await asyncio.create_subprocess_exec(BINARY,'watch',env=self.env,stdin=asyncio.subprocess.PIPE,stdout=asyncio.subprocess.PIPE,stderr=asyncio.subprocess.PIPE)
        async def collect():
            while line:=await self.worker.stdout.readline():
                self.assertNotIn(TOKEN,line.decode());self.assertNotIn('private_attribute',line.decode());self.messages.append(json.loads(line))
        self.collector=asyncio.create_task(collect())
        await self.until(lambda:any(m.get('ready') for m in self.messages))
        if write:await self.until(lambda:any(m.get('connected') for m in self.messages))

    async def send(self,value):
        self.worker.stdin.write((json.dumps(value)+'\n').encode());await self.worker.stdin.drain()

    async def rpc(self,value):
        value={**value,'request':len(self.messages)+1000};await self.send(value)
        await self.until(lambda:any(m.get('request')==value['request'] for m in self.messages))
        return next(m for m in self.messages if m.get('request')==value['request'])

    def latest(self):return next(m for m in reversed(self.messages) if 'entities' in m)

    async def test_rest_selection_privacy_and_explicit_services(self):
        self.write();code,result=await self.command('status');self.assertEqual(code,0)
        self.assertEqual([e['controllable'] for e in result['entities']],[True,False,False])
        self.assertEqual(self.requests,[('/api/states','Bearer '+TOKEN,None)])
        for desired in ['on','off']:
            code,result=await self.command('set','light.desk',desired);self.assertEqual(code,0)
            self.assertEqual(self.requests[-2],('/api/services/light/turn_'+desired,'Bearer '+TOKEN,{'entity_id':'light.desk'}))
        before=len(self.requests)
        for args in [('set','lock.door','off'),('set','light.other','on'),('set','light.desk','toggle')]:self.assertEqual((await self.command(*args))[0],1)
        self.assertEqual(len(self.requests),before)

    async def test_rest_missing_private_files_redirect_and_size(self):
        self.assertFalse((await self.command('status'))[1]['configured']);self.assertEqual(self.requests,[])
        self.write();self.config_path.chmod(0o644);self.assertIn('0600',(await self.command('status'))[1]['error'])
        self.config_path.chmod(0o600);target=self.config_path.with_suffix('.real');self.config_path.rename(target);self.config_path.symlink_to(target)
        self.assertEqual((await self.command('status'))[0],1);self.config_path.unlink();target.rename(self.config_path)
        self.code=302;self.assertIn('redirected',(await self.command('status'))[1]['error']);self.assertEqual(len(self.requests),1)
        self.code=401;self.assertIn('Access was denied',(await self.command('status'))[1]['error'])
        self.code=200
        for response in [b'not json',b'{}',b'['+b' '*(2*1024*1024)]:self.response=response;self.assertEqual((await self.command('status'))[0],1)

    async def test_live_controls_catalog_preferences_and_keyring(self):
        await self.start();latest=self.latest();self.assertEqual(latest['entities'][0]['room'],'Office');self.assertEqual(latest['summary_text'],'21 °C')
        disk=self.config_path.read_text();self.assertNotIn(TOKEN,disk)
        keyring=json.loads((self.root/'keyring.json').read_text());self.assertEqual(keyring['stdin'],TOKEN);self.assertNotIn(TOKEN,str(keyring['args']))
        for desired,expected in [({'brightness':70},{'brightness_pct':70}),({'kelvin':4500},{'color_temp_kelvin':4500}),({'state':'off'}, {})]:
            self.assertTrue((await self.rpc({'action':'set','entity_id':'light.desk','desired':desired}))['ok']);self.assertEqual(self.calls[-1]['service_data'],expected)
        for desired in [{'kelvin':9000},{'brightness':101},{'state':'toggle'},{'service':'unlock'},{'percentage':True}]:self.assertFalse((await self.rpc({'action':'set','entity_id':'light.desk','desired':desired}))['ok'])
        self.assertTrue((await self.rpc({'action':'catalog'}))['ok']);await self.until(lambda:any('catalog' in m for m in self.messages))
        preferences={'action':'preferences','entities':[{'entity_id':'light.desk','name':TOKEN+'\nReading','room':'Study','favorite':True}],'summary':'light.desk'}
        self.assertTrue((await self.rpc(preferences))['ok']);disk=json.loads(self.config_path.read_text());self.assertEqual(disk['entities'][0]['name'],'[redacted]Reading');self.assertEqual(disk['entities'][0]['room'],'Study')

    async def test_service_result_waits_for_device_and_reconnects(self):
        await self.start();self.confirm=False
        await self.send({'action':'set','entity_id':'light.desk','desired':{'state':'on'},'request':1});await self.until(lambda:bool(self.calls))
        await asyncio.sleep(.1);self.assertFalse(any(m.get('request')==1 for m in self.messages))
        self.assertFalse((await self.rpc({'action':'set','entity_id':'light.desk','desired':{'state':'off'}}))['ok'])
        self.states[0]['state']='on';await self.event(self.states[0]);await self.until(lambda:any(m.get('request')==1 and m['ok'] for m in self.messages))
        await self.sockets[-1].close();await self.until(lambda:not self.latest()['connected']);self.assertEqual(self.latest()['entities'][0]['state'],'on')
        await self.until(lambda:len(self.sockets)>1 and self.latest()['connected'])

    async def test_failed_setup_preserves_metadata_and_legacy_migration(self):
        self.env['FIXTURE_KEYRING_FAIL']='1';await self.start(write=False)
        result=await self.rpc({'action':'setup','url':self.url,'token':TOKEN});self.assertFalse(result['ok']);self.assertFalse(self.config_path.exists())
        for url in ['ftp://invalid','https://user:pass@host','http://host:bad','http://host\\evil']:
            self.assertFalse((await self.rpc({'action':'setup','url':url,'token':TOKEN}))['ok'])

    async def test_failed_replacement_setup_keeps_connected_metadata(self):
        await self.start();before=self.config_path.read_text();(self.root/'keyring-fail').touch()
        result=await self.rpc({'action':'setup','url':self.url,'token':TOKEN})
        self.assertFalse(result['ok']);self.assertEqual(self.config_path.read_text(),before);self.assertTrue(self.latest()['connected'])
        (self.root/'keyring-fail').unlink();self.allow=False
        result=await self.rpc({'action':'setup','url':self.url,'token':TOKEN})
        self.assertFalse(result['ok']);self.assertEqual(self.config_path.read_text(),before)

    async def test_failed_legacy_migration_preserves_original_file(self):
        self.write();self.env['FIXTURE_KEYRING_FAIL']='1';await self.start(write=False)
        await self.until(lambda:bool(self.latest().get('error')))
        self.assertIn(TOKEN,self.config_path.read_text());self.assertFalse(self.latest()['configured'])

    async def test_fan_controls_and_sigterm_with_open_stdin(self):
        self.states=[{'entity_id':'fan.desk','state':'off','attributes':{'supported_features':49,'percentage':0,'percentage_step':25}}]
        self.config['entities']=[{'entity_id':'fan.desk'}];self.config['summary']='';await self.start()
        for desired in [{'state':'on'},{'percentage':50},{'state':'off'}]:self.assertTrue((await self.rpc({'action':'set','entity_id':'fan.desk','desired':desired}))['ok'])
        self.assertEqual(self.calls[1]['service'],'set_percentage')
        self.worker.terminate();await asyncio.wait_for(self.worker.wait(),3);self.assertEqual(self.worker.returncode,0)

    async def test_sigterm_during_keyring_reaps_owned_helper(self):
        self.env['FIXTURE_KEYRING_HOLD']='1';await self.start(write=False)
        await self.send({'action':'setup','url':self.url,'token':TOKEN,'request':9})
        await self.until(lambda:(self.root/'keyring.json').exists())
        pid=json.loads((self.root/'keyring.json').read_text())['pid']
        self.worker.terminate();await asyncio.wait_for(self.worker.wait(),3)
        with self.assertRaises(ProcessLookupError):os.kill(pid,0)
        self.assertFalse(self.config_path.exists())

    async def test_oversized_request_exits_without_unbounded_allocation(self):
        await self.start(write=False)
        self.worker.stdin.write(b'x' * 32769 + b'\n');await self.worker.stdin.drain()
        await asyncio.wait_for(self.worker.wait(),3)
        self.assertEqual(self.worker.returncode,0)

    async def test_stdout_backpressure_does_not_leave_worker_or_input_thread(self):
        def exercise():
            process=subprocess.Popen([BINARY,'watch'],env=self.env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
            def flood():
                try:process.stdin.write(b'{"action":"refresh"}\n'*1000);process.stdin.close()
                except BrokenPipeError:pass
            writer=threading.Thread(target=flood,daemon=True);writer.start()
            try:
                process.wait(timeout=8)
                output=process.stdout.read();errors=process.stderr.read()
                self.assertEqual(errors,b'');self.assertNotIn(TOKEN,output.decode())
            finally:
                if process.poll() is None:process.kill();process.wait()
                writer.join(timeout=1)
                process.stdout.close();process.stderr.close()
        await asyncio.to_thread(exercise)

    async def test_eof_and_oversized_request_bounded(self):
        await self.start(write=False)
        result=await self.rpc({'action':'setup','url':'ftp://invalid','token':TOKEN});self.assertFalse(result['ok'])
        self.worker.stdin.close();await asyncio.wait_for(self.worker.wait(),3);self.assertEqual(self.worker.returncode,0)

if __name__=='__main__':unittest.main()
