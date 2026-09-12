"""Exercise the gitlink compatibility transaction in private, offline Git/JJ repos."""
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

with tempfile.TemporaryDirectory(prefix='seele-submodule-fixture-') as temporary:
    root = Path(temporary)
    root.chmod(0o700)
    home = root / 'home'
    (home / '.config/jj').mkdir(parents=True, mode=0o700)
    (home / '.config/jj/config.toml').write_text('[user]\nname="Fixture"\nemail="fixture@example.invalid"\n[signing]\nbehavior="drop"\n')
    tools = root / 'bin'
    tools.mkdir()
    forbidden = tools / 'nix'
    forbidden.write_text('#!' + sys.executable + '\nraise AssertionError("Nix must not run")\n')
    forbidden.chmod(0o700)
    env = {key: value for key, value in os.environ.items() if not key.startswith(('GIT_', 'JJ_', 'XDG_'))}
    env.update(HOME=str(home), XDG_CONFIG_HOME=str(home / '.config'), PATH=str(tools) + os.pathsep + os.environ['PATH'], GIT_CONFIG_NOSYSTEM='1', GIT_CONFIG_GLOBAL=os.devnull, GIT_AUTHOR_NAME='Fixture', GIT_AUTHOR_EMAIL='fixture@example.invalid', GIT_COMMITTER_NAME='Fixture', GIT_COMMITTER_EMAIL='fixture@example.invalid', GIT_CONFIG_COUNT='2', GIT_CONFIG_KEY_0='commit.gpgsign', GIT_CONFIG_VALUE_0='false', GIT_CONFIG_KEY_1='core.hooksPath', GIT_CONFIG_VALUE_1=str(tools / 'no-hooks'))
    def run(cwd, *args, success=True):
        result = subprocess.run(args, cwd=cwd, env=env, capture_output=True, timeout=30)
        if success:
            assert result.returncode == 0, (args, result.stdout.decode(), result.stderr.decode())
        return result
    def git(cwd, *args):
        return run(cwd, GIT, *args).stdout.decode().strip()
    def jj(cwd, *args):
        return run(cwd, JJ, *args).stdout.decode().strip()
    source = root / 'source'
    source.mkdir()
    git(source, 'init', '-b', 'main')
    (source / 'flake.lock').write_text('{"fixture":1}\n')
    (source / 'flake.nix').write_text('{ inputs = {}; outputs = _: {}; }\n')
    git(source, 'add', '.')
    git(source, 'commit', '-m', 'old shell')
    parent = root / 'parent'
    parent.mkdir()
    git(parent, 'init', '-b', 'main')
    git(parent, '-c', 'protocol.file.allow=always', 'submodule', 'add', str(source), 'seele-shell')
    for name in ('flake.lock', 'staged.txt', 'unstaged.txt'):
        (parent / name).write_text('original\n')
    git(parent, 'add', '.')
    git(parent, 'commit', '-m', 'parent baseline')
    baseline = git(parent, 'rev-parse', 'HEAD')
    jj(parent, 'git', 'init', '--colocate')
    jj(parent, 'new')
    jj(parent, 'bookmark', 'create', 'review', '-r', '@-')
    git(source, 'checkout', '-b', 'review-shell')
    (source / 'runtime.rs').write_text('fn main() {}\n')
    git(source, 'add', '.')
    git(source, 'commit', '-m', 'new shell')
    new = git(source, 'rev-parse', 'HEAD')
    child = parent / 'seele-shell'
    git(child, 'fetch', 'origin')
    git(child, 'checkout', '--detach', new)
    jj(child, 'git', 'init', '--colocate')
    (parent / 'staged.txt').write_text('staged change\n')
    git(parent, 'add', 'staged.txt')
    (parent / 'staged.txt').write_text('unstaged after staged\n')
    (parent / 'unstaged.txt').write_text('unstaged change\n')
    (parent / 'untracked.txt').write_text('untracked change\n')
    before_index = git(parent, 'show', ':staged.txt')
    before_refs = git(parent, 'for-each-ref', '--format=%(refname) %(objectname)', 'refs/heads')
    before_bookmarks = jj(parent, '--ignore-working-copy', 'bookmark', 'list')
    before_head = git(parent, 'rev-parse', 'HEAD')
    for staged in (False, True):
        (parent / 'flake.lock').write_text('changed lock\n')
        if staged:
            git(parent, 'add', 'flake.lock')
            (parent / 'flake.lock').write_text('original\n')
        rejected = run(parent, str(BINARY), '--pr', '--keep-lock', success=False)
        assert rejected.returncode == 1 and b'unchanged parent flake.lock' in rejected.stderr
        assert git(parent, 'rev-parse', 'HEAD') == before_head
        assert git(parent, 'for-each-ref', '--format=%(refname) %(objectname)', 'refs/heads') == before_refs
        (parent / 'flake.lock').write_text('original\n')
        git(parent, 'add', 'flake.lock')
    result = run(parent, str(BINARY), '--pr', '--keep-lock')
    assert b'No Nix evaluation' in result.stdout
    assert git(parent, 'for-each-ref', '--format=%(refname) %(objectname)', 'refs/heads') == before_refs
    assert git(parent, 'rev-parse', 'main') == baseline
    assert jj(parent, '--ignore-working-copy', 'bookmark', 'list') == before_bookmarks
    assert git(parent, 'rev-parse', 'HEAD:seele-shell') == new
    assert git(parent, 'diff-tree', '--no-commit-id', '--name-only', '-r', 'HEAD') == 'seele-shell'
    assert git(parent, 'show', 'HEAD:staged.txt') == 'original'
    assert git(parent, 'show', ':staged.txt') == before_index
    assert (parent / 'staged.txt').read_text() == 'unstaged after staged\n'
    assert (parent / 'unstaged.txt').read_text() == 'unstaged change\n'
    assert (parent / 'untracked.txt').read_text() == 'untracked change\n'
    assert git(parent, 'show', 'HEAD:flake.lock') == 'original'
    print('Private Git/JJ PR transaction preserves all bookmarks and unrelated staged, unstaged, and untracked changes; only the published gitlink is committed; Nix never runs')
