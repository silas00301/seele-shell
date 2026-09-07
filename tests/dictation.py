#!/usr/bin/env python3
"""Validate fragmented native Voxtype frames, reconnect, and stdin cleanup."""
import json
import os
from pathlib import Path
import select
import socket
import struct
import subprocess
import sys
import tempfile

with tempfile.TemporaryDirectory(prefix='seele-dictation-test-') as temp:
    runtime = Path(temp)
    (runtime / 'voxtype').mkdir()
    listener = socket.socket(socket.AF_UNIX)
    listener.bind(str(runtime / 'voxtype/audio.sock'))
    listener.listen()
    listener.settimeout(5)
    worker = subprocess.Popen([sys.argv[1]], env=dict(os.environ, XDG_RUNTIME_DIR=temp), stdin=subprocess.PIPE, stdout=subprocess.PIPE)
    def read():
        assert select.select([worker.stdout], [], [], 5)[0], 'level worker did not reply'
        return float(worker.stdout.readline())
    client, _ = listener.accept()
    frames = b''.join(struct.pack('@Ifff', i, -.25, .1, -12) for i in range(5))
    for offset in range(0, len(frames), 3):
        client.sendall(frames[offset:offset+3])
    assert read() == .5
    client.close()
    assert read() == 0
    client, _ = listener.accept()
    client.sendall(struct.pack('@Ifff', 0, float('nan'), .8, -4) * 5)
    assert read() == 0, 'invalid samples must not become NaN geometry'
    client.sendall(struct.pack('@Ifff', 1, -1, 1, 0) * 5)
    assert read() == 1
    worker.stdin.close()
    assert worker.wait(timeout=5) == 0, 'EOF must stop even an idle socket reader'
    client.close()
    listener.close()
print('Dictation framing, reconnect, invalid samples, and cleanup checks passed')
