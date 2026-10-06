"""Production shelf, authenticated CLI IPC and Notes copying with fixture data."""
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import sys
import tempfile

binary=Path(sys.argv[1]).resolve()
shellctl=binary.with_name('seele-shellctl')
notes=binary.with_name('seele-notes-store')
with tempfile.TemporaryDirectory(prefix='seele-shelf-fixture-') as temporary:
    root=Path(temporary);root.chmod(0o700)
    runtime=root/'runtime';runtime.mkdir(mode=0o700)
    source=root/'original file.png';source.write_bytes(b'fixture-original')
    launcher=root/'quickshell';calls=root/'calls.json'
    launcher.write_text('#!'+sys.executable+'\nimport json,sys\nopen('+repr(str(calls))+',"w").write(json.dumps(sys.argv[1:]))\n')
    launcher.chmod(0o700)
    env=dict(os.environ,XDG_RUNTIME_DIR=str(runtime),SEELE_SHELL_PATH='fixture-shell',PATH=str(root)+':'+os.environ['PATH'])
    def launch():
        process=subprocess.Popen([str(binary)],env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
        state=json.loads(process.stdout.readline());assert state['items']==[]
        return process
    process=launch()
    def request(value):
        process.stdin.write(json.dumps(value).encode()+b'\n');process.stdin.flush()
        return json.loads(process.stdout.readline())
    state=request(dict(op='files',paths=[str(source),str(source)]));assert len(state['items'])==1
    assert state['items'][0]['uri'].endswith('original%20file.png')
    assert request(dict(op='files',paths=[str(source),str(root/'missing')]))['error']
    assert len(request(dict(op='status'))['items'])==1
    text='PRIVATE-SNIPPET-MUST-NOT-ENTER-ARGV\nsecond line'
    result=subprocess.run([str(shellctl),'shelf','--text'],input=text.encode(),env=env,capture_output=True,timeout=10)
    assert result.returncode==0,result.stderr
    args=json.loads(calls.read_text());assert args[-1]=='openShelf' and text not in json.dumps(args),args
    state=json.loads(process.stdout.readline())
    snippet=Path(state['items'][-1]['path'])
    assert snippet.read_text()==text and snippet.stat().st_mode & 0o777 == 0o600
    assert Path(state['items'][-1]['path']).parent.stat().st_mode & 0o777 == 0o700
    assert (runtime/'seele-shelf.sock').stat().st_mode & 0o777 == 0o600
    # Explicit Notes capture makes durable independent attachments and a note.
    vault=root/'vault';vault.mkdir(mode=0o700)
    config=root/'config'/'seele-notes';config.mkdir(parents=True,mode=0o700)
    (config/'settings.json').write_text(json.dumps(dict(vault=str(vault),directory='Inbox',attachments='Attachments')))
    (config/'settings.json').chmod(0o600)
    note_env=dict(env,XDG_CONFIG_HOME=str(root/'config'),XDG_STATE_HOME=str(root/'state'))
    result=subprocess.run([str(notes),'capture-files',str(source),str(snippet)],env=note_env,capture_output=True,timeout=10)
    reply=json.loads(result.stdout);assert reply['ok'],(reply,result.stderr)
    note=(vault/reply['path']).read_text()
    assert note.startswith('# Shelf capture') and '![[Shelf ' in note
    copied=list((vault/'Inbox'/'Attachments').iterdir())
    assert len(copied)==2 and sorted(p.read_bytes() for p in copied)==sorted([source.read_bytes(),text.encode()])
    assert source.read_bytes()==b'fixture-original'
    # A failed mixed batch produces no additional attachment or note.
    result=subprocess.run([str(notes),'capture-files',str(source),str(root/'missing')],env=note_env,capture_output=True,timeout=10)
    assert not json.loads(result.stdout)['ok'] and len(list((vault/'Inbox'/'Attachments').iterdir()))==2
    state=request(dict(op='clear'));assert state['items']==[] and not snippet.exists() and source.exists()
    request(dict(op='text',text='temporary again'))
    process.stdin.close();process.wait(timeout=5)
    assert process.returncode==0 and not [p for p in runtime.iterdir() if p.name != "seele-shelf.lock"] and not process.stderr.read()
    # Idle termination cleans text and the socket too.
    process=launch();state=request(dict(op='text',text='signal cleanup'))
    process.send_signal(signal.SIGTERM);process.wait(timeout=5)
    assert process.returncode==0 and not [p for p in runtime.iterdir() if p.name != "seele-shelf.lock"] and source.exists()
    # A competing worker cannot remove an active endpoint; SIGKILL leaves a
    # stale endpoint that the next lock owner can retire safely.
    process=launch()
    other=subprocess.run([str(binary)],env=env,input=b'',capture_output=True,timeout=5)
    assert other.returncode!=0 and (runtime/'seele-shelf.sock').exists()
    process.kill();process.wait(timeout=5)
    assert (runtime/'seele-shelf.sock').exists()
    process=launch();process.stdin.close();process.wait(timeout=5)
    assert process.returncode==0 and not (runtime/'seele-shelf.sock').exists()
    unsafe=runtime/'seele-shelf.sock';unsafe.write_text('preserve')
    other=subprocess.run([str(binary)],env=env,input=b'',capture_output=True,timeout=5)
    assert other.returncode!=0 and unsafe.read_text()=='preserve';unsafe.unlink()
print('Shelf: atomic references, private snippets/socket, no text argv, independent Notes copies, EOF and SIGTERM cleanup passed')
