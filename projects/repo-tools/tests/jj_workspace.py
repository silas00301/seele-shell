"""Resolve PR repositories through real Jujutsu workspaces, without GitHub writes."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

BINARY = Path(sys.argv[1]).resolve()
GIT = shutil.which('git')
JJ = shutil.which('jj')
assert GIT and JJ, 'This fixture requires Git and Jujutsu'

with tempfile.TemporaryDirectory(prefix='seele-pr-workspace-') as temporary:
    root = Path(temporary)
    home = root / 'home'
    (home / '.config/jj').mkdir(parents=True)
    (home / '.config/jj/config.toml').write_text(
        '[user]\nname="Fixture"\nemail="fixture@example.invalid"\n'
        '[signing]\nbehavior="drop"\n')
    tools = root / 'bin'
    tools.mkdir()
    log = root / 'gh-calls'
    stub = tools / 'gh'
    stub.write_text('#!' + sys.executable + '\n' + '''import json,os,pathlib,subprocess,sys
assert not any(key in os.environ for key in ('GIT_WORK_TREE','GIT_COMMON_DIR','GIT_INDEX_FILE','GH_REPO'))
remote=subprocess.check_output([os.environ['TEST_GIT'],'remote','get-url','origin'],text=True).strip()
with open(os.environ['TEST_LOG'],'a') as out:
 out.write(json.dumps([sys.argv[1:],remote,os.getcwd()])+"\\n")
assert sys.argv[1:3] in (['pr','create'],['pr','list'])
if sys.argv[1:3]==['pr','list']: print('[]')
''')
    stub.chmod(0o700)
    env = {key: value for key, value in os.environ.items()
           if not key.startswith(('GIT_', 'JJ_', 'XDG_', 'GH_'))}
    env.update(HOME=str(home), XDG_CONFIG_HOME=str(home / '.config'),
               PATH=str(tools) + os.pathsep + os.environ['PATH'],
               GIT_CONFIG_NOSYSTEM='1', GIT_CONFIG_GLOBAL=os.devnull,
               TEST_GIT=GIT, TEST_LOG=str(log))

    def run(cwd, *args, extra=None, success=True):
        result = subprocess.run(args, cwd=cwd, env=dict(env, **(extra or {})),
                                capture_output=True, text=True, timeout=20)
        if success:
            assert result.returncode == 0, (args, result.stdout, result.stderr)
        return result

    for colocated in (True, False):
        name = 'colocated' if colocated else 'separate'
        repo = root / name
        args = ['git', 'init'] + (['--colocate'] if colocated else []) + [str(repo)]
        run(root, JJ, *args)
        remote = f'https://github.com/fixture/{name}.git'
        run(repo, JJ, 'git', 'remote', 'add', 'origin', remote)
        run(repo, JJ, 'bookmark', 'create', 'review')
        workspace = root / (name + ' workspace')
        run(repo, JJ, 'workspace', 'add', '--name', name, '-r', '@', str(workspace))
        assert not (workspace / '.git').exists()
        for cwd in (repo, workspace):
            # The caller may have been using an unrelated Git worktree. Its
            # process environment stays untouched, while gh gets this backend.
            unrelated = dict(GIT_DIR=str(root / 'wrong.git'),
                             GIT_WORK_TREE=str(root / 'wrong-tree'),
                             GIT_COMMON_DIR=str(root / 'wrong-common'),
                             GIT_INDEX_FILE=str(root / 'wrong-index'),
                             GH_REPO='wrong/repository')
            run(cwd, str(BINARY), 'submit', 'review', extra=unrelated)
            run(cwd, str(BINARY), 'checkout', extra=unrelated)
            records = [json.loads(line) for line in log.read_text().splitlines()]
            assert records[-2] == [['pr', 'create', '--head', 'review'], remote, str(cwd)]
            assert records[-1] == [['pr', 'list', '--json', 'number,title'], remote, str(cwd)]

    previous = log.read_text()
    failure = run(root, str(BINARY), 'checkout', success=False)
    assert failure.returncode != 0
    assert 'Git backend' in failure.stderr
    assert log.read_text() == previous
    print('Jujutsu PR workspace fixture passed')
