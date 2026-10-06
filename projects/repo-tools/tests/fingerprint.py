import hashlib,os,subprocess,sys,tempfile
from pathlib import Path
binary=str(Path(sys.argv[1]).resolve())
with tempfile.TemporaryDirectory() as root:
    root=Path(root).resolve(); path=root/'- quote " space'; data=b'abc\x00\xff'*50000;path.write_bytes(data)
    expected=hashlib.sha256(data).hexdigest()
    def invoke(*args,env=None): return subprocess.run([binary,*map(str,args)],capture_output=True,text=True,env=env,timeout=10)
    result=invoke('--',path); assert result.returncode==0 and result.stdout.strip()==expected
    assert path.read_bytes()==data
    link=root/'link';link.symlink_to(path);assert invoke('--',link).returncode!=0
    fifo=root/'fifo';os.mkfifo(fifo);assert invoke('--',fifo).returncode!=0
    fake=root/'bin';fake.mkdir(); clip=fake/('pbcopy' if sys.platform=='darwin' else 'wl-copy');receipt=root/'receipt'
    clip.write_text('#!/usr/bin/env python3\nimport sys,os\nfrom pathlib import Path\nassert sys.argv[1:]==["--type","text/plain;charset=utf-8"]\nPath(os.environ["COPY_RECEIPT"]).write_bytes(sys.stdin.buffer.read())\n');clip.chmod(0o700)
    env={**os.environ,'PATH':str(fake)+os.pathsep+os.environ['PATH'],'COPY_RECEIPT':str(receipt)}
    assert invoke('--copy','--',path,env=env).returncode==0
    assert receipt.read_text()==expected and path.read_bytes()==data
    clip.write_text('#!/bin/sh\nexit 1\n');assert invoke('--copy','--',path,env=env).returncode!=0
print('fingerprint: streamed vector, unusual path, unchanged original, symlink/FIFO refusal and explicit clipboard passed')
