"""Exercise dispatch and failure handling without evaluating or activating a host."""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(sys.argv.pop(1)).resolve()


class CheckCommandTest(unittest.TestCase):
    def run_check(self, *args, system="x86_64-linux", failure="", root=True):
        with tempfile.TemporaryDirectory() as directory:
            work = Path(directory)
            (work / "bin").mkdir()
            if root:
                (work / "flake.nix").touch()
                (work / "modules/hosts").mkdir(parents=True)
            log = work / "calls.jsonl"
            fake = work / "bin/nix"
            fake.write_text(
                f"#!{sys.executable}\n"
                "import json, os, sys\n"
                "with open(os.environ['CHECK_TEST_LOG'], 'a') as log:\n"
                "    log.write(json.dumps(sys.argv[1:]) + '\\n')\n"
                "if 'builtins.currentSystem' in sys.argv:\n"
                "    print(os.environ['CHECK_TEST_SYSTEM'])\n"
                "if ' '.join(sys.argv[1:]).startswith(os.environ.get('CHECK_TEST_FAIL') or '!'):\n"
                "    sys.exit(42)\n"
            )
            fake.chmod(0o755)
            env = os.environ | {
                "PATH": str(work / "bin"),
                "CHECK_TEST_LOG": str(log),
                "CHECK_TEST_SYSTEM": system,
                "CHECK_TEST_FAIL": failure,
            }
            result = subprocess.run(
                [str(SCRIPT), *args],
                cwd=work, env=env, text=True, capture_output=True,
            )
            calls = [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []
            return result, calls

    def test_linux_default_formats_and_evaluates_without_build(self):
        result, calls = self.run_check()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual([call[:2] for call in calls[1:]], [
            ["fmt", "--no-write-lock-file"], ["flake", "show"],
            ["flake", "check"], ["eval", "--raw"],
        ])
        self.assertIn(".#nixosConfigurations.nerv.config.system.build.toplevel.drvPath", calls[-1])
        self.assertTrue(all("--no-write-lock-file" in call for call in calls[1:]))

    def test_darwin_build_uses_native_host_without_link(self):
        result, calls = self.run_check("--build", system="aarch64-darwin")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(calls[-1], ["build", ".#darwinConfigurations.asuka.system", "--no-link", "--no-write-lock-file"])

    def test_other_system_checks_portables_and_rejects_host_build(self):
        result, calls = self.run_check(system="aarch64-linux")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(calls[-1][:2], ["flake", "check"])
        result, calls = self.run_check("--build", system="aarch64-linux")
        self.assertEqual(result.returncode, 2)
        self.assertEqual(len(calls), 1)

    def test_failures_stop_before_subsequent_steps(self):
        for failure in ["fmt", "flake show", "flake check", "eval --raw", "build"]:
            with self.subTest(failure=failure):
                result, calls = self.run_check("--build", failure=failure)
                self.assertEqual(result.returncode, 42)
                self.assertTrue(" ".join(calls[-1]).startswith(failure))
                self.assertNotIn("validation completed", result.stdout)

    def test_help_and_invalid_arguments_do_not_run_nix(self):
        for argument, status in [("--help", 0), ("--wat", 2)]:
            result, calls = self.run_check(argument, root=False)
            self.assertEqual(result.returncode, status)
            self.assertEqual(calls, [])

    def test_wrong_directory_does_not_run_nix(self):
        result, calls = self.run_check(root=False)
        self.assertEqual(result.returncode, 2)
        self.assertEqual(calls, [])


if __name__ == "__main__":
    unittest.main()
