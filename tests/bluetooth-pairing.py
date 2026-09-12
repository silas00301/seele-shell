#!/usr/bin/env python3
"""Private local fixture; never connects to Bluetooth or a live session."""
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile
import time

control = str(Path(sys.argv[1]).resolve())
with tempfile.TemporaryDirectory() as directory:
    root = Path(directory)
    runtime = root / 'runtime'
    state = runtime / 'seele-shell'
    state.mkdir(parents=True, mode=0o700)
    runtime.chmod(0o700)
    request = state / 'bluetooth-pairing.json'
    answer = state / 'bluetooth-pairing.answer'
    env = {**os.environ, 'XDG_RUNTIME_DIR': str(runtime), 'SEELE_CONTROL_NO_STATUS': '1'}
    token = '0123456789abcdef0123456789abcdef'
    def publish(kind='passkey'):
        request.unlink(missing_ok=True)
        request.write_text(json.dumps({'token': token, 'kind': kind, 'passkey': '123456'}))
        request.chmod(0o600)
    def call(verb, *args, data=None, success=True):
        result = subprocess.run([control, verb, *args], input=data, capture_output=True, env=env, timeout=8)
        assert (result.returncode == 0) == success, (verb, result.returncode, result.stderr)
        return result
    def respond(value='', verdict='accept', request_token=token, success=True):
        return call('bluetooth-pairing-answer-stdin', data=json.dumps({'token': request_token, 'verdict': verdict, 'value': value}).encode(), success=success)
    publish()
    shown = call('bluetooth-pairing-read', token)
    assert json.loads(shown.stdout)['passkey'] == '123456'
    call('bluetooth-pairing-read', 'bad-token', success=False)
    call('bluetooth-pairing-read', 'a' * 32, success=False)
    respond('000123')
    assert answer.read_text() == f'{token} accept 000123\n'
    assert stat.S_IMODE(answer.stat().st_mode) == 0o600
    original = answer.read_bytes()
    for value in ('', '1000000', '12x3', '123\n4', '１２３'):
        respond(value, success=False)
        assert answer.read_bytes() == original
    respond('123456', request_token='a' * 32, success=False)
    call('bluetooth-pairing-answer-stdin', data=b'x' * 4097, success=False)
    call('bluetooth-pairing-answer-stdin', data=b'{"token":', success=False)
    assert answer.read_bytes() == original
    publish('confirm')
    respond('123456', success=False)
    respond()
    respond(verdict='reject')
    publish('pincode')
    respond('long-enough-code')
    respond('x' * 17, success=False)
    respond('123\0', success=False)
    publish()
    request.chmod(0o644)
    call('bluetooth-pairing-read', token, success=False)
    publish()
    backup = state / 'held'
    request.rename(backup)
    request.symlink_to(backup)
    call('bluetooth-pairing-read', token, success=False)
    request.unlink()
    os.link(backup, request)
    call('bluetooth-pairing-read', token, success=False)
    request.unlink()
    os.mkfifo(request, 0o600)
    call('bluetooth-pairing-read', token, success=False)
    publish()
    request.write_bytes(b'x' * 4097)
    call('bluetooth-pairing-read', token, success=False)
    publish()
    child = subprocess.Popen([control, 'bluetooth-pairing-answer-stdin'], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env)
    try:
        child.stdin.write(b'{"value":"private pairing code"')
        child.stdin.flush()
        cmdline = Path(f'/proc/{child.pid}/cmdline').read_bytes()
        assert b'private pairing code' not in cmdline
        started = time.monotonic()
        assert child.wait(timeout=5) != 0
        assert time.monotonic() - started < 4
    finally:
        if child.poll() is None:
            child.kill()
        child.communicate()
print('Bluetooth private request, stale token, strict answer, file boundary and bounded stdin checks passed')
