"""Black-box runtime tests; fixtures never contact GitHub or read an account."""
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest

BINARY = str(Path(sys.argv.pop(1)).resolve())

class GitHubProcessTests(unittest.TestCase):
    def setUp(self):
        self.root = tempfile.TemporaryDirectory()
        self.addCleanup(self.root.cleanup)
        self.path = Path(self.root.name)
        self.env = dict(os.environ, PATH=str(self.path), GH_DEBUG='api', SEELE_GITHUB_HOST='github.com')

    def stub(self, body):
        path = self.path / 'gh'
        path.write_text('#!' + sys.executable + '\n' + body)
        path.chmod(0o700)

    def run_worker(self):
        result = subprocess.run([BINARY], env=self.env, capture_output=True, timeout=5, check=True)
        self.assertEqual(result.stderr, b'')
        return json.loads(result.stdout)

    def test_snapshot_and_noninteractive_environment(self):
        self.stub('''import os,json,sys
assert os.environ['GH_PROMPT_DISABLED'] == '1'
assert os.environ['GH_PAGER'] == 'cat'
assert 'GH_DEBUG' not in os.environ
assert sys.argv[1:4] == ['api','--hostname','github.com']
if sys.argv[-1] == 'user':
 print(json.dumps({'login':'fixture'}))
else:
 assert 'graphql' in sys.argv
 assert 'reviews=is:pr is:open review-requested:fixture sort:updated-desc' in sys.argv
 print(json.dumps({'data':{'viewer':{'login':'fixture','pullRequests':{'nodes':[],'totalCount':0}},'search':{'nodes':[],'issueCount':0}}}))
''')
        result = self.run_worker()
        self.assertEqual(result['state'], 'ready')
        self.assertEqual(result['viewer'], 'fixture')
        self.assertEqual(result['authored'], [])
        self.assertTrue(result['updatedAt'].endswith('Z'))

    def test_errors_are_sanitized_and_output_is_bounded(self):
        self.stub("import sys\nprint('HTTP 401 PRIVATE_TOKEN',file=sys.stderr)\nsys.exit(1)\n")
        result = self.run_worker()
        self.assertEqual(result['state'], 'auth-required')
        self.assertNotIn('PRIVATE_TOKEN', json.dumps(result))
        self.stub("import os\nwhile True: os.write(1,b'x'*16384)\n")
        self.assertIn('more data', self.run_worker()['message'])
        self.stub("print('not json')\n")
        self.assertIn('unreadable', self.run_worker()['message'])

    def test_invalid_host_never_starts_gh(self):
        marker = self.path / 'started'
        self.stub('from pathlib import Path\nPath(' + repr(str(marker)) + ').touch()\n')
        self.env['SEELE_GITHUB_HOST'] = 'github.com/path'
        self.assertEqual(self.run_worker()['state'], 'error')
        self.assertFalse(marker.exists())

    def test_focus_reads_one_pull_and_rejects_a_bad_url_without_gh(self):
        self.stub('''import json,sys
assert sys.argv[1:4] == ['api','--hostname','github.com']
assert 'graphql' in sys.argv
query = next(arg for arg in sys.argv if arg.startswith('query='))
assert 'mutation' not in query and 'diff_hunk' not in query
assert 'statusCheckRollup' in query and 'pull/183' not in query
assert 'pullRequest(number: 183)' in query
print(json.dumps({'data':{'repository':{'pullRequest':{
 'number':183,'title':'Focus','url':'https://github.com/silas00301/seele/pull/183',
 'isDraft':False,'reviewDecision':'REVIEW_REQUIRED',
 'commits':{'nodes':[{'commit':{'statusCheckRollup':{'state':'FAILURE'}}}]},
 'reviews':{'nodes':[{'submittedAt':'2026-10-02T12:00:00Z','author':{'login':'ada'},'body':'Please split this'}]},
 'comments':{'nodes':[]}
}}}}))
''')
        result = subprocess.run([BINARY, 'focus', 'https://github.com/silas00301/seele/pull/183'],
                                env=self.env, capture_output=True, timeout=5, check=True)
        self.assertEqual(result.stderr, b'')
        payload = json.loads(result.stdout)
        self.assertEqual(payload['state'], 'ready')
        self.assertEqual(payload['checks'], 'FAILURE')
        self.assertEqual(payload['comment'], 'Please split this')
        self.assertEqual(payload['commentAuthor'], 'ada')
        marker = self.path / 'started'
        self.stub('from pathlib import Path\nPath(' + repr(str(marker)) + ').touch()\n')
        refused = subprocess.run([BINARY, 'focus', 'https://github.com/org/repo/pull/0'],
                                 env=self.env, capture_output=True, timeout=5, check=True)
        body = json.loads(refused.stdout)
        self.assertEqual(body['state'], 'error')
        self.assertFalse(marker.exists())
        self.assertNotIn('http', body['message'].lower())

    def test_shutdown_owns_and_reaps_gh(self):
        for number in (signal.SIGTERM, signal.SIGINT):
            with self.subTest(signal=number):
                marker = self.path / str(number)
                self.stub('import os,time\nfrom pathlib import Path\nPath(' + repr(str(marker)) +
                          ').write_text(str(os.getpid()))\ntime.sleep(30)\n')
                worker = subprocess.Popen([BINARY], env=self.env, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
                try:
                    deadline = time.monotonic() + 3
                    while not marker.exists() and time.monotonic() < deadline:
                        time.sleep(0.01)
                    self.assertTrue(marker.exists())
                    child = int(marker.read_text())
                    worker.send_signal(number)
                    stdout, stderr = worker.communicate(timeout=3)
                    self.assertEqual(worker.returncode, 128 + number)
                    self.assertEqual((stdout, stderr), (b'', b''))
                    with self.assertRaises(ProcessLookupError): os.kill(child, 0)
                finally:
                    if worker.poll() is None: worker.kill()
                    worker.communicate()

if __name__ == '__main__':
    unittest.main()
