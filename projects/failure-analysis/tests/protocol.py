"""Exercise installed Rust failure helpers with fake tools and a private broker."""
import json
import os
from pathlib import Path
import socket
import stat
import subprocess
import sys
import tempfile
import threading

ROOT = Path(__file__).resolve().parents[3]
BINARY = Path(sys.argv[1]).resolve() if len(sys.argv)>1 else ROOT/'target/debug/seele-failure-report'
GENERATOR = BINARY.with_name('seele-failure-generator')
REBUILD = BINARY.with_name('seele-rebuild')
RB = BINARY.with_name('seele-rb')


def fixture(root, action=''):
    runtime=root/'runtime';runtime.mkdir(mode=0o700)
    tools=root/'tools';tools.mkdir(mode=0o700)
    calls=root/'calls.jsonl'
    common=('#!' + sys.executable + '\n') + 'import json,os,pathlib,sys\np=pathlib.Path('+repr(str(calls))+')\nwith p.open("a") as f:f.write(json.dumps([pathlib.Path(sys.argv[0]).name,sys.argv[1:]])+"\\n")\n'
    scripts={
      'notify':common+'marker=pathlib.Path('+repr(str(root/'notified'))+')\nif not marker.exists():\n marker.touch()\n print('+repr(action)+')\n',
      'systemctl':common+'print("FIXTURE_TOKEN=known-secret")\n',
      'systemd-run':common,
      'nh':common+'os.write(1,b"\\r\\x1b[2Kbuilding 1/2\\r\\x1b[2Kbuilding 2/2 "+"✓".encode()+b"\\r\\n")\nos.write(2,b"failed progress tail")\nsys.exit(int(os.environ.get("FIXTURE_STATUS","0")))\n',
      'pi':common+'raise AssertionError("Pi must never run")\n',
    }
    for name,script in scripts.items():path=tools/name;path.write_text(script);path.chmod(0o700)
    environment=dict(os.environ,XDG_RUNTIME_DIR=str(runtime),SEELE_FAILURE_NOTIFY=str(tools/'notify'),SEELE_FAILURE_SYSTEMCTL=str(tools/'systemctl'),SEELE_FAILURE_SYSTEMD_RUN=str(tools/'systemd-run'),SEELE_FAILURE_NH=str(tools/'nh'),SEELE_FAILURE_PI=str(tools/'pi'),FIXTURE_TOKEN='known-secret')
    return runtime,calls,environment


def run(args,environment,payload=b'raw report'):
    result=subprocess.run([str(BINARY),*args],input=payload,capture_output=True,env=environment,timeout=15)
    assert not result.stderr,result.stderr
    return result


def offer(environment,payload=b'raw report'):
    return run(['store-offer','--subject','demo.service','--summary','failed TOKEN=known-secret'],environment,payload)


