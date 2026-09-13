"""Production Rust screenshot workflow; fake desktop/curl never uploads anything."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

BINARY=Path(sys.argv[1]).resolve() if len(sys.argv)>1 else Path(__file__).resolve().parents[3]/'target/debug/seele-screenshot'
PNG=b'\x89PNG\r\n\x1a\nfake-png:10,20 300x200'


def main():
    with tempfile.TemporaryDirectory(prefix='seele-screenshot-fixture-') as temporary:
        root=Path(temporary);root.chmod(0o700);tools=root/'bin';tools.mkdir(mode=0o700)
        script=tools/'command'
        script.write_text(('#!' + sys.executable + '\n') + '''import json,os,pathlib,sys,time
name=pathlib.Path(sys.argv[0]).name
args=sys.argv[1:]
state=pathlib.Path(os.environ['TEST_STATE'])
with (state/'calls.jsonl').open('a') as log:log.write(json.dumps([name,args])+"\\n")
image=b'\\x89PNG\\r\\n\\x1a\\nfake-png:10,20 300x200'
if name=='hyprctl':
 if args[0]=='monitors':print('[{"x":0,"y":0,"width":1920,"height":1080,"scale":1,"transform":0,"activeWorkspace":{"id":1},"specialWorkspace":{"id":0}}]')
 else:print('[{"at":[10,20],"size":[300,200],"workspace":{"id":1},"mapped":true,"hidden":false,"pinned":false}]')
elif name=='hyprpicker':
 (state/'freeze-pid').write_text(str(os.getpid()))
 if os.environ.get('TEST_FREEZE')!='fail':time.sleep(3600)
elif name=='slurp':
 hints=sys.stdin.read();assert '0,0 1920x1080' in hints and '10,20 300x200' in hints
 if os.environ.get('TEST_SLURP')=='cancel':sys.exit(1)
 print(os.environ.get('TEST_SELECTION','10,20 300x200'))
elif name=='grim':
 assert args[:2]==['-g','10,20 300x200'],args
 pathlib.Path(args[-1]).write_bytes(image)
elif name=='satty':
 if os.environ.get('TEST_SATTY')!='cancel':pathlib.Path(args[args.index('--output-filename')+1]).write_bytes(pathlib.Path(args[args.index('--filename')+1]).read_bytes())
elif name=='wl-copy':
 (state/'clipboard').write_bytes(sys.stdin.buffer.read())
 if os.environ.get('TEST_CLIPBOARD_OWNER')=='1':
  pid=os.fork()
  if pid:
   (state/'clipboard-owner').write_text(str(pid))
   sys.exit(0)
  time.sleep(3600)
elif name=='zenity':
 if os.environ.get('TEST_SWAP')=='1':
  for path in (pathlib.Path(os.environ['HOME'])/'Pictures/Screenshots').glob('*.png'):path.write_bytes(b'private unrelated file')
 sys.exit(0 if os.environ.get('TEST_CHOICE')=='upload' else 1)
elif name=='curl':
 assert args[0]=='-q' and args[-1]=='https://0x0.st'
 assert args[args.index('--proto')+1]=='=https'
 assert 'secret=' in args and 'expires=24' in args
 form=args[args.index('--form')+1]
 assert form.startswith('file=@/proc/self/fd/')
 captured=pathlib.Path(form.split(';',1)[0][6:]).read_bytes()
 assert captured==image,captured
 (state/'uploaded').write_bytes(captured)
 mode=os.environ.get('TEST_CURL','success')
 if mode=='fail':sys.exit(22)
 print('https://0x0.st/test-image.png' if mode=='success' else '<html>not a link</html>')
elif name=='notify-send':pass
else:raise AssertionError(name)
''');script.chmod(0o700)
        for name in ('hyprctl','hyprpicker','slurp','grim','satty','wl-copy','zenity','curl','notify-send'):(tools/name).symlink_to(script)
        def case(name,mode='capture',**values):
            directory=root/name;directory.mkdir(mode=0o700);home=directory/'home';home.mkdir(mode=0o700);runtime=directory/'runtime';runtime.mkdir(mode=0o700);state=directory/'state';state.mkdir(mode=0o700)
            environment=dict(os.environ,HOME=str(home),XDG_RUNTIME_DIR=str(runtime),TEST_STATE=str(state),PATH=str(tools)+os.pathsep+os.environ['PATH'],**values)
            result=subprocess.run([str(BINARY),mode],env=environment,capture_output=True,timeout=10)
            assert not result.stdout
            calls=[json.loads(line)for line in(state/'calls.jsonl').read_text().splitlines()]
            assert not list(runtime.iterdir()),list(runtime.iterdir())
            freeze=state/'freeze-pid'
            if freeze.exists():
                try:os.kill(int(freeze.read_text()),0)
                except ProcessLookupError:pass
                else:raise AssertionError('freeze child survived')
            images=list((home/'Pictures/Screenshots').glob('*.png'))
            return result,state,images,calls,environment
        result,state,images,calls,env=case('capture');assert result.returncode==0,result.stderr;assert len(images)==1 and images[0].read_bytes()==PNG;assert images[0].stat().st_mode&0o777==0o600;assert(state/'clipboard').read_bytes()==PNG;assert not any(row[0]in('curl','zenity')for row in calls)
        second=subprocess.run([str(BINARY),'capture'],env=env,capture_output=True,timeout=10);assert second.returncode==0;assert len(list(images[0].parent.glob('*.png')))==2
        result,state,images,calls,_=case('click',TEST_SELECTION='11,21 1x1');assert result.returncode==0,result.stderr;assert images[0].read_bytes()==PNG
        result,state,images,calls,_=case('annotate','annotate');assert result.returncode==0,result.stderr;assert images[0].read_bytes()==PNG;args=next(args for name,args in calls if name=='satty');assert '--actions-on-enter' in args and 'save-to-file' in args and 'exit'in args
        result,state,images,calls,_=case('annotate-cancel','annotate',TEST_SATTY='cancel');assert result.returncode==0 and not images and not(state/'clipboard').exists()
        result,state,images,calls,_=case('upload','upload',TEST_CHOICE='upload');assert result.returncode==0,result.stderr;assert(state/'clipboard').read_text()=='https://0x0.st/test-image.png';dialog=next(args for name,args in calls if name=='zenity');assert '--ok-label=Upload'in dialog and '--cancel-label=Copy image'in dialog;assert any('public third-party host' in arg and '24 hours'in arg for arg in dialog)
        for variant in ('fail','invalid'):
            result,state,images,calls,_=case('upload-'+variant,'upload',TEST_CHOICE='upload',TEST_CURL=variant);assert result.returncode==0,result.stderr;assert(state/'clipboard').read_bytes()==PNG;assert any(name=='notify-send'and'Screenshot upload failed'in args for name,args in calls)
        result,state,images,calls,_=case('decline','upload');assert result.returncode==0 and(state/'clipboard').read_bytes()==PNG;assert not any(name=='curl'for name,_ in calls)
        result,state,images,calls,_=case('swap','upload',TEST_CHOICE='upload',TEST_SWAP='1');assert result.returncode==0,result.stderr;assert(state/'uploaded').read_bytes()==PNG and images[0].read_bytes()!=PNG
        for mode in ('capture','upload'):
            result,state,images,calls,_=case('owner-'+mode,mode,TEST_CLIPBOARD_OWNER='1',TEST_CHOICE='upload')
            assert result.returncode==0,result.stderr
            owner=int((state/'clipboard-owner').read_text())
            try:
                os.kill(owner,0)
                assert '\nState:\tZ' not in Path('/proc',str(owner),'status').read_text(),'clipboard owner was killed'
            finally:os.kill(owner,9)
        result,state,images,calls,_=case('cancel',TEST_SLURP='cancel');assert result.returncode==0 and not images
        result,state,images,calls,_=case('freeze-failure',TEST_FREEZE='fail');assert result.returncode!=0 and not images and not any(name=='grim'for name,_ in calls)
        result,state,images,calls,_=case('invalid-geometry',TEST_SELECTION='-9223372036854775808,0 1x1');assert result.returncode!=0 and not images
    print('Rust screenshot: frozen geometry, exclusive images, annotation cancellation, exact consent, FD-only upload, swapped-path safety, copy fallback and cleanup passed')


if __name__=='__main__':main()
