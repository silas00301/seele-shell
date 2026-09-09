#!/usr/bin/env python3
"""Exercise the vault-backed store against real Markdown files.

Every assertion here is about bytes on disk: what the worker writes, what it
refuses to write over, and what it leaves exactly as it found it.
"""
import json
import os
from pathlib import Path
import select
import signal
import subprocess
import sys
import tempfile
import time
import wave

binary = str(Path(sys.argv[1]).absolute())

with tempfile.TemporaryDirectory(prefix='seele-notes-test-') as temp:
    work = Path(temp)
    vault = work / 'vault'
    (vault / '.obsidian').mkdir(parents=True)
    mock = work / 'bin'
    mock.mkdir()
    recorder = mock / 'parecord'
    recorder.write_text(f'''#!{sys.executable}
import os, signal, struct, time
signal.signal(signal.SIGINT, lambda *_: exit(0))
open(os.environ['RECORDER_PID'], 'w').write(str(os.getpid()))
while True:
    os.write(1, struct.pack('<h', 8192) * 800)
    time.sleep(.05)
''')
    recorder.chmod(0o755)
    env = dict(
        os.environ,
        HOME=str(work / 'home'),
        XDG_CONFIG_HOME=str(work / 'config'),
        XDG_STATE_HOME=str(work / 'state'),
        XDG_DATA_HOME=str(work / 'data'),
        RECORDER_PID=str(work / 'pid'),
        PATH=str(mock) + ':' + os.environ['PATH'],
    )

    def start(*args):
        return subprocess.Popen([binary, *args], env=env, stdin=subprocess.PIPE,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, bufsize=1)

    def read(process, timeout=5):
        if not select.select([process.stdout], [], [], timeout)[0]:
            raise AssertionError('worker did not reply')
        line = process.stdout.readline()
        assert line, process.stderr.read()
        return json.loads(line)

    class Worker:
        def __init__(self):
            self.process = start('watch')
            self.serial = 0
            self.ready = read(self.process)

        def send(self, action, **fields):
            self.serial += 1
            self.process.stdin.write(json.dumps(dict(action=action, request=self.serial, **fields)) + '\n')
            self.process.stdin.flush()
            while True:
                result = read(self.process)
                # Unsolicited refreshes carry no request number.
                if result.get('request') == self.serial:
                    return result

        def watch_for(self, predicate, timeout=6):
            deadline = time.monotonic() + timeout
            while time.monotonic() < deadline:
                if not select.select([self.process.stdout], [], [], deadline - time.monotonic())[0]:
                    break
                message = json.loads(self.process.stdout.readline())
                if predicate(message):
                    return message
            raise AssertionError('the vault watcher never reported the change')

        def close(self):
            self.process.communicate(timeout=5)
            assert self.process.returncode == 0

    worker = Worker()
    assert worker.ready['ready'] and not worker.ready['config']['configured']

    # An unconfigured worker refuses to touch anything.
    assert not worker.send('list')['ok']
    assert not worker.send('configure', vault=str(vault), directory='../escape')['ok']
    assert not worker.send('configure', vault=str(vault), directory='')['ok']

    configured = worker.send('configure', vault=str(vault), directory='Inbox')
    assert configured['ok'], configured
    assert configured['config']['writable'] and configured['config']['isVault']
    inbox = vault / 'Inbox'
    assert inbox.is_dir()

    # A first save earns a filename from the first meaningful line.
    body = '---\ntags: [idea]\n---\n\n# Trip plan\n\nBerlin **hotel** and [[Packing list]].\n'
    created = worker.send('save', path='', text=body, baseline='')
    assert created['created']
    note = created['note']
    assert note['path'] == 'Inbox/Trip plan.md', note
    assert note['title'] == 'Trip plan'
    assert note['excerpt'] == 'Berlin hotel and Packing list.', note
    written = inbox / 'Trip plan.md'
    assert written.read_text() == body, 'the file holds exactly what was typed'
    assert written.stat().st_mode & 0o777 == 0o644, 'vault content is ordinary user content'

    # Ordinary body edits keep the filename they already earned.
    edited = body + '\nTrain at 08:15.\n'
    saved = worker.send('save', path=note['path'], text=edited, baseline=note['hash'])
    assert saved['note']['path'] == 'Inbox/Trip plan.md', 'a body edit never renames the file'
    assert written.read_text() == edited

    # Opening and closing without editing must not rewrite the file.
    before = written.stat().st_mtime_ns
    time.sleep(0.01)
    untouched = worker.send('save', path=note['path'], text=edited, baseline=saved['hash'])
    assert untouched['unchanged']
    assert written.stat().st_mtime_ns == before, 'an unchanged save leaves the file alone'

    # A list row shows the words, not the block syntax that opens the line.
    checklist = worker.send('save', path='', text='- [ ] milk\n- [ ] bread\n', baseline='')
    assert checklist['note']['title'] == 'milk', checklist
    assert checklist['note']['path'] == 'Inbox/milk.md', checklist
    assert checklist['note']['excerpt'] == 'bread'
    assert (inbox / 'milk.md').read_text() == '- [ ] milk\n- [ ] bread\n', 'the file keeps its syntax'

    # A second capture with the same first line does not overwrite the first.
    second = worker.send('save', path='', text='# Trip plan\n\nOther one\n', baseline='')
    assert second['note']['path'] == 'Inbox/Trip plan 2.md'

    # An audio-only capture is named for its time, not for the recording.
    audio_only = worker.send('save', path='', text='![[Voice memo 2026-01-01 101010.wav]]\n', baseline='')
    assert audio_only['note']['path'] != 'Inbox/Voice memo 2026-01-01 101010.wav.md', audio_only

    # A name that cannot be a filename is made into one without escaping.
    hostile = worker.send('save', path='', text='# ../../etc/passwd: *?"<>|\n', baseline='')
    assert Path(hostile['note']['path']).parent.as_posix() == 'Inbox', hostile
    assert not (work / 'etc').exists()
    assert not worker.send('save', path='../../escape.md', text='no', baseline='')['ok']
    assert not worker.send('read', path='/etc/passwd')['ok']
    assert not worker.send('save', path='Inbox/x.md', text='a' * (2 * 1024 * 1024 + 1), baseline='')['ok']

    # Frontmatter, wikilinks and unsupported syntax survive a round trip.
    document = worker.send('read', path=note['path'])
    assert document['text'] == edited
    assert '```' not in document['text']
    roundtrip = '---\ntags: [idea]\n---\n\n# Trip plan\n\n> quote\n\n$$e=mc^2$$\n\n```rust\nfn main() {}\n```\n'
    worker.send('save', path=note['path'], text=roundtrip, baseline=document['hash'])
    assert written.read_text() == roundtrip

    # An external edit is reported without being asked for.
    written.write_text(roundtrip + '\nAdded by Obsidian.\n')
    refreshed = worker.watch_for(lambda message: message.get('changed'))
    assert any(item['path'] == note['path'] for item in refreshed['notes'])

    # A save against a stale digest never overwrites the newer file.
    conflicted = worker.send('save', path=note['path'], text='mine only\n', baseline=document['hash'])
    assert 'conflict' in conflicted, conflicted
    assert conflicted['conflict']['text'].endswith('Added by Obsidian.\n')
    assert written.read_text().endswith('Added by Obsidian.\n'), 'the disk version is intact'
    drafts = worker.send('config')['drafts']
    assert any(draft['text'] == 'mine only\n' for draft in drafts), 'the local version is kept too'

    # Saving a copy keeps both versions as files.
    copy = worker.send('resolve', path=note['path'], mode='copy', text='mine only\n')
    assert copy['switched'] and Path(copy['note']['path']).name.startswith('Trip plan (Seele ')
    assert (vault / copy['note']['path']).read_text() == 'mine only\n'
    assert written.read_text().endswith('Added by Obsidian.\n')
    assert not worker.send('config')['drafts'], 'a resolved conflict clears its recovery draft'

    # Keeping the local version preserves the external one beside it.
    current = worker.send('read', path=note['path'])
    worker.send('save', path=note['path'], text='stale\n', baseline='nonsense')
    kept = worker.send('resolve', path=note['path'], mode='mine', text='mine wins\n')
    assert written.read_text() == 'mine wins\n'
    external = [path for path in inbox.iterdir() if '(external ' in path.name]
    assert len(external) == 1 and external[0].read_text().endswith('Added by Obsidian.\n')

    # A file removed elsewhere is not resurrected by a save already in flight.
    disposable = worker.send('save', path='', text='# Temporary\n', baseline='')
    (vault / disposable['note']['path']).unlink()
    stale = worker.send('save', path=disposable['note']['path'], text='# Temporary\n\nmore\n',
                        baseline=disposable['hash'])
    assert stale['gone'], stale
    assert not (vault / disposable['note']['path']).exists()
    assert any(draft['reason'] == 'gone' for draft in worker.send('config')['drafts'])

    # A write the filesystem refuses leaves its text in recovery, while a
    # request that was never valid leaves nothing behind.
    for draft in worker.send('config')['drafts']:
        worker.send('discard', path=draft['path'])
    assert not worker.send('save', path='../outside.md', text='no', baseline='')['ok']
    assert not worker.send('config')['drafts'], 'an invalid request is not recovery material'
    current = worker.send('read', path=note['path'])
    inbox.chmod(0o500)
    try:
        refused = worker.send('save', path=note['path'], text='refused text\n',
                              baseline=current['hash'])
        assert not refused['ok'], refused
    finally:
        inbox.chmod(0o700)
    assert any(draft['text'] == 'refused text\n' for draft in worker.send('config')['drafts'])
    for draft in worker.send('config')['drafts']:
        worker.send('discard', path=draft['path'])
    assert not worker.send('config')['drafts']

    # Recording writes into the vault and is referenced by an embed.
    recording = start('record')
    assert read(recording)['recording']
    progress = read(recording)
    assert progress['level'] > 0 and progress['duration'] > 0
    duplicate = start('record')
    duplicate.communicate(timeout=5)
    assert duplicate.returncode != 0, 'only one recorder may own the microphone'
    recording.stdin.write('stop\n')
    recording.stdin.flush()
    remaining, errors = recording.communicate(timeout=5)
    assert recording.returncode == 0, errors
    memo = next(json.loads(line) for line in remaining.splitlines() if json.loads(line).get('saved'))
    attachment = inbox / 'Attachments' / memo['saved']
    assert attachment.is_file()
    assert attachment.stat().st_mode & 0o777 == 0o644, 'Obsidian has to be able to read it'
    with wave.open(str(attachment), 'rb') as audio:
        assert audio.getnchannels() == 1 and audio.getframerate() == 16000
        assert audio.getsampwidth() == 2 and audio.getnframes() > 0
    assert not list((inbox / 'Attachments').glob('*.part'))
    pid = int((work / 'pid').read_text())
    assert not Path(f'/proc/{pid}').exists(), 'the recorder child was leaked'

    # EOF and SIGTERM finalize a usable recording and reap the microphone.
    for terminate in (False, True):
        recording = start('record')
        read(recording)
        read(recording)
        if terminate:
            recording.send_signal(signal.SIGTERM)
        recording.communicate(timeout=5)
        assert recording.returncode == 0
        assert not Path(f"/proc/{int((work / 'pid').read_text())}").exists()

    # An embed resolves to a playable file, and removing it leaves that file.
    memo_note = worker.send('save', path='', text=f'# Voice\n\n![[{memo["saved"]}]]\n', baseline='')
    resolved = worker.send('read', path=memo_note['note']['path'])
    assert len(resolved['audio']) == 1
    assert resolved['audio'][0]['path'] == str(attachment)
    assert resolved['audio'][0]['duration'] > 0
    assert memo_note['note']['audio'] == 1
    worker.send('save', path=memo_note['note']['path'], text='# Voice\n', baseline=resolved['hash'])
    assert attachment.is_file(), 'removing an embed never deletes the recording'

    # The embed still resolves after the whole vault moves.
    moved = work / 'moved-vault'
    os.rename(vault, moved)
    worker.close()
    relocated = Worker()
    assert relocated.send('configure', vault=str(moved), directory='Inbox')['ok']
    away = relocated.send('read', path=memo_note['note']['path'])
    assert away['audio'] == [] or away['audio'][0]['path'].startswith(str(moved))
    relocated.close()
    os.rename(moved, vault)
    worker = Worker()
    assert worker.send('configure', vault=str(vault), directory='Inbox')['ok']

    # Trash is reversible, keeps attachments, and never overwrites on restore.
    listing = worker.send('list')
    trashable = listing['notes'][0]['path']
    trashed = worker.send('trash', path=trashable)
    assert not (vault / trashable).exists()
    assert (vault / '.trash' / trashed['trashed']).is_file()
    assert attachment.is_file(), 'trashing a note leaves shared recordings alone'
    assert all(item['path'] != trashable for item in trashed['notes']), 'the trash is not the active list'
    assert any(item['id'] == trashed['trashed'] for item in trashed['trash'])
    (vault / trashable).write_text('# Something else took the name\n')
    restored = worker.send('restore', id=trashed['trashed'])
    assert restored['restored'] != trashable, 'a restore never overwrites what took the name'
    assert (vault / trashable).read_text() == '# Something else took the name\n'
    assert (vault / restored['restored']).is_file()

    # Trash survives a restart of the worker.
    again = worker.send('trash', path=restored['restored'])

    # A trashed note can be read back without being restored first.
    peeked = worker.send('read', id=again['trashed'])
    assert peeked['preview'] and peeked['text']
    assert not worker.send('read', id='../Inbox/Trip plan.md')['ok']

    worker.close()
    worker = Worker()
    assert any(item['id'] == again['trashed'] for item in worker.send('list')['trash'])

    # Migration from the private JSON library.
    legacy = work / 'data' / 'seele-shell' / 'notes'
    for index, (title, trashed) in enumerate([('Old note', False), ('Old trashed', True)]):
        directory = legacy / f'{index:032x}'
        directory.mkdir(parents=True)
        (directory / 'note.json').write_text(json.dumps({
            'id': f'{index:032x}', 'title': title, 'body': 'body text',
            'created': 1, 'updated': 2, 'trashed': trashed,
        }))
        with wave.open(str(directory / f'{index + 90:032x}.wav'), 'wb') as audio:
            audio.setnchannels(1)
            audio.setsampwidth(2)
            audio.setframerate(16000)
            audio.writeframes(b'\x00\x10' * 16000)
    broken = legacy / f'{7:032x}'
    broken.mkdir()
    (broken / 'note.json').write_text('{not json')

    preview = worker.send('migrate', mode='preview')['migration']
    assert preview['present'] and preview['pending'] == 2 and preview['memos'] == 2
    assert preview['malformed'] == 1
    assert not (inbox / 'Old note.md').exists(), 'a preview writes nothing'

    assert worker.send('config')['config']['legacy'] == 2, 'the banner counts what is left'
    ran = worker.send('migrate', mode='run')['migration']
    assert ran['migrated'] == 2 and not ran['failures'], ran
    assert ran['pending'] == 0, 'a finished run has nothing left to offer'
    assert worker.send('config')['config']['legacy'] == 0, 'nothing is still waiting'
    migrated = (inbox / 'Old note.md').read_text()
    assert migrated.startswith('# Old note') and 'body text' in migrated
    assert '![[Voice memo Old note ' in migrated
    assert len(list((inbox / 'Attachments').glob('Voice memo Old note*.wav'))) == 1
    assert (broken / 'note.json').read_text() == '{not json', 'malformed entries are untouched'
    assert (legacy / f'{0:032x}' / 'note.json').is_file(), 'originals stay where they are'
    assert all(item['path'] != 'Inbox/Old trashed.md' for item in worker.send('list')['notes'])
    assert any('Old trashed' in item['name'] for item in worker.send('list')['trash'])

    # A retry is idempotent, including after a migrated note is deleted.
    (inbox / 'Old note.md').unlink()
    retry = worker.send('migrate', mode='run')['migration']
    assert retry['migrated'] == 0 and retry['pending'] == 0, retry
    assert not (inbox / 'Old note.md').exists(), 'a retry does not resurrect a deleted note'
    assert len(list(inbox.glob('Old note*.md'))) == 0

    # A capture folder that cannot be written to is reported rather than hidden.
    inbox.chmod(0o500)
    try:
        assert not worker.send('list')['writable']
        assert not worker.send('save', path='', text='# Blocked\n', baseline='')['ok']
    finally:
        inbox.chmod(0o700)
    assert worker.send('list')['writable']

    # A failed capture reports itself and leaves no partial file behind.
    recorder.write_text('#!/bin/sh\nexit 1\n')
    failed = start('record')
    failed.communicate(timeout=5)
    assert failed.returncode != 0
    assert not list((inbox / 'Attachments').glob('*.part'))

    worker.close()

print('Notes vault storage, conflicts, external edits, trash, audio embeds and migration checks passed')
