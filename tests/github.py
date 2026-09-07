"""GitHub fixtures and bounded mocked-gh process checks; never contacts an account."""
import importlib.util
import json
import os
import signal
import subprocess
from pathlib import Path
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

source = Path(sys.argv.pop(1)).resolve()
spec = importlib.util.spec_from_file_location("github_status", source)
github = importlib.util.module_from_spec(spec)
spec.loader.exec_module(github)


def pull(number=1, state="SUCCESS"):
    return {"number": number, "title": "<b>Fixture</b>\nchange", "url": f"https://github.com/org/repo/pull/{number}",
            "repository": {"nameWithOwner": "org/repo"}, "reviewDecision": "APPROVED", "isDraft": False,
            "updatedAt": "2026-09-08T00:00:00Z", "commits": {"nodes": [{"commit": {"statusCheckRollup": {"state": state}}}]}}


def fixture():
    return {"data": {"viewer": {"login": "fixture-user", "pullRequests": {"nodes": [pull()], "totalCount": 1}},
                     "search": {"nodes": [pull(2, "FAILURE")], "issueCount": 1}}}


class GitHubTests(unittest.TestCase):
    def test_snapshot_is_two_read_only_requests_with_bounded_query(self):
        calls = []
        def run(args):
            calls.append(args)
            return {"login": "fixture-user"} if len(calls) == 1 else fixture()
        result = github.snapshot(run)
        self.assertEqual(result["state"], "ready")
        self.assertEqual(result["authored"][0]["checks"], "SUCCESS")
        self.assertEqual(result["reviews"][0]["checks"], "FAILURE")
        self.assertNotIn("\n", result["authored"][0]["title"])
        self.assertEqual(len(calls), 2)
        self.assertEqual(calls[0], ["api", "--hostname", "github.com", "user"])
        self.assertIn("reviews=is:pr is:open review-requested:fixture-user sort:updated-desc", calls[1])
        self.assertNotIn("mutation", github.QUERY.lower())
        self.assertIn("first: 20", github.QUERY)

    def test_rows_limit_dedup_draft_and_unknown_checks(self):
        records = [pull(), pull()] + [pull(i) for i in range(2, 50)]
        records[0]["isDraft"] = True
        records[0]["commits"] = {"nodes": [{"commit": {"statusCheckRollup": None}}]}
        entries, total = github.connection({"nodes": records, "totalCount": 80}, "github.com", "totalCount")
        self.assertLessEqual(len(entries), 20)
        self.assertEqual(total, 80)
        self.assertTrue(entries[0]["draft"])
        self.assertEqual(entries[0]["checks"], "UNKNOWN")
        self.assertEqual(len({p["url"] for p in entries}), len(entries))

    def test_urls_are_canonical_https_on_selected_host(self):
        valid = "https://github.com/org/repo/pull/1"
        self.assertEqual(github.safe_url(valid, "github.com"), valid)
        for url in ["javascript:alert(1)", "https://evil.example/org/repo/pull/1", "https://github.com.evil/org/repo/pull/1", "https://user@github.com/org/repo/pull/1", valid + "?token=secret", valid + "#note", "https://github.com/org/repo/issues/1", "https://github.com:443/org/repo/pull/1"]:
            self.assertEqual(github.safe_url(url, "github.com"), "", url)
        self.assertEqual(github.safe_url("https://git.example/org/repo/pull/1", "git.example"), "https://git.example/org/repo/pull/1")

    def test_auth_rate_limit_and_graphql_errors_are_sanitized(self):
        for raw, expected in [("HTTP 401 secret", "auth-required"), ("API rate limit exceeded secret", "rate-limited"), ("connection failed secret", "error")]:
            error = github.failure(raw)
            self.assertEqual(error.state, expected)
            self.assertNotIn("secret", error.message)
        responses = iter([{"login": "fixture-user"}, {"errors": [{"message": "rate limit exceeded SECRET"}]}])
        with self.assertRaises(github.FetchError) as result:
            github.snapshot(lambda _: next(responses))
        self.assertEqual(result.exception.state, "rate-limited")
        self.assertNotIn("SECRET", result.exception.message)

    def test_account_changes_and_invalid_host_fail(self):
        data = fixture(); data["data"]["viewer"]["login"] = "other"
        responses = iter([{"login": "fixture-user"}, data])
        with self.assertRaises(github.FetchError): github.snapshot(lambda _: next(responses))
        for host in ["https://github.com", "github.com/path", "user@github.com", "bad..host"]:
            with self.assertRaises(github.FetchError): github.hostname(host)

    def test_shell_shutdown_kills_and_reaps_cli(self):
        for stop_signal in (signal.SIGTERM, signal.SIGINT):
            with self.subTest(signal=stop_signal), tempfile.TemporaryDirectory() as directory:
                pid_file = Path(directory) / "gh.pid"
                binary = Path(directory) / "gh"
                binary.write_text(f"#!{sys.executable}\nimport os,time\nfrom pathlib import Path\n"
                                  "Path(os.environ['SEELE_GITHUB_TEST_PID']).write_text(str(os.getpid()))\n"
                                  "time.sleep(30)\n")
                binary.chmod(0o755)
                environment = dict(os.environ, PATH=directory, SEELE_GITHUB_TEST_PID=str(pid_file))
                backend = subprocess.Popen([sys.executable, str(source)], env=environment,
                                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                child = None
                try:
                    deadline = time.monotonic() + 3
                    while time.monotonic() < deadline:
                        try:
                            value = pid_file.read_text()
                            if value.isdigit():
                                child = int(value)
                                break
                        except FileNotFoundError:
                            pass
                        time.sleep(0.01)
                    self.assertIsNotNone(child, "mock CLI must have started")
                    backend.send_signal(stop_signal)
                    self.assertEqual(backend.wait(timeout=3), 128 + stop_signal)
                    with self.assertRaises(ProcessLookupError):
                        os.kill(child, 0)
                    child = None
                finally:
                    if backend.poll() is None:
                        backend.kill()
                    backend.wait()
                    if child is not None:
                        try:
                            os.killpg(child, signal.SIGKILL)
                        except ProcessLookupError:
                            pass

    def test_mocked_cli_has_size_timeout_and_noninteractive_bounds(self):
        with tempfile.TemporaryDirectory() as directory:
            binary = Path(directory) / "gh"
            def stub(body):
                binary.write_text(f"#!{sys.executable}\n" + body)
                binary.chmod(0o755)
            with patch.dict(os.environ, {"PATH": directory, "GH_DEBUG": "api"}):
                stub("import os,json\nassert os.environ['GH_PROMPT_DISABLED']=='1'\nassert 'GH_DEBUG' not in os.environ\nprint(json.dumps({'login':'fixture'}))\n")
                self.assertEqual(github.run_gh(["api", "user"]), {"login": "fixture"})
                stub("print('x'*10000)\n")
                with self.assertRaises(github.FetchError) as error: github.run_gh([], max_output=128)
                self.assertIn("more data", error.exception.message)
                stub("import time\ntime.sleep(30)\n")
                start = time.monotonic()
                with self.assertRaises(github.FetchError): github.run_gh([], timeout=0.05)
                self.assertLess(time.monotonic() - start, 2)
                stub("import sys\nprint('HTTP 401 TOKEN',file=sys.stderr)\nsys.exit(1)\n")
                with self.assertRaises(github.FetchError) as error: github.run_gh([])
                self.assertEqual(error.exception.state, "auth-required")
                self.assertNotIn("TOKEN", error.exception.message)


if __name__ == "__main__":
    unittest.main()
