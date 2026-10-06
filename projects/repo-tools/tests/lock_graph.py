import json,os,subprocess,sys,tempfile
from pathlib import Path
binary=str(Path(sys.argv[1]).resolve())
with tempfile.TemporaryDirectory() as directory:
    root=Path(directory);lock=root/'flake.lock'
    data={'version':7,'root':'root','nodes':{'root':{'inputs':{'pkg':'pkg','alias':['pkg','dep']}},'pkg':{'inputs':{'dep':'lib'}},'lib':{'locked':{'url':'https://sensitive@example.invalid?token=do-not-print','rev':'do-not-print'}}}}
    lock.write_text(json.dumps(data));original=lock.read_bytes()
    def check(path):return subprocess.run([binary,'--json','--',str(path)],capture_output=True,text=True,timeout=5)
    reply=check(lock);assert reply.returncode==0;graph=json.loads(reply.stdout)
    assert 'do-not-print' not in reply.stdout+reply.stderr and 'https' not in reply.stdout
    assert any(edge['target']=='lib' and edge['follows'] for edge in graph['edges'])
    assert lock.read_bytes()==original
    data['nodes']['root']['inputs']['bad']='missing';lock.write_text(json.dumps(data))
    reply=check(lock);assert reply.returncode!=0 and reply.stdout=='' and 'do-not-print' not in reply.stderr
    link=root/'link';link.symlink_to(lock);assert check(link).returncode!=0
    fifo=root/'fifo';os.mkfifo(fifo);assert check(fifo).returncode!=0
    lock.write_bytes(b' '* (8*1024*1024+1));assert check(lock).returncode!=0
print('lock graph: production follows topology, no source metadata, immutable input, bounded files and missing-reference refusal passed')
