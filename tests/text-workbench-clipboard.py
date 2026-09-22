#!/usr/bin/env python3
"""Real bounded native clipboard process against private fake Wayland peers."""
import json, os, pathlib, subprocess, sys, tempfile
helper = str(pathlib.Path(sys.argv[1]).resolve())
with tempfile.TemporaryDirectory(prefix='seele-text-clipboard-') as directory:
    root = pathlib.Path(directory)
    for command, code in {
        'wl-paste': 'import os,sys\nsys.stdout.buffer.write(bytes.fromhex(os.environ["FIXTURE_TEXT"]))\n',
        'wl-copy': 'import os,sys,pathlib\nassert sys.argv[1:] == ["--type", "text/plain;charset=utf-8"]\npathlib.Path(os.environ["FIXTURE_COPY"]).write_bytes(sys.stdin.buffer.read())\n',
    }.items():
        path=root/command
        path.write_text(f'#!{sys.executable}\n'+code)
        path.chmod(0o700)
    env=dict(os.environ,PATH=f'{root}:'+os.environ['PATH'],FIXTURE_COPY=str(root/'copied'))
    def run(mode, data=b''):
        return json.loads(subprocess.run([helper,mode],input=data,capture_output=True,env=env,check=True,timeout=6).stdout)
    for payload in [b'', 'Grüße 🦀\n'.encode(), b'x'*65536]:
        env['FIXTURE_TEXT']=payload.hex()
        # Linux has a per-variable size limit; the exact boundary uses a file.
        if len(env['FIXTURE_TEXT']) > 100000:
            env.pop('FIXTURE_TEXT')
            (root/'paste').write_bytes(payload)
            (root/'wl-paste').write_text(f'#!{sys.executable}\nimport pathlib,sys\nsys.stdout.buffer.write(pathlib.Path({str(root/"paste")!r}).read_bytes())\n')
        reply=run('paste'); assert reply == {'ok':True,'text':payload.decode()},reply
    # Use a file for binary and large clipboard fixtures.
    env.pop('FIXTURE_TEXT',None)
    for payload in [b'\xff',b'a\0b',b'x'*65537]:
        (root/'paste').write_bytes(payload)
        reply=run('paste'); assert not reply['ok'] and 'text' not in reply,reply
    for payload in ['first\n'.encode(),'second 🦀'.encode(), b'x'*262144]:
        reply=run('copy',payload); assert reply['ok'],reply
        assert (root/'copied').read_bytes()==payload
    previous=(root/'copied').read_bytes()
    for payload in [b'\xff',b'\0',b'x'*262145]:
        reply=run('copy',payload); assert not reply['ok'],reply
        assert (root/'copied').read_bytes()==previous
print('text workbench clipboard: exact UTF-8, repeated copies, binary rejection and limits passed')
