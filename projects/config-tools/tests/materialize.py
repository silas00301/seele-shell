import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True

script = Path(sys.argv.pop(1)).resolve()
import os
os.umask(0o077)
class module:
    @staticmethod
    def materialize(source, destination):
        subprocess.run([str(script), str(source), str(destination)], check=True)


class MaterializeTest(unittest.TestCase):
    def test_refresh_preserves_user_state_and_removes_obsolete_managed_links(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            one, two, dest = [root / name for name in ("one", "two", "config")]
            one.mkdir()
            two.mkdir()
            for name in ("managed", "obsolete", "overridden"):
                (one / name).write_text("old")
            for name in ("managed", "overridden", "user-file"):
                (two / name).write_text("new")
            module.materialize(one, dest)
            (dest / "user-link").symlink_to("/user/owned/target")
            (dest / "user-file").write_text("credentials")
            (dest / "overridden").unlink()
            (dest / "overridden").write_text("application state")
            module.materialize(two, dest)
            self.assertEqual((dest / "managed").read_text(), "new")
            self.assertFalse((dest / "obsolete").is_symlink())
            self.assertEqual((dest / "user-link").readlink(), Path("/user/owned/target"))
            self.assertEqual((dest / "user-file").read_text(), "credentials")
            self.assertEqual((dest / "overridden").read_text(), "application state")

    def test_concurrent_launches_and_directory_symlinks(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source, target, dest = [root / name for name in ("source", "target", "dest")]
            source.mkdir()
            target.mkdir()
            for i in range(50):
                (target / str(i)).write_text(str(i))
            (source / "nested").symlink_to(target, target_is_directory=True)
            processes = [subprocess.Popen([str(script), str(source), str(dest)]) for _ in range(4)]
            for process in processes:
                self.assertEqual(process.wait(), 0)
            self.assertFalse((dest / "nested").is_symlink())
            for i in range(50):
                self.assertEqual((dest / "nested" / str(i)).read_text(), str(i))
            self.assertEqual(len(json.loads((dest / ".seele-manifest.json").read_text())["links"]), 50)

    def test_user_directory_symlink_is_not_traversed(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source, dest, outside = [root / name for name in ("source", "dest", "outside")]
            (source / "app").mkdir(parents=True)
            dest.mkdir()
            outside.mkdir()
            (source / "app" / "config").write_text("managed")
            (dest / "app").symlink_to(outside, target_is_directory=True)
            module.materialize(source, dest)
            self.assertFalse((outside / "config").exists())


unittest.main()
