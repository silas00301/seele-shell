"""Real open-panel worker: cadence, projection, bounded data and EOF cleanup."""
import json
import os
import selectors
import subprocess
import sys
import tempfile
import time

with tempfile.TemporaryDirectory() as work:
    env = dict(os.environ, XDG_STATE_HOME=work + '/state', XDG_CACHE_HOME=work + '/cache')
    worker = subprocess.Popen([sys.argv[1]], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env)
    selector = selectors.DefaultSelector()
    selector.register(worker.stdout, selectors.EVENT_READ)
    pending = bytearray()
    def receive():
        until = time.monotonic() + 5
        while b'\n' not in pending:
            assert selector.select(max(0, until - time.monotonic())), 'worker response timed out'
            chunk = os.read(worker.stdout.fileno(), 65536)
            assert chunk, 'worker exited unexpectedly'
            pending.extend(chunk)
        line, _, rest = pending.partition(b'\n')
        pending[:] = rest
        value = json.loads(line)
        assert value['version'] == 1
        assert len(value['rows']) <= 256
        assert len(value['cpuHistory']) <= 60
        assert len(value['memoryHistory']) <= 60
        for row in value['rows']:
            assert set(row) == {'id','pid','name','state','threads','rss','virtualBytes','cpu'}
        return value
    def request(value):
        worker.stdin.write(json.dumps(value).encode() + b'\n')
        worker.stdin.flush()
        return receive()
    try:
        first = receive()
        assert first['cpu'] is None
        assert all(row['cpu'] is None for row in first['rows'])
        second = receive()
        assert second['cpu'] is None or 0 <= second['cpu'] <= 100
        assert len(second['cpuHistory']) == 2
        own = str(os.getpid())
        value = request({'op':'query','text':own})
        assert all(own in str(row['pid']) or own in row['name'].lower() for row in value['rows'])
        identity = next(row['id'] for row in value['rows'] if row['pid'] == os.getpid())
        selected = request({'op':'select','id':identity})
        assert selected['selected']['id'] == identity
        value = request({'op':'query','text':'no-such-resource-process'})
        assert value['rows'] == [] and value['selected']['id'] == identity
        value = request({'op':'select','id':identity + '0'})
        assert value['selected'] is None and value['selectionGone']
        request({'op':'query','text':''})
        value = request({'op':'sort','value':'memory'})
        rss = [row['rss'] for row in value['rows']]
        assert rss == sorted(rss, reverse=True)
        # Requests change presentation only and cannot create fake history points.
        length = len(value['cpuHistory'])
        for _ in range(4):
            value = request({'op':'sort','value':'cpu'})
        assert len(value['cpuHistory']) <= length + 1
        worker.stdin.close()
        assert worker.wait(timeout=3) == 0
        assert not worker.stderr.read()
        assert os.listdir(work) == []
    finally:
        if worker.poll() is None:
            worker.kill()
            worker.wait()
        selector.close()
print('resource worker protocol passed')
