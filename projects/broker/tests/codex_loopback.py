"""Real packaged Codex through the production broker, against loopback only.

Exit 77 means the installed executable lacks the mandatory isolation flags.
Package checks must use a supported Codex and treat that boundary as a failure.
"""
import http.server
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import threading
import tempfile
import protocol


def main():
    real = os.environ.get('SEELE_BROKER_CODEX') or shutil.which('codex')
    assert real, 'Codex is required for the no-tools contract check'
    guard = {'HTTP_PROXY':'http://127.0.0.1:9', 'HTTPS_PROXY':'http://127.0.0.1:9',
             'ALL_PROXY':'http://127.0.0.1:9', 'NO_PROXY':'127.0.0.1,localhost',
             'http_proxy':'http://127.0.0.1:9', 'https_proxy':'http://127.0.0.1:9',
             'all_proxy':'http://127.0.0.1:9', 'no_proxy':'127.0.0.1,localhost'}
    with tempfile.TemporaryDirectory(prefix='seele-codex-help-') as home:
        Path(home).chmod(0o700)
        environment = dict(guard, HOME=home, CODEX_HOME=home, XDG_RUNTIME_DIR=home,
                           PATH=os.environ.get('PATH',''), RUST_LOG='off')
        help_result = subprocess.run([real, 'exec', '--ignore-user-config', '--ignore-rules', '--help'],
                                     env=environment, cwd=home, capture_output=True, timeout=10)
    if help_result.returncode:
        print('Installed Codex lacks mandatory --ignore-user-config/--ignore-rules isolation flags', file=sys.stderr)
        return 77
    requests = []
    marker = 'PRIVATE_GLOBAL_INSTRUCTIONS_MUST_NOT_REACH_INFERENCE'
    class Handler(http.server.BaseHTTPRequestHandler):
        def log_message(self, *args): pass
        def do_GET(self):
            self.send_response(200); self.end_headers(); self.wfile.write(b'{"models":[]}')
        def do_POST(self):
            length = int(self.headers['Content-Length'])
            assert 0 <= length <= 2*1024*1024
            request = json.loads(self.rfile.read(length)); requests.append(request)
            self.send_response(200); self.send_header('Content-Type', 'text/event-stream'); self.end_headers()
            for event in [
                {'type':'response.created','response':{'id':'fixture'}},
                {'type':'response.output_item.done','item':{'type':'message','role':'assistant','content':[{'type':'output_text','text':'42'}]}},
                {'type':'response.completed','response':{'id':'fixture','status':'completed','output':[], 'usage':{'input_tokens':2,'output_tokens':1,'total_tokens':3}}},
            ]:
                self.wfile.write(('data: '+json.dumps(event)+'\n\n').encode())
    server = http.server.HTTPServer(('127.0.0.1',0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True); thread.start()
    overrides = ['-c','model_provider="fixture"','-c','model_providers.fixture.name="fixture"',
                 '-c',f'model_providers.fixture.base_url="http://127.0.0.1:{server.server_port}"',
                 '-c','model_providers.fixture.wire_api="responses"','-c','model_providers.fixture.requires_openai_auth=false']
    wrapper = (('#!' + sys.executable + '\n') + 'import os,sys\nargs=sys.argv[1:]\n'
               'os.environ.update('+repr(guard)+')\n'
               'if "exec" in args: args=args[:-1]+'+repr(overrides)+'+[args[-1]]\n'
               'os.execv('+repr(real)+',['+repr(real)+']+args)\n')
    try:
        with protocol.broker(fake=wrapper) as (runtime,path,environment,process):
            Path(environment['CODEX_HOME'], 'AGENTS.md').write_text(marker)
            result = subprocess.run([str(protocol.BINARY),'call'], input=json.dumps(protocol.payload()).encode(), env=environment, capture_output=True, timeout=40)
            reply = json.loads(result.stdout)
            assert reply.get('result') == 42, reply
            assert reply['job']['tokens'] == {'input':2,'output':1}
            assert requests and all(not request.get('tools') for request in requests), 'Codex exposed a tool'
            assert marker not in json.dumps(requests), 'Codex inherited global instructions'
            protocol.eventually(lambda:list(runtime.iterdir())==[path])
        print('Real Codex: empty tool set, excluded global instructions, structured result, usage and runtime cleanup passed')
    finally:
        server.shutdown(); server.server_close(); thread.join()
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
