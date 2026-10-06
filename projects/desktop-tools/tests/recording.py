"""Production recorder, fake capture/audio/media programs; no capture or upload."""
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile

binary = Path(sys.argv[1]).resolve()
with tempfile.TemporaryDirectory(prefix='seele-record-fixture-') as temporary:
    root = Path(temporary); root.chmod(0o700)
    tools = root/'bin'; tools.mkdir(mode=0o700)
    script = tools/'fake'
    script.write_text('#!'+sys.executable+'\n'+r'''
import json, os, pathlib, signal, sys, time
name=pathlib.Path(sys.argv[0]).name; args=sys.argv[1:]; state=pathlib.Path(os.environ['TEST_STATE'])
with (state/'calls').open('a') as f:f.write(json.dumps([name,args])+'\n')
if name=='hyprctl':
 print(json.dumps([{'x':0,'y':0,'width':1920,'height':1080,'scale':1,'activeWorkspace':{'id':1}}] if args[0]=='monitors' else [{'at':[10,20],'size':[300,200],'workspace':{'id':1}}]))
elif name=='slurp':
 assert '10,20 300x200' in sys.stdin.read();print('11,21 1x1')
elif name=='zenity':
 title=next(a for a in args if a.startswith('--title='))
 if title=='--title=Record screen':assert '--print-column=2' in args;print(os.environ.get('AUDIO','Silent'))
 elif title.startswith('--title=Select'):print('0')
 elif title=='--title=Recording':
  for i in range(100):
   if (state/'record-ready').exists():break
   time.sleep(.01)
  if os.environ.get('DISCARD'):sys.exit(1)
 elif title=='--title=Trim recording':
  if os.environ.get('TRIM'):print(os.environ['TRIM'])
  else:sys.exit(1)
 elif title=='--title=Share recording?':sys.exit(0 if os.environ.get('SHARE') else 1)
 else:raise AssertionError(title)
elif name=='wf-recorder':
 assert args[args.index('-g')+1]=='10,20 300x200'
 path=pathlib.Path(args[args.index('-f')+1]);path.write_bytes(b'original recording')
 (state/'pid').write_text(str(os.getpid()))
 signal.signal(signal.SIGINT,lambda *_:sys.exit(0))
 (state/'record-ready').touch()
 while True:time.sleep(.01)
elif name=='ffprobe':
 assert pathlib.Path(args[-1]).read_bytes() in (b'original recording',b'trimmed recording');print('2.000')
elif name=='ffmpeg':
 assert '-n' in args and args[args.index('-ss')+1]=='0.5' and args[args.index('-t')+1]=='1'
 assert pathlib.Path(args[args.index('-i')+1]).read_bytes()==b'original recording'
 if os.environ.get('TRIM_FAIL'):sys.exit(1)
 pathlib.Path(args[-1]).write_bytes(b'trimmed recording')
elif name=='wl-copy':(state/'clipboard').write_bytes(sys.stdin.buffer.read())
elif name=='curl':
 assert args[0]=='-q' and args[-1]=='https://0x0.st' and 'expires=24' in args
 form=args[args.index('--form')+1];data=pathlib.Path(form.split(';')[0][6:]).read_bytes()
 assert data==b'trimmed recording';(state/'uploaded').write_bytes(data);print('https://0x0.st/test.mp4')
elif name=='pactl':
 counter=state/'pulse-counter';tick=int(counter.read_text()) if counter.exists() else 0;counter.write_text(str(tick+1))
 original={'index':3,'client':4,'sink':2,'volume':tick,'properties':{'application.name':'Test app','media.name':'Changing title '+str(tick),'object.serial':'stable-stream','application.process.id':'123'}}
 if 'list' in args:
  kind=args[-1]
  if kind=='sources':print(json.dumps([{'index':1,'name':'test_mic','description':'Test mic'}]))
  elif kind=='sink-inputs':
   if (state/'moved').exists():original['sink']=9
   print(json.dumps([original]))
  elif kind=='sinks':print(json.dumps([{'index':9,'name':(state/'route').read_text(),'owner_module':7}]))
  else:raise AssertionError(kind)
 elif args[0]=='load-module':
  assert args[1]=='module-remap-sink' and 'master=2' in args
  (state/'route').write_text(next(a.split('=',1)[1] for a in args if a.startswith('sink_name=')));print(7)
 elif args[0]=='move-sink-input':
  assert args[1]=='3'
  if args[2]=='2':(state/'restored').touch()
  else:(state/'moved').touch()
 elif args[0]=='unload-module':assert args[1]=='7';(state/'unloaded').touch()
 else:raise AssertionError(args)
else:raise AssertionError(name)
''');script.chmod(0o700)
    for name in ('hyprctl','slurp','zenity','wf-recorder','ffprobe','ffmpeg','wl-copy','curl','pactl'):(tools/name).symlink_to(script)
    def case(name, **values):
        directory=root/name;directory.mkdir(mode=0o700)
        home=directory/'home';home.mkdir(mode=0o700)
        runtime=directory/'runtime';runtime.mkdir(mode=0o700)
        state=directory/'state';state.mkdir(mode=0o700)
        env=dict(os.environ, HOME=str(home),XDG_RUNTIME_DIR=str(runtime),TEST_STATE=str(state),PATH=str(tools)+os.pathsep+os.environ['PATH'],**values)
        result=subprocess.run([str(binary)],env=env,capture_output=True,timeout=15)
        assert not list(runtime.iterdir())
        if (state/'pid').exists():
            try:os.kill(int((state/'pid').read_text()),0)
            except ProcessLookupError:pass
            else:raise AssertionError('recorder survived')
        paths=list((home/'Videos/Recordings').glob('*.mp4'))
        for path in paths:assert path.stat().st_mode & 0o777==0o600
        calls=[json.loads(line) for line in (state/'calls').read_text().splitlines()]
        return result,paths,state,calls
    result,paths,state,calls=case('silent');assert result.returncode==0,result.stderr;assert len(paths)==1 and paths[0].read_bytes()==b'original recording';assert not any(name in ('pactl','curl','ffmpeg') for name,_ in calls)
    result,paths,state,calls=case('trim',TRIM='0.5|1.5',SHARE='1');assert result.returncode==0,result.stderr;assert sorted(p.read_bytes() for p in paths)==[b'original recording',b'trimmed recording'];assert (state/'clipboard').read_text()=='https://0x0.st/test.mp4'
    result,paths,_,_=case('discard',DISCARD='1');assert result.returncode==0 and not paths
    result,paths,_,calls=case('invalid-trim',TRIM='0|999');assert result.returncode==1 and len(paths)==1 and not any(name=='ffmpeg' for name,_ in calls)
    result,paths,_,_=case('failed-trim',TRIM='0.5|1.5',TRIM_FAIL='1');assert result.returncode==1 and len(paths)==1
    result,paths,_,calls=case('microphone',AUDIO='Microphone');assert result.returncode==0,result.stderr;assert '--audio=test_mic' in next(args for name,args in calls if name=='wf-recorder')
    result,paths,state,calls=case('application',AUDIO='Application audio');assert result.returncode==0,result.stderr;assert (state/'restored').exists() and (state/'unloaded').exists()
    result,paths,state,calls=case('application-discard',AUDIO='Application audio',DISCARD='1');assert result.returncode==0 and not paths and (state/'restored').exists() and (state/'unloaded').exists()
print('recording fixture passed')
