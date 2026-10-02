"""Pi status must read recorded Jujutsu state without snapshotting user files."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

BINARY = Path(sys.argv[1]).resolve()
JJ = shutil.which('jj')
assert JJ, 'This fixture requires Jujutsu'
TEMPLATE = 'if(self.local_bookmarks(), self.local_bookmarks().join(","), change_id.shortest(8))'

with tempfile.TemporaryDirectory(prefix='seele-pi-readonly-') as temporary:
    root = Path(temporary)
    home = root / 'home'
    (home / '.config/jj').mkdir(parents=True)
    (home / '.config/jj/config.toml').write_text(
        '[user]\nname="Fixture"\nemail="fixture@example.invalid"\n'
        '[signing]\nbehavior="drop"\n')
    env = {key: value for key, value in os.environ.items()
           if not key.startswith(('GIT_', 'JJ_', 'XDG_'))}
    env.update(HOME=str(home), XDG_CONFIG_HOME=str(home / '.config'),
               GIT_CONFIG_NOSYSTEM='1', GIT_CONFIG_GLOBAL=os.devnull)

    def run(cwd, *args):
        result = subprocess.run(args, cwd=cwd, env=env, capture_output=True,
                                text=True, timeout=10)
        assert result.returncode == 0, (args, result.stdout, result.stderr)
        return result.stdout

    def read(cwd, *args):
        return run(cwd, JJ, '--ignore-working-copy', *args)

    def identity(cwd):
        return (read(cwd, 'log', '-r', '@', '--no-graph', '-T', 'commit_id'),
                read(cwd, 'op', 'log', '--no-graph', '--limit', '1', '-T', 'id'))

    for colocated in (False, True):
        repo = root / ('colocated' if colocated else 'separate')
        run(root, JJ, 'git', 'init', *(['--colocate'] if colocated else []), str(repo))
        tracked = repo / 'tracked.txt'
        tracked.write_text('original\n')
        run(repo, JJ, 'describe', '-m', 'recorded work')
        run(repo, JJ, 'bookmark', 'create', 'topic')
        for named in (True, False):
            if not named:
                # Forget explicitly before making this round's unsnapshotted edits.
                tracked.write_text('original\n')
                (repo / 'untracked.txt').unlink()
                run(repo, JJ, 'bookmark', 'forget', 'topic')
            before = identity(repo)
            expected = read(repo, 'log', '--no-graph', '-r', '@', '-T', TEMPLATE)
            if named:
                assert expected == 'topic'
            else:
                assert expected and expected != 'topic'
            tracked.write_text('unsnapshotted edit\n')
            (repo / 'untracked.txt').write_text('unrelated private work\n')
            for mode in ('detect', 'revision', 'detect'):
                output = json.loads(run(repo, str(BINARY), JJ, mode))
                assert output == {'repository': True, 'revision': expected}, output
                assert identity(repo) == before, 'status query recorded a new repository operation'
                assert 'untracked.txt' not in read(repo, 'file', 'list', '-r', '@')
                assert read(repo, 'file', 'show', '-r', '@', 'tracked.txt') == 'original\n'
                assert tracked.read_text() == 'unsnapshotted edit\n'
    assert json.loads(run(root, str(BINARY), JJ, 'detect')) == {
        'repository': False, 'revision': ''}
    print('Pi status leaves real dirty Jujutsu repositories unchanged')
