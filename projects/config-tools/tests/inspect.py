import json
import os
import pathlib
import subprocess
import sys
import tempfile

binary = pathlib.Path(sys.argv[1]).resolve()
with tempfile.TemporaryDirectory() as directory:
    work = pathlib.Path(directory)
    checkout = work / "checkout"
    source = checkout / "modules" / "features" / "fish.nix"
    source.parent.mkdir(parents=True)
    source.write_text("{}")
    catalog = work / "catalog.json"
    row = {"kind": "setting", "key": "home.programs.fish.enable", "value": "enabled", "sources": ["/nix/store/fixture-source/modules/features/fish.nix"]}
    catalog.write_text(json.dumps({"version": 1, "sourceRoot": "/nix/store/fixture-source", "rows": [row]}))
    environment = dict(os.environ, SEELE_INSPECT_CATALOG=str(catalog))
    def run(*arguments, **extra):
        return subprocess.run([str(binary), *arguments], env=environment, capture_output=True, text=True, timeout=10, **extra)
    result = run("FISH", "enabled", "--json")
    assert result.returncode == 0, result.stderr
    assert json.loads(result.stdout) == [row]
    assert json.loads(run("fish", "disabled", "--json").stdout) == []
    marker = work / "editor-arguments.json"
    editor = work / "editor"
    editor.write_text("#!" + sys.executable + "\nimport json, pathlib, sys\npathlib.Path(" + repr(str(marker)) + ").write_text(json.dumps(sys.argv[1:]))\n")
    editor.chmod(0o700)
    environment["SEELE_INSPECT_EDITOR"] = str(editor)
    result = run("--open", row["key"], "--source", "1", "--repo", str(checkout))
    assert result.returncode == 0, result.stderr
    assert json.loads(marker.read_text()) == ["--", str(source)]
    marker.unlink()
    row["sources"] = ["/upstream/modules/features/fish.nix"]
    catalog.write_text(json.dumps({"version": 1, "sourceRoot": "/nix/store/fixture-source", "rows": [row]}))
    assert run("--open", row["key"], "--repo", str(checkout)).returncode != 0
    assert not marker.exists()
    catalog.write_text("{malformed")
    assert run("fish").returncode != 0
    catalog.write_bytes(b"x" * (16 * 1024 * 1024 + 1))
    assert run("fish").returncode != 0
print("configuration inspector executable search, literal editor arguments and source-root boundaries passed")
