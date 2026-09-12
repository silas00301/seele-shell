"""Production Rust executables with isolated fake Codex; no accounts/model I/O."""
import contextlib
import json
import os
from pathlib import Path
import shutil
import signal
import socket
import stat
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[3]
BINARY = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else ROOT / 'target/debug/seele-codex'
HEALTH = BINARY.with_name('seele-codex-health')


def payload(consumer='fixture', **changes):
    value = dict(consumer=consumer, label='Fixture job', prompt='Private prompt', context={'value': 1},
                 input={'version': '1', 'schema': {'type': 'object', 'required': ['value']}},
                 output={'version': '1', 'schema': {'type': 'integer'}})
    value.update(changes)
    return value


def eventually(check, timeout=5):
    deadline = time.monotonic() + timeout
    while not check():
        assert time.monotonic() < deadline, 'condition timed out'
        time.sleep(.01)


@contextlib.contextmanager
def broker(binary=BINARY, fake=None):
    with tempfile.TemporaryDirectory(prefix='seele-broker-fixture-') as temporary:
        root = Path(temporary)
        root.chmod(0o700)
        runtime = root / 'runtime'
        runtime.mkdir(mode=0o700)
        home = root / 'home'
        home.mkdir(mode=0o700)
        (home / "auth.json").write_text(json.dumps({"OPENAI_API_KEY":"synthetic-test-only"}))
        (home / "auth.json").chmod(0o600)
        codex = root / 'codex'
        codex.write_text(fake or ('#!' + sys.executable + '\n') + '''import fcntl,json,os,sys,time
from pathlib import Path
args=sys.argv[1:]
if 'features' in args:
 print('shell_tool stable true\\nskip_host_skill_discovery experimental false\\nguardianv2.thread_context experimental false')
 sys.exit(0)
if 'login' in args:
 print('PRIVATE AUTHENTICATION OUTPUT')
 sys.exit(0)
assert 'exec' in args and '--ignore-user-config' in args and '--ignore-rules' in args and '--ephemeral' in args
assert args[args.index('--sandbox')+1]=='read-only'
assert args[args.index('--disable')+1]=='shell_tool'
assert 'tools.view_image=false' in args and 'web_search="disabled"' in args
assert os.environ.get('PRIVATE_INTEGRATION_TOKEN') is None
assert Path(os.environ['CODEX_HOME'], 'auth.json').is_symlink()
assert not Path(os.environ['CODEX_HOME'], 'AGENTS.md').exists()
assert Path(os.environ['HOME']).is_dir()
assert Path.cwd().stat().st_mode & 0o777 == 0o700
assert not list(Path.cwd().iterdir())
schema_path=args[args.index('--output-schema')+1]
fd=int(schema_path.rsplit('/',1)[1])
assert fcntl.fcntl(fd,fcntl.F_GET_SEALS) & fcntl.F_SEAL_WRITE
assert json.loads(Path(schema_path).read_text()) == {'type':'integer'}
request=json.load(sys.stdin)
assert request['context']=={'value':1}
if request['task']=='slow': time.sleep(30)
print(json.dumps({'type':'item.completed','item':{'type':'agent_message','text':'42'}}),flush=True)
print(json.dumps({'type':'turn.completed','usage':{'input_tokens':2,'output_tokens':1}}),flush=True)
''')
        codex.chmod(0o700)
        path = runtime / 'seele-codex.sock'
        with socket.socket(socket.AF_UNIX) as listener:
            listener.bind(str(path))
            path.chmod(0o600)
            listener.listen()
            environment = dict(os.environ, XDG_RUNTIME_DIR=str(runtime), HOME=str(home), CODEX_HOME=str(home),
                               SEELE_BROKER_CODEX=str(codex), PRIVATE_INTEGRATION_TOKEN='must-never-be-inherited')
            launch = 'import os,sys;os.dup2(int(sys.argv[1]),3);os.set_inheritable(3,True);os.environ.update(LISTEN_PID=str(os.getpid()),LISTEN_FDS="1");os.execv(sys.argv[2],sys.argv[2:])'
            process = subprocess.Popen([sys.executable, '-c', launch, str(listener.fileno()), str(binary), 'serve', '--idle', '30'],
                                       env=environment, pass_fds=(listener.fileno(),), stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            try:
                yield runtime, path, environment, process
            finally:
                process.send_signal(signal.SIGTERM)
                stdout, stderr = process.communicate(timeout=10)
                assert process.returncode == 0, (process.returncode, stdout, stderr)
                assert not stdout and not stderr, (stdout, stderr)
                assert list(runtime.iterdir()) == [path], 'private attempt artifacts remained'


def rpc(path, message):
    with socket.socket(socket.AF_UNIX) as client:
        client.settimeout(10)
        client.connect(str(path))
        client.sendall(json.dumps(message).encode() + b'\n')
        with client.makefile('rb') as stream:
            return json.loads(stream.readline(512*1024+1))


def main():
    with broker() as (runtime, path, environment, process):
        configuration = rpc(path, {'op':'configuration'})
        assert set(configuration)=={'ok','epoch','model'} and configuration['ok'] and configuration['model']=='gpt-5.6-luna', configuration
        assert rpc(path, {'op':'configuration','model':'override'})['error']=='invalid_input'
        assert rpc(path, {'op':'list'})['jobs']==[]
        a = rpc(path, {'op': 'submit', 'request': payload('client-a')})
        b = rpc(path, {'op': 'submit', 'request': payload('client-b')})
        for reply in (a, b):
            assert reply['ok'], reply
            identity = dict(id=reply['job']['id'], epoch=reply['epoch'])
            result = rpc(path, dict(op='wait', **identity))
            assert result.get('result') == 42, result
            assert result['job']['tokens'] == {'input': 2, 'output': 1}, result
            assert rpc(path, dict(op='status', **dict(identity, epoch='old')))['error'] == 'broker_restarted'
            assert rpc(path, dict(op='release', **identity))['ok']
        listing = rpc(path, {'op': 'list'})
        assert not any(key in json.dumps(listing) for key in ('Private prompt', 'context', 'result'))
        eventually(lambda: list(runtime.iterdir()) == [path])
        result = subprocess.run([str(BINARY), 'call'], input=json.dumps(payload()).encode(), env=environment, capture_output=True, timeout=10)
        assert json.loads(result.stdout)['result'] == 42, result.stdout
        invalid = subprocess.run([str(BINARY), 'request'], input=b'x'*(256*1024+1), env=environment, capture_output=True, timeout=10)
        assert json.loads(invalid.stdout)['error'] == 'invalid_input'
        bad = rpc(path, {'op':'submit','request':payload(output={'version':'1','schema':{'$ref':'file:///private'}})})
        assert bad['error'] == 'invalid_input'
        with socket.socket(socket.AF_UNIX) as client:
            client.settimeout(3); client.connect(str(path)); client.sendall(b' '*(256*1024+1))
            assert json.loads(client.recv(4096))['error'] == 'invalid_input'
        slow = rpc(path, {'op':'submit','request':payload(prompt='slow')})
        identity = dict(id=slow['job']['id'], epoch=slow['epoch'])
        eventually(lambda: rpc(path,dict(op='status',**identity))['job']['state']=='running')
        assert rpc(path,dict(op='cancel',**identity))['job']['state']=='cancelled'
        eventually(lambda: list(runtime.iterdir()) == [path])
        # Health uses the same framed RPC; authentication stdout/stderr go to
        # /dev/null and only the five fixed health fields enter shell IPC stdin.
        bindir=runtime.parent/'bin'; bindir.mkdir(mode=0o700)
        published=runtime.parent/'published.json'
        shellctl=bindir/'seele-shellctl'
        shellctl.write_text(('#!' + sys.executable + '\n') + 'import pathlib,sys\nassert sys.argv[1:]==["health-publish","codex"]\npathlib.Path('+repr(str(published))+').write_bytes(sys.stdin.buffer.read())\n')
        shellctl.chmod(0o700)
        health=subprocess.run([str(HEALTH)],env=dict(environment,PATH=str(bindir)+os.pathsep+environment['PATH']),capture_output=True,timeout=15)
        assert health.returncode==0 and not health.stdout and not health.stderr
        state=json.loads(published.read_text());assert state['state']=='healthy' and state['lastSuccess']>0
        assert 'PRIVATE' not in published.read_text()
        # File-auth delegation is required; keyring-only or missing credentials
        # fail before discovery/model invocation and do not retry automatically.
        Path(environment['CODEX_HOME'], 'auth.json').unlink()
        missing = rpc(path, {'op':'submit','request':payload('missing-auth')})
        failed = rpc(path, {'op':'wait','id':missing['job']['id'],'epoch':missing['epoch']})
        assert failed['job']['state']=='failed' and failed['job']['error']=='authentication_unavailable', failed
        assert failed['job']['attempts']==1, failed

    print('Rust broker: real socket activation, independent clients, no-tool argv, sealed memfd, environment, output schema, cleanup, cancellation, limits and health passed')


if __name__ == '__main__':
    main()
