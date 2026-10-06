"""Production workflow under a test-only helper seam; installed binary rejects spoofed escalation."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
binary=Path(sys.argv[1]).resolve()
with tempfile.TemporaryDirectory(prefix='seele-restore-ui-fixture-') as temporary:
    root=Path(temporary);root.chmod(0o700);tools=root/'bin';tools.mkdir(mode=0o700)
    runtime=root/'runtime';runtime.mkdir(mode=0o700)
    source=root/'current-for-comparison';source.write_bytes(b'current bytes\n')
    destination=root/'restored-copy';snapshot='a'*64
    script=tools/'fake'
    script.write_text('#!'+sys.executable+'\n'+r'''
import json,os,pathlib,sys
name=pathlib.Path(sys.argv[0]).name;args=sys.argv[1:];state=pathlib.Path(os.environ['STATE'])
with (state/'calls').open('a') as f:f.write(json.dumps([name,args])+'\n')
if name=='run0':
 assert args[:3]==['--pipe','--property=RuntimeMaxSec=180','--'];assert args[3]=='/fixture/root-helper'
 if args[4]=='versions':print(json.dumps([{'snapshot':'a'*64,'modified':'2026-10-06T00:00:00Z','size':13}]))
 elif args[4]=='dump':assert args[5]=='a'*64;sys.stdout.buffer.write(b'backup bytes\n')
 else:raise AssertionError(args)
elif name=='zenity':
 title=next(arg for arg in args if arg.startswith('--title='))
 if title=='--title=Choose a backup version':print('0')
 elif title=='--title=Backup version':
  counter=state/'action';index=int(counter.read_text()) if counter.exists() else 0;counter.write_text(str(index+1))
  actions=['Preview','Compare with current','Restore a copy','Restore a copy']
  if index>=len(actions):sys.exit(1)
  print(actions[index])
 elif title=='--title=Save a restored copy':print(os.environ['DESTINATION'])
 elif title=='--title=Compare versions':
  text=sys.stdin.read();assert '\x1b' not in text and 'SHA-256' in text;assert 'current bytes' in text and 'backup bytes' in text
 elif title=='--title=Backup preview':pass
 elif title=='--title=Restore copy':
  text=sys.stdin.read()
  if (state/'copied').exists():assert 'already exists' in text
  else:assert 'separate restored copy' in text;(state/'copied').touch()
 else:raise AssertionError(title)
elif name=='shellctl':
 assert args[0]=='quicklook' and pathlib.Path(args[1]).read_bytes()==b'backup bytes\n'
 assert pathlib.Path(args[1]).stat().st_mode&0o777==0o600
 (state/'previewed').touch()
else:raise AssertionError(name)
''');script.chmod(0o700)
    for name in ('run0','zenity','shellctl'):(tools/name).symlink_to(script)
    env=dict(os.environ,STATE=str(root),DESTINATION=str(destination),XDG_RUNTIME_DIR=str(runtime),SEELE_BACKUP_FILES_HELPER='/fixture/root-helper',SEELE_SHELLCTL=str(tools/'shellctl'),PATH=str(tools)+os.pathsep+os.environ['PATH'])
    if '--test-binary' in sys.argv:
        env['SEELE_TEST_RESTORE_PATH']=str(source)
        result=subprocess.run([str(binary),'--exact','tests::ui_fixture_child','--nocapture'],env=env,capture_output=True,timeout=15)
    else:
        result=subprocess.run([str(binary),str(source)],env=env,capture_output=True,timeout=15)
        assert result.returncode!=0 and not destination.exists() and not (root/'calls').exists()
        print('restore UI: installed executable rejects untrusted helper and PATH run0 before escalation')
        sys.exit(0)
    assert result.returncode==0,result.stderr
    assert source.read_bytes()==b'current bytes\n' and destination.read_bytes()==b'backup bytes\n'
    assert destination.stat().st_mode&0o777==0o600 and (root/'previewed').exists()
    assert not list(runtime.iterdir()),list(runtime.iterdir())
print('restore UI: private preview, content comparison, exclusive copy, original preservation and cleanup passed')
