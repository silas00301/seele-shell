"""Native wizard cancellation and wallet arguments; never accesses a real account."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
binary=Path(sys.argv[1]).resolve()
with tempfile.TemporaryDirectory(prefix='seele-linear-fixture-') as temporary:
    root=Path(temporary);root.chmod(0o700);tools=root/'bin';tools.mkdir(mode=0o700)
    script=tools/'fake'
    script.write_text('#!'+sys.executable+'\n'+r'''
import json,os,pathlib,sys
name=pathlib.Path(sys.argv[0]).name;args=sys.argv[1:];state=pathlib.Path(os.environ['TEST_STATE'])
with (state/'calls').open('a') as f:f.write(json.dumps([name,args])+'\n')
if name=='zenity':
 if '--password' in args:print('synthetic-linear-api-key')
 elif '--forms' in args and os.environ.get('METADATA_CANCEL'):print('|Fixture title|Fixture description')
 elif '--checklist' in args:assert '--print-column=2' in args;sys.exit(1)
 else:sys.exit(1)
elif name=='secret-tool':
 assert args[-4:]==['application','seele-linear-capture','account','linear']
 assert not any('synthetic-linear-api-key' in arg for arg in args)
 if args[0]=='store':assert sys.stdin.buffer.read()==b'synthetic-linear-api-key';(state/'stored').touch()
 elif args[0]=='clear':(state/'cleared').touch()
 else:raise AssertionError('canceled draft must not request wallet credentials')
else:raise AssertionError(name)
''');script.chmod(0o700)
    for name in ('zenity','secret-tool'):(tools/name).symlink_to(script)
    def case(name,args,**values):
        state=root/name;state.mkdir(mode=0o700)
        env=dict(os.environ,TEST_STATE=str(state),HOME=str(state),PATH=str(tools)+os.pathsep+os.environ['PATH'],**values)
        result=subprocess.run([str(binary),*map(str,args)],env=env,capture_output=True,timeout=10)
        assert b'synthetic-linear-api-key' not in result.stdout+result.stderr
        calls=[json.loads(line) for line in (state/'calls').read_text().splitlines()] if (state/'calls').exists() else []
        return result,state,calls
    result,state,calls=case('connect',['connect']);assert result.returncode==0,result.stderr;assert (state/'stored').exists();assert any('--password' in args for _,args in calls)
    result,state,calls=case('disconnect',['disconnect']);assert result.returncode==0 and (state/'cleared').exists()
    image=root/'capture.png';image.write_bytes(b'\x89PNG\r\n\x1a\nfixture')
    result,state,calls=case('draft-cancel',[image]);assert result.returncode==0,result.stderr;assert len(calls)==1 and calls[0][0]=='zenity';assert image.read_bytes()==b'\x89PNG\r\n\x1a\nfixture'
    result,state,calls=case('metadata-cancel',[image],METADATA_CANCEL='1');assert result.returncode==0,result.stderr;assert len(calls)==2 and all(name=='zenity' for name,_ in calls)
    link=root/'symlink.png';link.symlink_to(image)
    result,state,calls=case('symlink',[link]);assert result.returncode==1 and not calls
    result,state,calls=case('picker-cancel',[]);assert result.returncode==0 and len(calls)==1
print('Linear draft/wallet fixture passed')
