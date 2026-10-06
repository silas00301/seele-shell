"""Encrypted local restic repository and production verifier; no user data."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

binary=Path(sys.argv[1]).resolve()
restic=Path(os.environ['SEELE_TEST_RESTIC']).resolve()
with tempfile.TemporaryDirectory(prefix='seele-restic-fixture-') as temporary:
    root=Path(temporary);root.chmod(0o700)
    repository=root/'repository';password=root/'password';reference=root/'repository-file'
    password.write_text('synthetic-local-password');password.chmod(0o600)
    reference.write_text(str(repository));reference.chmod(0o600)
    environment=dict(os.environ,RESTIC_REPOSITORY_FILE=str(reference),RESTIC_PASSWORD_FILE=str(password),RESTIC_CACHE_DIR=str(root/'cache'))
    def run(*args):
        return subprocess.run([str(restic),*args],env=environment,capture_output=True,check=True,timeout=30)
    run('init')
    source=root/'source';source.mkdir(mode=0o700)
    sample=source/'real document.txt'
    content=b'PRIVATE-FIXTURE-CONTENT\x00\xff\n'
    sample.write_bytes(content)
    run('backup','--host','fixture','--tag','seele',str(source))
    # Snapshot content must be restored, even after the original changes.
    sample.write_bytes(b'new live content')
    runtime=root/'runtime';runtime.mkdir(mode=0o700)
    receipt=root/'receipt.json';config=root/'config.json'
    cfg=dict(restic=str(restic),host='fixture',samples=[str(sample)],runtime=str(runtime),receipt=str(receipt))
    config.write_text(json.dumps(cfg))
    result=subprocess.run([str(binary),str(config)],env=environment,capture_output=True,timeout=30)
    assert result.returncode==0 and not result.stdout and not result.stderr,result
    record=json.loads(receipt.read_text())
    assert record['restored']==[dict(path=str(sample),bytes=len(content),sha256=hashlib.sha256(content).hexdigest())],record
    assert len(record['snapshot'])==64 and receipt.stat().st_mode & 0o777 == 0o600
    assert not list(runtime.iterdir()) and sample.read_bytes()==b'new live content'
    previous=receipt.read_bytes()
    for changes in [dict(samples=[str(source/'missing')]),dict(host='missing'),dict(samples=[str(source/'*')]),dict(samples=[])]:
        config.write_text(json.dumps(dict(cfg,**changes)))
        result=subprocess.run([str(binary),str(config)],env=environment,capture_output=True,timeout=30)
        assert result.returncode!=0 and not result.stdout and result.stderr==b'backup restore check failed\n',result
        assert not list(runtime.iterdir()) and receipt.read_bytes()==previous
    config.write_text(json.dumps(cfg))
    backend=root/'backend-environment';backend.write_text('SYNTHETIC=fixture');backend.chmod(0o644)
    result=subprocess.run([str(binary),str(config)],env=dict(environment,SEELE_BACKUP_ENVIRONMENT_FILE=str(backend)),capture_output=True,timeout=30)
    assert result.returncode!=0 and not list(runtime.iterdir()) and receipt.read_bytes()==previous
    password.chmod(0o644)
    result=subprocess.run([str(binary),str(config)],env=environment,capture_output=True,timeout=30)
    assert result.returncode!=0 and not list(runtime.iterdir()) and receipt.read_bytes()==previous
print('Real restic: encrypted snapshot restore, byte comparison, private receipt, original preservation and failure cleanup passed')
