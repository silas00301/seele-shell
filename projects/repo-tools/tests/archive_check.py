import gzip,hashlib,os,subprocess,sys,tempfile,zipfile
from pathlib import Path
binary=str(Path(sys.argv[1]).resolve());sevenzip=str(Path(sys.argv[2]).resolve())
with tempfile.TemporaryDirectory() as directory:
    root=Path(directory);tools=root/'bin';tools.mkdir();(tools/'7zz').symlink_to(sevenzip)
    env={**os.environ,'PATH':str(tools)+os.pathsep+os.environ['PATH']}
    def check(path):return subprocess.run([binary,'--',str(path)],env=env,cwd=root,capture_output=True,text=True,timeout=10)
    archive=root/'- weird zip.data'
    with zipfile.ZipFile(archive,'w',compression=zipfile.ZIP_STORED) as z:z.writestr('../must-not-extract','private fixture payload')
    before=hashlib.sha256(archive.read_bytes()).digest()
    ok=check(archive);assert ok.returncode==0,(ok.stdout,ok.stderr)
    assert hashlib.sha256(archive.read_bytes()).digest()==before
    assert not (root.parent/'must-not-extract').exists()
    damaged=root/'bad.zip';damaged.write_bytes(archive.read_bytes().replace(b'private fixture payload',b'changed fixture payload'))
    assert check(damaged).returncode!=0
    compressed=root/'compressed';compressed.write_bytes(gzip.compress(b'hello'*1000));assert check(compressed).returncode==0
    compressed.write_bytes(compressed.read_bytes()[:-4]);assert check(compressed).returncode!=0
    payload=root/'payload';payload.write_text('test only')
    encrypted=root/'encrypted.7z'
    subprocess.run([sevenzip,'a','-y','-ptest-only','-mhe=on',str(encrypted),str(payload)],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,check=True)
    result=check(encrypted);assert result.returncode!=0,(result.stdout,result.stderr)
    assert 'test-only' not in result.stdout+result.stderr and 'payload' not in result.stdout+result.stderr
    link=root/'link';link.symlink_to(archive);assert check(link).returncode!=0
    fifo=root/'fifo';os.mkfifo(fifo);assert check(fifo).returncode!=0
    assert payload.read_text()=='test only'
print('archive check: real 7-Zip ZIP/gzip CRCs, corruption, encryption refusal, no extraction, immutable originals and unsafe-file refusal passed')
