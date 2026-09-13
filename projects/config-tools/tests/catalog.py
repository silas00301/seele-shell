"""Check platform filtering, discoverability, and the JSON interface."""

import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(sys.argv.pop(1)).resolve()
APPLICATIONS = [
    {"name": "linux-only", "binary": "linux-tool", "modules": ["desktop"], "systems": ["x86_64-linux"]},
    {"name": "jj", "binary": "jj", "modules": ["jujutsu", "bat"], "systems": ["x86_64-linux", "aarch64-darwin"]},
    {"name": "btm", "binary": "btm", "modules": ["bottom"], "systems": ["aarch64-darwin"]},
]


class CatalogTest(unittest.TestCase):
    def run_catalog(self, *args, applications=APPLICATIONS):
        with tempfile.TemporaryDirectory() as directory:
            manifest = Path(directory) / "manifest.json"
            manifest.write_text(json.dumps({"system": "aarch64-darwin", "applications": applications}))
            return subprocess.run(
                [str(SCRIPT), str(manifest), *args],
                capture_output=True, text=True,
            )

    def test_native_json_is_filtered_and_sorted(self):
        result = self.run_catalog("--json")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual([app["name"] for app in json.loads(result.stdout)], ["btm", "jj"])
        self.assertEqual(json.loads(result.stdout)[1]["modules"], ["jujutsu", "bat"])

    def test_all_systems_includes_other_platforms(self):
        result = self.run_catalog("--all-systems", "--json")
        self.assertEqual(len(json.loads(result.stdout)), 3)

    def test_detail_uses_output_name_and_exposes_executable(self):
        result = self.run_catalog("linux-only", "--all-systems")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("executable: linux-tool", result.stdout)
        self.assertIn("#linux-only", result.stdout)
        self.assertIn("desktop", result.stdout)

    def test_unknown_and_unavailable_commands_fail(self):
        for command in ["linux-only", "missing"]:
            result = self.run_catalog(command)
            self.assertEqual(result.returncode, 2)
            self.assertIn("--all-systems", result.stderr)

    def test_table_shows_command_feature_mapping(self):
        result = self.run_catalog()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("aarch64-darwin", result.stdout)
        self.assertIn("jujutsu, bat", result.stdout)
        self.assertNotIn("linux-only", result.stdout)

    def test_empty_catalog_and_help(self):
        result = self.run_catalog(applications=[])
        self.assertEqual(result.returncode, 0, result.stderr)
        result = self.run_catalog("--json", applications=[])
        self.assertEqual(json.loads(result.stdout), [])
        result = self.run_catalog("--help")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("--all-systems", result.stdout)


if __name__ == "__main__":
    unittest.main()
