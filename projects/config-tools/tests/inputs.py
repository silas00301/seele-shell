"""Fixture and CLI coverage for offline flake input reporting."""

import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

HELPER = Path(sys.argv.pop(1)).resolve()
class report:
    LockError = ValueError
    class LockGraph:
        def __init__(self, data):
            self.data = data
            self.report()  # executable validates the entire graph
        def report(self, include_all=False, selected=None):
            with tempfile.TemporaryDirectory() as directory:
                lock = Path(directory) / "flake.lock"
                lock.write_text(json.dumps(self.data))
                command = [str(HELPER), "--lock-file", str(lock), "--json"]
                if include_all: command.append("--all")
                if selected is not None: command.append(selected)
                result = subprocess.run(command, capture_output=True, text=True, timeout=10)
                if result.returncode: raise ValueError(result.stderr)
                return json.loads(result.stdout)
    @staticmethod
    def render(rows):
        # Display behavior is asserted against the CLI in dedicated cases.
        return json.dumps(rows)


def fixture():
    return {"version": 7, "root": "root-node", "nodes": {
        "root-node": {"inputs": {"pkg": "packages", "tool": "tool-node", "tar": "tar-node", "shell": "shell-node"}},
        "packages": {"locked": {"type": "github", "owner": "NixOS", "repo": "nixpkgs", "rev": "abc123", "lastModified": 0, "narHash": "HASH-MUST-STAY-HIDDEN"}},
        "tool-node": {"inputs": {"nixpkgs": ["pkg"], "nested": "nested-node"}, "locked": {"type": "github", "owner": "org", "repo": "tool"}},
        "nested-node": {"inputs": {"nixpkgs": ["tool", "nixpkgs"]}, "locked": {"type": "git", "url": "ssh://git:SECRET@host.example:2222/project?token=TOKEN#FRAGMENT"}},
        "tar-node": {"flake": False, "locked": {"type": "tarball", "url": "https://user:PASS@host.example/src.tar.gz?signature=SIGN#FRAG", "lastModified": 1725667200}},
        "shell-node": {"inputs": {"nixpkgs": ["pkg"]}, "locked": {"type": "path", "path": "./shell"}},
    }}


