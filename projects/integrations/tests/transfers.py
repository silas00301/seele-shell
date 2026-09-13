#!/usr/bin/env python3
"""Native service against a real private Unix HTTP LocalAPI fixture, no accounts."""
import contextlib
import http.server
import json
import os
from pathlib import Path
import socket
import selectors
import socketserver
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from urllib.parse import unquote

BINARY=str(Path(sys.argv.pop(1)).resolve())

class UnixServer(socketserver.ThreadingUnixStreamServer):
    daemon_threads=True
    def handle_error(self,*args):pass

class Transfers(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.root=Path(self.temp.name)
        self.downloads=self.root/'downloads';self.downloads.mkdir(mode=0o700)
        self.runtime=self.root/'runtime';self.runtime.mkdir(mode=0o700)
        self.state=self.root/'state';self.state.mkdir(mode=0o700)
        self.bin=self.root/'bin';self.bin.mkdir()
        self.actions=self.root/'actions';self.notifications=self.root/'notifications'
        for command in ['seele-shellctl','notify-send','gio','xdg-open']:
            script=self.bin/command
            script.write_text(f"#!{sys.executable}\nimport json,os,sys,time\nfrom pathlib import Path\np=Path(os.environ['FIXTURE_ACTIONS'])\nwith p.open('a') as f:f.write(json.dumps({{'args':sys.argv,'pid':os.getpid()}})+'\\n')\nif Path(sys.argv[0]).name=='notify-send':time.sleep(60)\n")
            script.chmod(0o700)
        self.env={**os.environ,'XDG_RUNTIME_DIR':str(self.runtime),'XDG_STATE_HOME':str(self.state),'SEELE_TRANSFERS_DOWNLOADS':str(self.downloads),'SEELE_TAILSCALE_SOCKET':str(self.root/'daemon.sock'),'FIXTURE_ACTIONS':str(self.actions),'PATH':str(self.bin)+os.pathsep+os.environ['PATH']}
        self.available=True;self.pending={};self.deleted=[];self.sent=[];self.failures=0;self.fail_ack=0;self.send_hold=threading.Event();self.send_hold.set();self.receive_hold=threading.Event();self.receive_hold.set();self.send_started=threading.Event();self.receiving=threading.Event();self.stop=threading.Event();self.events=[]
        fixture=self
        class Handler(http.server.BaseHTTPRequestHandler):
            def log_message(self,*args):pass
            def reply(self,value,status=200):
                body=value if isinstance(value,bytes) else json.dumps(value).encode();self.send_response(status);self.send_header('Content-Length',str(len(body)));self.end_headers()
                with contextlib.suppress(BrokenPipeError,ConnectionResetError):self.wfile.write(body)
            def do_GET(self):
                route=unquote(self.path)
                if route.endswith('/status'):self.reply({'Self':{'UserID':7}})
                elif route.endswith('/file-targets'):
                    self.reply([{'Node':{'StableID':'same','ComputedName':'Phone','User':7,'Online':fixture.available}},{'Node':{'StableID':'other','User':8,'Online':True}},{'Node':{'StableID':'offline','User':7,'Online':False}},{'Node':{'StableID':'unknown','User':7}}])
                elif '/watch-ipn-bus?' in route:
                    self.send_response(200);self.end_headers();offset=0
                    while not fixture.stop.wait(.02):
                        for event in fixture.events[offset:]:
                            try:self.wfile.write(json.dumps(event).encode()+b'\n');self.wfile.flush()
                            except (BrokenPipeError,ConnectionResetError):return
                            offset+=1
                elif route.endswith('/files/'):
                    self.reply([{'Name':n,'Size':len(payload)} for n,payload in list(fixture.pending.items())])
                elif '/files/' in route:
                    name=route.split('/files/',1)[1];payload=fixture.pending.get(name)
                    if payload is None:self.reply(b'',404);return
                    self.send_response(200);self.send_header('Content-Length',str(len(payload)));self.end_headers();fixture.receiving.set()
                    if not fixture.receive_hold.is_set():
                        self.wfile.write(payload[:10]);self.wfile.flush()
                        while not fixture.receive_hold.wait(.01):
                            if fixture.stop.is_set():return
                        payload=payload[10:]
                    with contextlib.suppress(BrokenPipeError,ConnectionResetError):self.wfile.write(payload)
                else:self.reply([],404)
            def do_PUT(self):
                payload=self.rfile.read(int(self.headers['Content-Length']));fixture.send_started.set()
                while not fixture.send_hold.wait(.01):
                    if fixture.stop.is_set():return
                if fixture.failures:fixture.failures-=1;self.reply(b'',500);return
                fixture.sent.append((self.path,payload));self.reply(b'')
            def do_DELETE(self):
                name=unquote(self.path.split('/files/',1)[1])
                if fixture.fail_ack:fixture.fail_ack-=1;self.reply(b'',500);return
                fixture.deleted.append(name);fixture.pending.pop(name,None);self.reply(b'',204)
        self.server=UnixServer(str(self.root/'daemon.sock'),Handler);self.thread=threading.Thread(target=self.server.serve_forever,daemon=True);self.thread.start();self.process=None
        self.start()

    def tearDown(self):
        self.stop.set();self.send_hold.set();self.receive_hold.set()
        if self.process:
            if self.process.poll() is None:self.process.terminate()
            stdout,stderr=self.process.communicate(timeout=6);self.assertEqual(stdout,b'');self.assertEqual(stderr,b'')
        self.server.shutdown();self.server.server_close();self.thread.join();self.temp.cleanup()

    def start(self):
        self.process=subprocess.Popen([BINARY,'serve'],env=self.env,stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
        self.until(lambda:(self.runtime/'seele-transfers.sock').exists())
        self.until(lambda:self.snapshot()['targets'])

    def until(self,predicate,timeout=8):
        deadline=time.monotonic()+timeout
        while time.monotonic()<deadline:
            if predicate():return
            if self.process and self.process.poll() is not None:raise AssertionError('service exited '+repr(self.process.communicate()))
            time.sleep(.01)
        raise AssertionError('fixture condition timed out: '+json.dumps(self.snapshot()))

    def request(self,value):
        with socket.socket(socket.AF_UNIX,socket.SOCK_STREAM) as conn:
            conn.settimeout(6);conn.connect(str(self.runtime/'seele-transfers.sock'));conn.sendall(json.dumps(value).encode()+b'\n')
            with conn.makefile('rb') as stream:return json.loads(stream.readline())
    def snapshot(self):return self.request({'op':'snapshot'})
    def group(self,id):return next(g for g in self.snapshot()['groups'] if g['id']==id)
    def file(self,name='a.txt',payload=b'original\x00bytes'):
        path=self.root/name;path.write_bytes(payload);return str(path)
    def send(self,paths):
        self.assertEqual(self.request({'op':'select','paths':paths}),{'ok':True});result=self.request({'op':'send','target':'same'});self.assertTrue(result.get('ok'),result);return result['id']
    def wait(self,id,state='completed',timeout=8):
        self.until(lambda:self.group(id)['state']==state,timeout);return self.group(id)

    def test_original_bytes_selection_consumed_filtering_and_private_history(self):
        self.assertEqual(self.snapshot()['targets'],[{'id':'same','name':'Phone'}])
        a=self.file();b=self.file('space # and\nnewline.txt',b'second');id=self.send([a,b,a]);self.assertEqual(self.request({'op':'send','target':'same'})['error'],'choose-files')
        group=self.wait(id);self.assertEqual([b for _,b in self.sent],[b'original\x00bytes',b'second']);self.assertEqual(len(group['files']),2);self.assertNotIn('path',group['files'][0]);self.assertIn('space%20%23%20and%0Anewline.txt',self.sent[-1][0])
        disk=self.state/'seele-transfers/history.json';self.assertEqual(disk.stat().st_mode&0o777,0o600);self.assertNotIn('original',disk.read_text());self.assertFalse(self.actions.exists())
        self.assertEqual((self.runtime/'seele-transfers.sock').stat().st_mode&0o777,0o600)

    def test_offline_directory_and_unknown_target_preserve_selection(self):
        path=self.file();self.request({'op':'select','paths':[path]});self.available=False
        self.assertEqual(self.request({'op':'send','target':'same'})['error'],'target-unavailable');self.assertEqual(self.snapshot()['selection'],['a.txt'])
        self.assertEqual(self.request({'op':'send','target':'other'})['error'],'target-unavailable')
        self.assertEqual(self.request({'op':'select','paths':[str(self.root)]})['error'],'not-a-file')

    def test_cancel_retry_identity_and_daemon_progress(self):
        self.send_hold.clear();path=self.file();id=self.send([path]);self.assertTrue(self.send_started.wait(2))
        self.events.append({'OutgoingFiles':[{'PeerID':'same','Name':'a.txt','Sent':5,'Finished':False}]});self.until(lambda:self.group(id)['bytes']==5)
        self.events.append({'OutgoingFiles':[{'PeerID':'same','Name':'a.txt','Sent':99,'Finished':True}]});time.sleep(.1);self.assertEqual(self.group(id)['bytes'],5)
        self.assertTrue(self.request({'op':'cancel','id':id})['ok']);self.wait(id,'cancelled');self.assertTrue(Path(path).exists())
        self.send_hold.set();self.assertTrue(self.request({'op':'retry','id':id})['ok']);self.wait(id)

    def test_retry_bound_and_missing_source(self):
        self.failures=10;path=self.file();id=self.send([path]);group=self.wait(id,'failed');self.assertEqual(group['files'][0]['attempts'],3)
        self.until(lambda:self.actions.exists());Path(path).unlink();self.request({'op':'retry','id':id});self.until(lambda:self.group(id)['error']=='source-missing')

    def test_receive_collision_ack_recovery_restart_and_repeated_name(self):
        source=self.file('safe.txt',b'unchanged');(self.downloads/'a.txt').write_bytes(b'existing');(self.downloads/'a (1).txt').symlink_to(source)
        self.fail_ack=1;self.pending['a.txt']=b'received\x00payload'*60000
        self.until(lambda:bool(self.snapshot()['groups']));id=self.snapshot()['groups'][0]['id'];self.wait(id)
        group=self.group(id);path=Path(group['files'][0]['path']);self.assertEqual(path.name,'a (2).txt');self.assertEqual(path.read_bytes(),b'received\x00payload'*60000);self.assertEqual(path.stat().st_mode&0o777,0o600)
        self.assertEqual(self.deleted,['a.txt']);self.assertFalse((self.downloads/'a (3).txt').exists());self.assertEqual(Path(source).read_bytes(),b'unchanged')
        self.pending['a.txt']=b'second';self.until(lambda:len(self.snapshot()['groups'])==2);second=self.snapshot()['groups'][0]['id'];self.assertNotEqual(second,id);self.wait(second)
        self.assertEqual((self.downloads/'a (3).txt').read_bytes(),b'second')
        self.request({'op':'dismiss','id':id});self.assertTrue(path.exists())
        self.process.terminate();self.process.communicate(timeout=6);self.start();self.assertEqual(self.group(second)['state'],'completed')

    def test_cancel_receipt_removes_private_temporary_not_user_file(self):
        (self.downloads/'a.txt').write_bytes(b'keep');self.receive_hold.clear();self.pending['a.txt']=b'x'*100000
        self.assertTrue(self.receiving.wait(4));self.until(lambda:any(self.downloads.glob('.seele-*.part')));id=self.snapshot()['groups'][0]['id']
        self.request({'op':'cancel','id':id});self.wait(id,'cancelled');self.until(lambda:not list(self.downloads.glob('.seele-*.part')))
        self.assertEqual((self.downloads/'a.txt').read_bytes(),b'keep');self.assertEqual(self.deleted,[])

    def test_move_collision_trash_focus_and_shutdown_owns_notifications(self):
        self.pending['a.txt']=b'received';self.until(lambda:bool(self.snapshot()['groups']));id=self.snapshot()['groups'][0]['id'];self.wait(id)
        target=self.root/'destination';target.mkdir(mode=0o700);(target/'a.txt').write_bytes(b'keep');old=Path(self.group(id)['files'][0]['path'])
        self.assertTrue(self.request({'op':'move','id':id,'file':0,'directory':str(target)})['ok']);self.assertFalse(old.exists());self.assertEqual(Path(self.group(id)['files'][0]['path']).name,'a (1).txt');self.assertEqual((target/'a.txt').read_bytes(),b'keep')
        self.request({'op':'focus','id':id});first=self.snapshot()['focusRevision'];self.request({'op':'focus','id':id});self.assertEqual(self.snapshot()['focusRevision'],first+1)
        self.assertTrue(self.request({'op':'trash','id':id,'file':0})['ok']);self.assertEqual(self.group(id)['files'][0]['state'],'trashed')
        self.until(lambda:self.actions.exists());actions=[json.loads(line) for line in self.actions.read_text().splitlines()];self.assertTrue(any(a['args'][1:3]==['trash','--'] for a in actions))
        notifications=[a['pid'] for a in actions if a['args'][0].endswith('notify-send')];self.assertTrue(notifications)
        self.process.terminate();self.process.communicate(timeout=6)
        for pid in notifications:
            with self.assertRaises(ProcessLookupError):os.kill(pid,0)
        self.assertFalse((self.runtime/'seele-transfers.sock').exists())

    def test_acknowledgement_recovery_after_service_restart_never_recopies(self):
        self.fail_ack=100;self.pending['a.txt']=b'received'
        self.until(lambda:bool(self.snapshot()['groups']));id=self.snapshot()['groups'][0]['id'];self.wait(id,'failed')
        path=Path(self.group(id)['files'][0]['path']);self.assertEqual(path.read_bytes(),b'received')
        disk=json.loads((self.state/'seele-transfers/history.json').read_text());self.assertTrue(disk[0]['files'][0]['pendingAck'])
        self.process.terminate();self.process.communicate(timeout=6);self.fail_ack=0;self.start();self.wait(id)
        self.assertEqual(self.deleted,['a.txt']);self.assertEqual(list(self.downloads.iterdir()),[path])

    def test_changed_same_size_source_is_never_retried(self):
        self.send_hold.clear();path=self.file();id=self.send([path]);self.assertTrue(self.send_started.wait(2))
        self.request({'op':'cancel','id':id});self.wait(id,'cancelled');Path(path).write_bytes(b'changed!'+b'x'*6)
        self.send_hold.set();time.sleep(.1);before=len(self.sent)
        self.request({'op':'retry','id':id});self.until(lambda:self.group(id)['error']=='source-changed')
        self.assertEqual(len(self.sent),before)

    def test_idle_history_and_watch_do_not_repeat_unchanged_work(self):
        history=self.state/'seele-transfers/history.json';before=history.stat().st_mtime_ns
        watcher=subprocess.Popen([BINARY,'watch'],env=self.env,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
        try:
            with selectors.DefaultSelector() as selector:
                selector.register(watcher.stdout,selectors.EVENT_READ)
                self.assertTrue(selector.select(2));line=watcher.stdout.readline();self.assertEqual(json.loads(line)['version'],1)
                self.assertFalse(selector.select(2.2),'unchanged snapshots must not rebind the UI')
            self.assertEqual(history.stat().st_mtime_ns,before,'idle polling must not rewrite history')
        finally:
            watcher.terminate();watcher.communicate(timeout=3)

    def test_seen_batch_is_validated_before_one_durable_update(self):
        self.pending['first.txt']=b'first';self.until(lambda:len(self.snapshot()['groups'])==1)
        first=self.snapshot()['groups'][0]['id'];self.wait(first)
        self.pending['second.txt']=b'second';self.until(lambda:len(self.snapshot()['groups'])==2)
        second=self.snapshot()['groups'][0]['id'];self.wait(second)
        history=self.state/'seele-transfers/history.json'
        before=history.stat().st_mtime_ns
        self.assertEqual(self.request({'op':'seen','ids':[first,'missing']})['error'],'transfer-missing')
        self.assertFalse(self.group(first)['seen']);self.assertEqual(history.stat().st_mtime_ns,before)
        for ids in [[],[True],['x'*129],['x']*4097]:
            self.assertEqual(self.request({'op':'seen','ids':ids})['error'],'invalid-request')
        self.assertEqual(self.request({'op':'seen','ids':[first,second,first]}),{'ok':True})
        self.assertTrue(all(group['seen'] for group in self.snapshot()['groups']))
        durable=json.loads(history.read_text());self.assertTrue(all(group['seen'] for group in durable))
        before=history.stat().st_mtime_ns
        self.assertEqual(self.request({'op':'seen','id':first}),{'ok':True})
        self.assertEqual(history.stat().st_mtime_ns,before,'already-seen requests do not rewrite history')

    def test_private_socket_rejects_oversize_stall_and_second_daemon(self):
        process=subprocess.run([BINARY,'serve'],env=self.env,capture_output=True,timeout=3);self.assertEqual(process.returncode,1);self.assertEqual(json.loads(process.stdout)['error'],'service-already-running')
        with socket.socket(socket.AF_UNIX,socket.SOCK_STREAM) as connection:
            connection.connect(str(self.runtime/'seele-transfers.sock'));connection.sendall(b'x'*(256*1024)+b'\n')
            response=connection.recv(1024);self.assertEqual(json.loads(response)['error'],'request-too-large')
        peers=[]
        try:
            for _ in range(10):peer=socket.socket(socket.AF_UNIX,socket.SOCK_STREAM);peer.connect(str(self.runtime/'seele-transfers.sock'));peer.sendall(b'{');peers.append(peer)
            self.assertEqual(self.snapshot()['version'],1)
        finally:
            for peer in peers:peer.close()

if __name__=='__main__':unittest.main()
