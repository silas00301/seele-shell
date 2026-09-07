#!/usr/bin/env python3
"""Exercise production storage and audio lifecycle with a synthetic microphone."""
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

binary = str(Path(sys.argv[1]).resolve())
# Keep the dispatching symlink's basename when resolving its parent.
binary = str(Path(sys.argv[1]).absolute())

with tempfile.TemporaryDirectory(prefix='seele-notes-test-') as temp:
    work = Path(temp)
    mock = work / 'bin'
    mock.mkdir()
    recorder = mock / 'parecord'
    recorder.write_text('''#!/usr/bin/env python3
import os, signal, struct, time
signal.signal(signal.SIGINT, lambda *_: exit(0))
open(os.environ['RECORDER_PID'], 'w').write(str(os.getpid()))
while True:
    os.write(1, struct.pack('<h', 8192) * 800)
    time.sleep(.05)
''')
    recorder.chmod(0o755)
    env = dict(os.environ, XDG_DATA_HOME=str(work / 'data'), RECORDER_PID=str(work / 'pid'), PATH=str(mock) + ':' + os.environ['PATH'])

    def start(*args):
        return subprocess.Popen([binary, *args], env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, bufsize=1)

    def read(process):
        # Use text buffering only after select indicates the next line; the
        # protocol writes one reply at a time except recording progress.
        if not select.select([process.stdout], [], [], 5)[0]:
            raise AssertionError('worker did not reply')
        line = process.stdout.readline()
        assert line, process.stderr.read()
        return json.loads(line)

    worker = start('watch')
    assert read(worker)['notes'] == []
    serial = 0
    def send(action, **fields):
        global serial
        serial += 1
        worker.stdin.write(json.dumps(dict(action=action, request=serial, **fields)) + '\n')
        worker.stdin.flush()
        result = read(worker)
        assert result['request'] == serial
        return result

    note = send('create')['note']
    note_id = note['id']
    directory = work / 'data/seele-shell/notes' / note_id
    content = 'Quotes " and newlines\nGrüße 🎙️\n$(touch should-not-exist)'
    saved = send('save', id=note_id, title='Voice notes', body=content)['note']
    assert saved['body'] == content
    assert directory.stat().st_mode & 0o777 == 0o700
    assert (directory / 'note.json').stat().st_mode & 0o777 == 0o600
    assert not send('save', id='../../escape', title='Bad', body='Bad')['ok']
    assert not send('save', id=note_id, title='Bad', body='a' * (2 * 1024 * 1024 + 1))['ok']
    assert send('list')['notes'][0]['body'] == content

    recording = start('record', note_id)
    assert read(recording)['recording']
    progress = read(recording)
    assert progress['level'] > 0 and progress['duration'] > 0
    assert not send('trash', id=note_id)['ok'], 'a recording note cannot be trashed'
    duplicate = start('record', note_id)
    duplicate.communicate(timeout=5)
    assert duplicate.returncode != 0
    recording.stdin.write('stop\n')
    recording.stdin.flush()
    remaining, errors = recording.communicate(timeout=5)
    assert recording.returncode == 0, errors
    assert any(json.loads(line).get('saved') for line in remaining.splitlines())
    memo = send('list')['notes'][0]['memos'][0]
    audio = Path(memo['path'])
    assert audio.stat().st_mode & 0o777 == 0o600
    with wave.open(str(audio), 'rb') as wav:
        assert wav.getnchannels() == 1 and wav.getframerate() == 16000
        assert wav.getsampwidth() == 2 and wav.getnframes() > 0
    assert not list(directory.glob('*.part'))
    pid = int((work / 'pid').read_text())
    assert not Path(f'/proc/{pid}').exists(), 'recorder child was leaked'

    # EOF and SIGTERM must finalize a usable memo and reap the microphone.
    for terminate in (False, True):
        recording = start('record', note_id)
        read(recording)
        read(recording)
        if terminate:
            recording.send_signal(signal.SIGTERM)
        recording.communicate(timeout=5)
        assert recording.returncode == 0
        pid = int((work / 'pid').read_text())
        assert not Path(f'/proc/{pid}').exists()
    assert len(send('list')['notes'][0]['memos']) == 3

    assert send('trash', id=note_id)['note']['trashed']
    assert not send('save', id=note_id, title='No', body='No')['ok']
    assert send('restore', id=note_id)['note']['body'] == content
    worker.communicate(timeout=5)
    assert worker.returncode == 0
    worker = start('watch')
    restored = read(worker)['notes'][0]
    assert restored['body'] == content and len(restored['memos']) == 3
    worker.communicate(timeout=5)

    # A failed capture produces an explicit failure and no empty memo.
    recorder.write_text('#!/bin/sh\nexit 1\n')
    failed = start('record', note_id)
    failed.communicate(timeout=5)
    assert failed.returncode != 0
    assert len(list(directory.glob('*.wav'))) == 3
    assert not list(directory.glob('*.part'))

print('Notes persistence, permissions, trash, recording, and cleanup checks passed')
