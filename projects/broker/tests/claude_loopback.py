"""Real installed Claude, production broker, synthetic wallet and loopback API."""
import http.server
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import threading
import protocol

real = os.environ.get('SEELE_BROKER_CLAUDE') or shutil.which('claude')
assert real, 'Claude is required for the no-tools contract check'
requests = []
marker = 'HOST_INSTRUCTIONS_MUST_NOT_REACH_CLAUDE'
guard = {key:'http://127.0.0.1:9' for key in ['HTTP_PROXY','HTTPS_PROXY','ALL_PROXY','http_proxy','https_proxy','all_proxy']}
guard.update(NO_PROXY='127.0.0.1,localhost',no_proxy='127.0.0.1,localhost')
class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self, *args): pass
    def do_GET(self):
        self.send_response(404); self.end_headers()
    def do_POST(self):
        length = int(self.headers['Content-Length'])
        assert 0 <= length <= 2*1024*1024
        request = json.loads(self.rfile.read(length))
        if '/count_tokens' in self.path:
            self.send_response(200); self.end_headers(); self.wfile.write(b'{"input_tokens":2}'); return
        assert self.path.startswith('/v1/messages'), self.path
        assert self.headers.get('x-api-key')=='SYNTHETIC-BROKER-KEY'
        requests.append(request)
        message=dict(id='msg_fixture',type='message',role='assistant',content=[],model=request['model'],stop_reason=None,stop_sequence=None,usage=dict(input_tokens=2,output_tokens=1))
        self.send_response(200); self.send_header('Content-Type','text/event-stream'); self.end_headers()
        for event in [
            dict(type='message_start',message=message),
            dict(type='content_block_start',index=0,content_block=dict(type='text',text='')),
            dict(type='content_block_delta',index=0,delta=dict(type='text_delta',text='42')),
            dict(type='content_block_stop',index=0),
            dict(type='message_delta',delta=dict(stop_reason='end_turn',stop_sequence=None),usage=dict(output_tokens=1)),
            dict(type='message_stop'),
        ]:
            self.wfile.write(('event: '+event['type']+'\ndata: '+json.dumps(event)+'\n\n').encode())

with tempfile.TemporaryDirectory(prefix='seele-claude-loopback-') as temporary:
    root=Path(temporary)
    server=http.server.HTTPServer(('127.0.0.1',0),Handler)
    thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
    quota=root/'quota';wallet=root/'wallet';wrapper=root/'claude'
    quota.write_text('#!'+sys.executable+'\nprint(\'[{"provider":"codex","usage":{"primary":{"usedPercent":100}}}]\')\n')
    wallet.write_text('#!'+sys.executable+'\nimport sys\nassert sys.argv[1:]==["lookup","application","seele-codex","account","claude"]\nprint("SYNTHETIC-BROKER-KEY")\n')
    # Only this fixture wrapper redirects the API. Production routing never
    # accepts a model endpoint override or inherited authentication variable.
    wrapper.write_text('#!'+sys.executable+'\nimport os,sys\nos.environ.update('+repr(dict(guard,ANTHROPIC_BASE_URL=f'http://127.0.0.1:{server.server_port}'))+')\nos.execv('+repr(real)+',['+repr(real)+']+sys.argv[1:])\n')
    for binary in [quota,wallet,wrapper]: binary.chmod(0o700)
    try:
        with protocol.broker(environment_overrides=dict(SEELE_BROKER_CODEXBAR=str(quota),SEELE_BROKER_CLAUDE=str(wrapper),SEELE_BROKER_SECRET_TOOL=str(wallet))) as (runtime,path,environment,_):
            Path(environment['HOME'],'CLAUDE.md').write_text(marker)
            result=subprocess.run([str(protocol.BINARY),'call'],input=json.dumps(protocol.payload()).encode(),env=environment,capture_output=True,timeout=45)
            reply=json.loads(result.stdout)
            assert reply.get('result')==42,reply
            assert reply['job']['model']=='claude:haiku',reply
            assert requests and all(not request.get('tools') for request in requests),'Claude exposed a tool'
            assert marker not in json.dumps(requests),'Claude inherited host instructions'
            protocol.eventually(lambda:list(runtime.iterdir())==[path])
    finally:
        server.shutdown();thread.join()
print('Real Claude loopback: empty tools, excluded host instructions, dedicated synthetic key, validated result and private cleanup passed')