class InputReportTests(unittest.TestCase):
    def test_direct_rows_metadata_dates_and_sorting(self):
        rows = report.LockGraph(fixture()).report()
        self.assertEqual([row["input"] for row in rows], ["pkg", "shell", "tar", "tool"])
        self.assertEqual(rows[0]["revision"], "abc123")
        self.assertEqual(rows[0]["last_modified"], "1970-01-01T00:00:00Z")
        self.assertEqual(rows[1]["source"], "path:./shell")
        self.assertIsNone(rows[1]["revision"])
        self.assertEqual(rows[2]["last_modified"], "2024-09-07T00:00:00Z")
        self.assertIsNone(rows[2]["revision"])
        output = json.dumps(rows)
        for secret in ["HASH-MUST", "PASS", "SIGN", "FRAG", "user"]:
            self.assertNotIn(secret, output)

    def test_nested_follows_are_root_relative(self):
        graph = report.LockGraph(fixture())
        row = graph.report(selected="tool/nested/nixpkgs")[0]
        self.assertEqual(row["node"], "packages")
        self.assertEqual(row["follows"], "tool/nixpkgs")
        self.assertEqual(row["revision"], "abc123")
        paths = [row["input"] for row in graph.report(True, "tool")]
        self.assertEqual(paths, ["tool", "tool/nested", "tool/nested/nixpkgs", "tool/nixpkgs"])

    def test_shared_graph_expands_each_node_once(self):
        data = {"version": 7, "root": "root", "nodes": {"root": {"inputs": {"start": "n0"}}}}
        for index in range(40):
            node = {"locked": {"type": "path", "path": f"./n{index}"}}
            if index < 39:
                node["inputs"] = {"a": f"n{index + 1}", "b": f"n{index + 1}"}
            data["nodes"][f"n{index}"] = node
        rows = report.LockGraph(data).report(True)
        self.assertEqual(len(rows), 79)
        self.assertEqual(sum(bool(row["shared_with"]) for row in rows), 39)

    def test_legal_dependency_cycle_and_empty_follows_are_reported(self):
        data = fixture()
        data["nodes"]["tool-node"]["inputs"]["parent"] = []
        rows = report.LockGraph(data).report(True)
        parent = next(row for row in rows if row["input"] == "tool/parent")
        self.assertEqual(parent["type"], "root")
        self.assertEqual(parent["follows"], "")
        self.assertTrue(parent["cycle"])
        self.assertEqual(parent["shared_with"], "<root>")
        self.assertIn("<root>", report.render(rows))

    def test_cycle_across_shared_root_inputs_is_identified(self):
        data = fixture()
        data["nodes"]["root-node"]["inputs"]["nested"] = "nested-node"
        data["nodes"]["nested-node"]["inputs"]["tool"] = "tool-node"
        rows = report.LockGraph(data).report(True)
        cycle = [row for row in rows if row["cycle"]]
        self.assertTrue(cycle)
        self.assertTrue(any(row["input"] in ("tool/nested", "nested/tool") for row in cycle))

    def test_cyclic_follows_is_an_error(self):
        data = fixture()
        data["nodes"]["root-node"]["inputs"].update({"a": ["b"], "b": ["a"]})
        with self.assertRaisesRegex(report.LockError, "cyclic follows"):
            report.LockGraph(data).report()

    def test_deep_follows_needs_no_python_recursion(self):
        data = fixture()
        inputs = data["nodes"]["root-node"]["inputs"]
        for index in range(1500):
            inputs[f"alias{index}"] = [f"alias{index + 1}"] if index < 1499 else ["pkg"]
        self.assertEqual(report.LockGraph(data).report(selected="alias0")[0]["node"], "packages")

    def test_missing_nodes_and_paths_fail_clearly(self):
        data = fixture()
        data["nodes"]["root-node"]["inputs"]["oops"] = "missing-node"
        with self.assertRaisesRegex(report.LockError, "missing node"):
            report.LockGraph(data)
        for path in ["missing", "tool/missing", "", "tool//nixpkgs"]:
            with self.subTest(path=path), self.assertRaises(report.LockError):
                report.LockGraph(fixture()).report(selected=path)

    def test_bad_schemas_and_dates_are_rejected(self):
        cases = [[], {}, {"version": True}, {"version": 8}]
        for mutate in [
            lambda d: d.update(root="missing"),
            lambda d: d["nodes"]["tool-node"].update(inputs=[]),
            lambda d: d["nodes"]["tool-node"]["inputs"].update(bad=[None]),
            lambda d: d["nodes"]["packages"]["locked"].update(url={}),
            lambda d: d["nodes"]["packages"]["locked"].update(lastModified=True),
            lambda d: d["nodes"]["packages"]["locked"].update(lastModified=-1),
            lambda d: d["nodes"]["packages"]["locked"].update(lastModified=10**30),
        ]:
            data = fixture()
            mutate(data)
            cases.append(data)
        for data in cases:
            with self.subTest(data=data), self.assertRaises(report.LockError):
                report.LockGraph(data)

    def test_url_redaction(self):
        rows = report.LockGraph(fixture()).report(True)
        for output in [report.render(rows), json.dumps(rows)]:
            for secret in ["SECRET", "TOKEN", "FRAGMENT", "HASH-MUST-STAY-HIDDEN"]:
                self.assertNotIn(secret, output)

    def test_empty_lock_and_terminal_controls(self):
        graph = report.LockGraph({"version": 7, "root": "r", "nodes": {"r": {}}})
        self.assertEqual(graph.report(True), [])

    def test_cli_flags_json_errors_and_file_remains_unchanged(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            lock = root / "flake.lock"
            original = json.dumps(fixture())
            lock.write_text(original)
            env = dict(os.environ, PATH="", TZ="Pacific/Honolulu", PYTHONDONTWRITEBYTECODE="1")
            run = lambda *args: subprocess.run([str(HELPER), *args], cwd=root, env=env, text=True, capture_output=True)
            help_result = run("--help")
            self.assertIn("usage: seele-inputs", help_result.stdout)
            default = run("--json")
            self.assertEqual(default.returncode, 0, default.stderr)
            self.assertEqual(len(json.loads(default.stdout)), 4)
            selected = run("--lock-file", str(lock), "--json", "shell/nixpkgs")
            self.assertEqual(json.loads(selected.stdout)[0]["node"], "packages")
            all_rows = run("--all", "--json")
            self.assertGreater(len(json.loads(all_rows.stdout)), 4)
            for args in [("--lock-file", "missing"), ("missing",), ("--unknown",)]:
                result = run(*args)
                self.assertEqual(result.returncode, 2)
                self.assertNotIn("Traceback", result.stderr)
            self.assertEqual(lock.read_text(), original)
            lock.write_text("invalid json")
            result = run()
            self.assertEqual(result.returncode, 2)
            self.assertNotIn("Traceback", result.stderr)
            lock.write_text(original)
            self.assertEqual(lock.read_text(), original)


if __name__ == "__main__":
    unittest.main()
