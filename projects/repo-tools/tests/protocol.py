"""Run real native repo helpers against fake Git/JJ/GitHub/Nix executables."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

BIN=Path(sys.argv[1]).resolve() if len(sys.argv)>1 else Path(__file__).resolve().parents[3]/'target/debug'
HASH='sha256-'+'A'*43+'='


def main():
    with tempfile.TemporaryDirectory(prefix='seele-repo-fixture-') as temporary:
        root=Path(temporary);root.chmod(0o700);tools=root/'bin';tools.mkdir(mode=0o700);repo=root/'repo';(repo/'modules/packages').mkdir(parents=True);(repo/'seele-shell').mkdir();state=root/'state';state.mkdir()
        script=tools/'command';script.write_text(('#!' + sys.executable + '\n') + '''import json,os,pathlib,sys
name=pathlib.Path(sys.argv[0]).name;args=sys.argv[1:];state=pathlib.Path(os.environ['TEST_STATE']);root=pathlib.Path(os.environ['TEST_ROOT'])
with(state/'calls').open('a')as log:log.write(json.dumps([name,args])+"\\n")
mode=os.environ.get('TEST_MODE','')
if name=='jj':
 if '--template'in args:print('topic' if mode!='multiple-bookmarks'else'topic\\nother',end='')
 elif args[:1]==['log']and'change_id'in args:print('kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk'if args[args.index('-r')+1]=='@'else'zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz',end='')
 elif 'remote_bookmarks(remote=origin)'in' '.join(args):print('a'*40 if mode!='unpublished'else'',end='')
 elif 'commit_id'in args:print('b'*40,end='')
 if mode=='fetch-fails'and'fetch'in args:sys.exit(42)
elif name=='gh':
 if args[:2]==['pr','list']:print(json.dumps([{'number':7,'title':'A title'}]))
 elif args[:2]==['pr','view']:print(json.dumps({'headRefName':'feature/topic','headRepositoryOwner':{'login':'owner' if mode!='unsafe-owner'else'owner@evil'},'headRepository':{'name':'repo'}}))
elif name=='gum':
 values=sys.stdin.read();assert '#7 | A title'in values
 print('#7 | A title')
elif name=='git':
 assert os.environ.get('GIT_LITERAL_PATHSPECS')=='1' and 'GIT_GLOB_PATHSPECS'not in os.environ
 if '--show-superproject-working-tree'in args:print('')
 elif '--show-toplevel'in args:print(root)
 elif '--is-inside-work-tree'in args:print('true')
 elif 'status'in args:print(' M dirty'if mode=='dirty'else'',end='')
 elif 'ls-tree'in args and args[-1]=='flake.lock':print('100644 blob '+('c'*40 if mode!='changed-lock' or args[args.index('ls-tree')+1]=='a'*40 else'd'*40)+'\\tflake.lock')
 elif 'ls-tree'in args:print('160000 commit '+'a'*40+'\\tseele-shell')
 elif '--abbrev-ref'in args:print('main'if mode=='attached-main'else'HEAD')
 elif 'diff'in args and mode=='parent-lock-dirty':sys.exit(1)
 elif 'rev-parse'in args and'HEAD'in args:print('b'*40)
elif name=='curl':
 assert args[0]=='-q'
 if 'steipete/CodexBar'in args[-1]:
  version='1.2.3'if mode!='bad-release'else'1\\"; injected = true; version = \\"'
  print(json.dumps([{'draft':False,'prerelease':False,'tag_name':'v'+version,'assets':[{'name':'CodexBarCLI-v'+version+'-linux-x86_64.tar.gz','browser_download_url':'https://github.com/steipete/CodexBar/releases/download/v1.2.3/asset'}]}]))
 else:print(json.dumps([{'draft':False,'prerelease':True,'tag_name':'v2.0.0-nightly.1','assets':[{'name':'T3-Code-2.0.0-nightly.1-x86_64.AppImage','digest':'sha256:'+'a'*64}]}]))
elif name=='nix':
 if args[:2]==['store','prefetch-file']:print(json.dumps({'hash':'sha256-'+'A'*43+'='}))
 elif args[:2]==['hash','convert']:print('sha256-'+'A'*43+'=')
else:raise AssertionError(name)
''');script.chmod(0o700)
        for name in('jj','gh','gum','git','curl','nix'):(tools/name).symlink_to(script)
        base=dict(os.environ,PATH=str(tools)+os.pathsep+os.environ['PATH'],TEST_STATE=str(state),TEST_ROOT=str(repo))
        def invoke(binary,*args,mode=''):
            (state/'calls').write_text('');result=subprocess.run([str(BIN/binary),*args],cwd=repo,env=dict(base,TEST_MODE=mode),capture_output=True,timeout=10)
            return result,[json.loads(line)for line in(state/'calls').read_text().splitlines()]
        result,calls=invoke('jj-flip');assert result.returncode==0,result.stderr;assert calls[2][1]==['parallelize','k'*32,'z'*32];assert calls[3][1]==['rebase','--branch','z'*32,'--destination','k'*32]
        result,calls=invoke('jj-pr','submit');assert result.returncode==0;assert calls[-1]==['gh',['pr','create','--head','topic']]
        result,calls=invoke('jj-pr','submit',mode='multiple-bookmarks');assert result.returncode==1 and not any(name=='gh'for name,_ in calls)
        for args in [('checkout','7'),('co',)]:
            result,calls=invoke('jj-pr',*args);assert result.returncode==0,result.stderr
            remote=next(args[3]for name,args in calls if name=='jj'and args[:3]==['git','remote','add'])
            assert next(args for name,args in calls if name=='jj'and args[:2]==['git','fetch'])==['git','fetch','--remote',remote,'--branch','exact:"feature/topic"']
            assert ['jj',['new','"feature/topic"@'+json.dumps(remote)]]in calls
            assert calls[-1]==['jj',['git','remote','remove',remote]]
        result,calls=invoke('jj-pr','checkout','7',mode='fetch-fails');assert result.returncode==42;assert calls[-1][1][:3]==['git','remote','remove'];assert not any(args[:1]==['new']for _,args in calls)
        result,calls=invoke('jj-pr','checkout','7',mode='unsafe-owner');assert result.returncode==1 and not any(name=='jj'for name,_ in calls)
        for mode in('dirty','unpublished'):
            result,calls=invoke('update-submodule',mode=mode);assert result.returncode==1;assert not any(name=='git'and'add'in args for name,args in calls)
        result,calls=invoke('update-submodule','../../outside');assert result.returncode==2 and not calls
        result,calls=invoke('update-submodule');assert result.returncode==0,result.stderr;assert calls[-1]==['nix',['flake','update','seele-shell']];assert any(name=='git'and'--only'in args for name,args in calls)
        for args in [('--keep-lock',),('--pr','--pr'),('--unknown',)]:
            result,calls=invoke('update-submodule',*args);assert result.returncode==2 and not calls
        for mode in ('attached-main','changed-lock','parent-lock-dirty'):
            result,calls=invoke('update-submodule','--pr','--keep-lock',mode=mode);assert result.returncode==1,(mode,result.stderr);assert not any(name=='git'and'add'in args for name,args in calls);assert not any(name=='nix'for name,_ in calls)
        result,calls=invoke('update-submodule','--pr','--keep-lock');assert result.returncode==0,result.stderr;assert not any(name=='nix'or name=='jj'and'bookmark'in args for name,args in calls);assert b'No Nix evaluation'in result.stdout
        result,calls=invoke('update-submodule','--pr');assert result.returncode==0,result.stderr;assert calls[-1]==['nix',['flake','update','seele-shell']];assert not any(name=='jj'and'bookmark'in args for name,args in calls)
        codex=repo/'modules/packages/codexbar.nix';t3=repo/'modules/packages/t3code.nix'
        original='version = "old";\nhash = "old";\n';codex.write_text(original);t3.write_text(original)
        result,calls=invoke('update-packaged',mode='bad-release');assert result.returncode==1;assert codex.read_text()==original and t3.read_text()==original
        result,calls=invoke('update-packaged');assert result.returncode==0,result.stderr;assert'1.2.3'in codex.read_text()and HASH in codex.read_text();assert'2.0.0-nightly.1'in t3.read_text()and HASH in t3.read_text();assert not list((repo/'modules/packages').glob('.tmp*'))
    print('Native repo helpers: stable flip IDs, safe PR metadata, picker, fork cleanup, publication/dirty gates, native updates and injection-resistant package pins passed')


if __name__=='__main__':main()
