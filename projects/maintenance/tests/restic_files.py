"""Real encrypted disposable repository; never opens personal backup data."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
backend=Path(sys.argv[1]).resolve()
restic=Path(sys.argv[2]).resolve()
with tempfile.TemporaryDirectory(prefix='seele-restic-files-fixture-') as temporary:
    root=Path(temporary);root.chmod(0o700)
    home=root/'home';home.mkdir(mode=0o700)
    source=home/'[draft]*.md';source.write_bytes(b'old notes\n')
    password=root/'password';password.write_text('disposable-fixture-password');password.chmod(0o600)
    repository=root/'repository';reference=root/'reference';reference.write_text(str(repository));reference.chmod(0o600)
    def r(*args):
        out=subprocess.run([str(restic),'--no-cache','-r',str(repository),'-p',str(password),*map(str,args)],env=dict(os.environ,RESTIC_PASSWORD=''),capture_output=True,timeout=40)
        assert out.returncode==0,out.stderr.decode();return out.stdout
    r('init')
    r('backup','--host','fixture','--tag','seele',source)
    old=json.loads(r('snapshots','--json'))[0]['id']
    source.write_bytes(b'new notes with a change\n')
    r('backup','--host','fixture','--tag','seele',source)
    r('backup','--host','another-host','--tag','seele',source)
    foreign=next(row['id'] for row in json.loads(r('snapshots','--json')) if row['hostname']=='another-host')
    env=dict(os.environ,SEELE_RESTIC_BIN=str(restic),SEELE_BACKUP_REPOSITORY_FILE=str(reference),SEELE_BACKUP_PASSWORD_FILE=str(password),SEELE_BACKUP_ALLOWED_HOME=str(home),SEELE_BACKUP_HOST='fixture')
    def call(*args):return subprocess.run([str(backend),*map(str,args)],env=env,capture_output=True,timeout=40)
    result=call('versions',source);assert result.returncode==0,result.stderr
    versions=json.loads(result.stdout);assert len(versions)==2 and old in [row['snapshot'] for row in versions] and foreign not in [row['snapshot'] for row in versions]
    result=call('dump',old,source);assert result.returncode==0 and result.stdout==b'old notes\n'
    assert source.read_bytes()==b'new notes with a change\n'
    result=call('dump',foreign,source);assert result.returncode==1 and not result.stdout
    result=call('versions','/etc/passwd');assert result.returncode==1 and not result.stdout
    result=call('dump','latest',source);assert result.returncode==1 and not result.stdout
    result=call('versions',str(home)+'/../reference');assert result.returncode==1 and not result.stdout
    reference.chmod(0o644);result=call('versions',source);assert result.returncode==1 and not result.stdout
print('encrypted restic file versions: exact glob path, tagged host filter, old bytes, preservation and credential guards passed')