def main():
    for action in ('','view','analyze'):
        with tempfile.TemporaryDirectory(prefix='seele-failure-fixture-') as temporary:
            root=Path(temporary);root.chmod(0o700);runtime,calls,environment=fixture(root,action)
            received=[];thread=None;listener=None
            if action=='analyze':
                listener=socket.socket(socket.AF_UNIX);listener.bind(str(runtime/'seele-codex.sock'));listener.listen();(runtime/'seele-codex.sock').chmod(0o600)
                def serve():
                    for expected in ('submit','wait','release'):
                        client,_=listener.accept()
                        with client,client.makefile('rwb') as stream:
                            message=json.loads(stream.readline());received.append(message);assert message['op']==expected
                            reply={'ok':True,'epoch':'00000000-0000-4000-8000-000000000001','job':{'id':'00000000-0000-4000-8000-000000000002','state':'succeeded'},'result':{'analysis':'Likely cause: bad option\nCheck the declarative unit setting.'}}
                            stream.write(json.dumps(reply).encode()+b'\n');stream.flush()
                thread=threading.Thread(target=serve,daemon=True);thread.start()
            private=b'TOKEN=known-secret\nAuthorization: Bearer opaque-value\nremote=https://alice:password@example.test/repo\ngithub=ghp_abcdefghijklmnopqrstuvwxyz123456\n"api_token": "json-secret"\nExecStart=demo --password cli-secret\n'
            result=offer(environment,private);assert result.returncode==0
            reports=list((runtime/'seele-shell/failures').glob('*.txt'));assert len(reports)==1;report=reports[0];assert stat.S_IMODE(report.stat().st_mode)==0o600
            assert report.read_bytes().startswith(private)
            log=[json.loads(line) for line in calls.read_text().splitlines()]
            assert not any(row[0]=='pi' for row in log)
            notified=json.dumps([row for row in log if row[0]=='notify']);assert 'known-secret' not in notified
            if action=='view':
                view=next(row for row in log if row[0]=='systemd-run');assert '--service-type=exec' in view[1];assert '--class=org.seele.failure' in view[1];assert '--clean' in view[1]
            else:assert not any(row[0]=='systemd-run' for row in log)
            if action=='analyze':
                thread.join(timeout=5);assert not thread.is_alive();listener.close();assert 'AI analysis (explicitly requested)' in report.read_text()
                context=json.dumps(received[0]['request']['context'])
                for secret in ('known-secret','opaque-value','alice:password','ghp_abcdefghijklmnopqrstuvwxyz123456','json-secret','cli-secret'):assert secret not in context,secret
                assert received[0]['request']['consumer']=='failure-analysis'
            else:assert 'AI analysis' not in report.read_text()
            unsafe=runtime/'seele-shell/failures'/('a'*16+'.txt');unsafe.symlink_to(report)
            invalid=subprocess.run([str(BINARY),'view','a'*16],env=environment,capture_output=True,timeout=5);assert invalid.returncode==1
            huge=subprocess.run([str(BINARY),'store-envelope'],input=b'x'*(512*1024+16*1024+1),env=environment,capture_output=True,timeout=5);assert huge.returncode==1
    for code in (0,4):
        with tempfile.TemporaryDirectory(prefix='seele-rebuild-fixture-') as temporary:
            root=Path(temporary);root.chmod(0o700);runtime,calls,environment=fixture(root);environment['FIXTURE_STATUS']=str(code)
            result=subprocess.run([str(REBUILD),'os','switch','--dry'],env=environment,capture_output=True,timeout=10)
            assert result.returncode==code,(result.returncode,result.stderr)
            assert result.stdout==b'\r\x1b[2Kbuilding 1/2\r\x1b[2Kbuilding 2/2 '+ '✓'.encode()+b'\r\nfailed progress tail',result.stdout
            assert not result.stderr
            log=[json.loads(line)for line in calls.read_text().splitlines()];assert log[0]==['nh',['os','switch','--dry']]
            assert any(row[0]=='notify'for row in log)==bool(code)
    # rb: record, check, build, diff, then activate the exact built store path.
    rb_tools={
      'jj':'print("kxtnwmpq 1a2b3c4d Reviewed change")\nif os.environ.get("FIXTURE_FAIL")=="jj":sys.exit(1)\n',
      'nix':'print("checking flake outputs")\nif os.environ.get("FIXTURE_FAIL")=="check":print("error: option does not exist");sys.exit(1)\n',
      'nh':('a=sys.argv[1:]\nif a[:2]==["os","build"]:\n if os.environ.get("FIXTURE_FAIL")=="build":print("error: build of nixos-system failed");sys.exit(1)\n'
            ' link=pathlib.Path(a[a.index("--out-link")+1]);target=pathlib.Path(os.environ["FIXTURE_BUILT"]);target.mkdir(parents=True,exist_ok=True);link.symlink_to(target)\n'
            'print("activating" if a[:2]==["os","switch"] else "built")\n'),
      'nvd':'print("<<< current\\n>>> built\\nAdded packages: 1")\n',
    }
    def rb(root,arguments,fail='',same=False,stdin=subprocess.DEVNULL):
        runtime,calls,environment=fixture(root)
        tools=root/'tools';store=root/'store';store.mkdir()
        for name,body in rb_tools.items():
            path=tools/name;path.write_text(('#!'+sys.executable+'\n')+'import json,os,pathlib,sys\np=pathlib.Path('+repr(str(calls))+')\nwith p.open("a") as f:f.write(json.dumps([pathlib.Path(sys.argv[0]).name,sys.argv[1:]])+"\\n")\n'+body);path.chmod(0o700)
        built=store/'bbbb-nixos-system-nerv';current=store/'aaaa-nixos-system-nerv';current.mkdir()
        if same:built=current
        running=root/'current-system';running.symlink_to(current)
        environment.update(SEELE_FAILURE_JJ=str(tools/'jj'),SEELE_FAILURE_NIX=str(tools/'nix'),SEELE_FAILURE_NVD=str(tools/'nvd'),
          SEELE_REBUILD_STORE_DIR=str(store),SEELE_REBUILD_CURRENT_SYSTEM=str(running),FIXTURE_BUILT=str(built),FIXTURE_FAIL=fail,NH_FLAKE=str(root/'seele'))
        environment.pop('NH_OS_FLAKE',None)
        result=subprocess.run([str(RB),*arguments],env=environment,stdin=stdin,capture_output=True,timeout=15)
        log=[json.loads(line) for line in calls.read_text().splitlines()]
        return result,log,built,runtime
    def names(log):return [row[0]+(' '+' '.join(row[1][:2]) if row[0]=='nh' else '') for row in log if row[0] not in ('notify','systemctl')]
    for arguments,expect_switch in ((['--dry-run'],False),(['--switch'],True),([],False)):
        with tempfile.TemporaryDirectory(prefix='seele-rb-fixture-') as temporary:
            root=Path(temporary);root.chmod(0o700)
            result,log,built,runtime=rb(root,arguments)
            assert result.returncode==0,(arguments,result.returncode,result.stdout,result.stderr)
            steps=['jj','nix','nh os build','nvd']+(['nh os switch'] if expect_switch else [])
            assert names(log)==steps,(arguments,names(log))
            jj=next(row for row in log if row[0]=='jj')[1];assert jj[:2]==['--repository',str(root/'seele')] and 'log' in jj,jj
            assert next(row for row in log if row[0]=='nix')[1]==['flake','check','--no-build','--no-write-lock-file',str(root/'seele')]
            build=next(row for row in log if row[0]=='nh')[1];assert build[-1]==str(root/'seele') and '--out-link' in build
            if expect_switch:
                assert log[-2][1]==['os','switch','--diff','never',str(built)],'the diffed store path is what gets activated'
            output=result.stdout.decode()
            assert 'kxtnwmpq 1a2b3c4d Reviewed change' in output and 'Added packages: 1' in output
            if arguments==[]:assert 'No terminal to ask on' in output,'without a terminal nothing is activated'
            notified=[row for row in log if row[0]=='notify'];assert len(notified)==1
            assert ('Activated' if expect_switch else 'Built, not activated') in notified[0][1][-1]
            assert not list(runtime.glob('seele-rb-*')),'the private result link is removed'
            assert not (runtime/'seele-shell/failures').exists(),'success creates no report'
    # On a terminal the default asks, and only an explicit yes activates.
    import pty
    for answer,expect_switch in ((b'y\n',True),(b'\n',False),(b'no\n',False)):
        with tempfile.TemporaryDirectory(prefix='seele-rb-fixture-') as temporary:
            root=Path(temporary);root.chmod(0o700)
            primary,secondary=pty.openpty();os.write(primary,answer)
            try:result,log,built,runtime=rb(root,[],stdin=secondary)
            finally:os.close(secondary);os.close(primary)
            assert result.returncode==0 and b'Activate this generation? [y/N]' in result.stdout
            assert (names(log)[-1]=='nh os switch')==expect_switch,(answer,names(log))
    with tempfile.TemporaryDirectory(prefix='seele-rb-fixture-') as temporary:
        root=Path(temporary);root.chmod(0o700)
        result,log,built,runtime=rb(root,['--switch'],same=True)
        assert result.returncode==0 and names(log)==['jj','nix','nh os build'],names(log)
        assert b'nothing to activate' in result.stdout
    for fail,expected in (('jj',['jj']),('check',['jj','nix']),('build',['jj','nix','nh os build'])):
        with tempfile.TemporaryDirectory(prefix='seele-rb-fixture-') as temporary:
            root=Path(temporary);root.chmod(0o700)
            result,log,built,runtime=rb(root,['--switch'],fail=fail)
            assert result.returncode==1,(fail,result.returncode)
            assert names(log)==expected,(fail,names(log))
            assert b'Nothing was activated' in result.stdout
            reports=list((runtime/'seele-shell/failures').glob('*.txt'));assert len(reports)==1
            text=reports[0].read_text();assert 'Failed step:' in text and 'Failed command:' in text
            assert any(row[0]=='notify' and '--action' in row[1] for row in log),'a failure offers the private report'
    with tempfile.TemporaryDirectory(prefix='seele-rb-fixture-') as temporary:
        root=Path(temporary);root.chmod(0o700);runtime,calls,environment=fixture(root)
        environment.pop('NH_FLAKE',None);environment.pop('NH_OS_FLAKE',None)
        assert subprocess.run([str(RB)],env=environment,capture_output=True,timeout=5).returncode==2
        assert subprocess.run([str(RB),'--dry-run','--switch'],env=dict(environment,NH_FLAKE='/x'),capture_output=True,timeout=5).returncode==2
        assert not calls.exists(),'invalid invocations run nothing'
    with tempfile.TemporaryDirectory(prefix='seele-generator-fixture-') as temporary:
        root=Path(temporary);root.chmod(0o700);high=root/'high';low=root/'low';output=root/'output'
        for path in (high,low,output):path.mkdir(mode=0o700)
        for name in ('alpha.service','beta.service','masked.service','seele-failure-report@.service'):(low/name).write_text('fixture')
        (high/'alpha.service').write_text('higher');(high/'masked.service').symlink_to('/dev/null')
        result=subprocess.run([str(GENERATOR),str(output),str(root/'early'),str(root/'late')],env=dict(os.environ,SEELE_FAILURE_UNIT_PATH=str(high)+os.pathsep+str(low)),capture_output=True,timeout=5)
        assert result.returncode==0,result.stderr
        assert sorted(path.parent.name for path in output.glob('*.service.d/*.conf'))==['alpha.service.d','beta.service.d']
        assert (output/'alpha.service.d/50-seele-failure-report.conf').read_text()=='[Unit]\nOnFailure=seele-failure-report@%n.service\n'
    print('Rust failure analysis: consent, private reports, broker redaction, no Pi, local viewer, bounds, raw rebuild bytes/status, rb record/check/build/diff/activation and systemd generator passed')


if __name__=='__main__':main()
