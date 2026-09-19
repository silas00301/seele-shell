#!/usr/bin/env python3
"""Exercise the real CLI with private config/state and fake desktop tools."""
import concurrent.futures
import copy
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

binary = str(Path(sys.argv[1]).resolve())
with tempfile.TemporaryDirectory(prefix="seele-themes-") as temporary:
    root = Path(temporary)
    config = root / "config with spaces"
    state_home = root / "state with spaces"
    state = state_home / "seele-theme"
    catalog_file = config / "seele-theme/catalog.json"
    catalog_file.parent.mkdir(parents=True)
    env = {"PATH": os.environ.get("PATH", ""), "HOME": str(root), "XDG_CONFIG_HOME": str(config), "XDG_STATE_HOME": str(state_home)}
    theme = dict(id="catppuccin-mocha", name="Catppuccin Mocha", flavor="mocha",
                 base="#1e1e2e", mantle="#181825", crust="#11111b", surface="#313244", overlay="#6c7086",
                 text="#cdd6f4", subtext="#a6adc8", accent="#b4befe", red="#f38ba8", green="#a6e3a1", yellow="#f9e2af",
                 terminal=["#123456"] * 16)
    light = dict(theme, id="catppuccin-latte", name="Catppuccin Latte", flavor="latte", base="#eff1f5", accent="#7287fd")
    catalog = dict(version=1, default=theme["id"], fontFamily="Maple Mono NF CN", wallpaper="/test/background.jpg", themes=[theme, light], commands={})
    def save(value=catalog):
        catalog_file.write_text(json.dumps(value))
    def call(*args, ok=True, environment=env):
        result = subprocess.run([binary, *args], env=environment, capture_output=True, text=True, timeout=15)
        assert (result.returncode == 0) == ok, (args, result.stdout, result.stderr)
        return json.loads(result.stdout) if ok else result
    save()
    assert call("list")["current"] == theme["id"]
    assert not state.exists(), "Listing must not initialize state"
    call("set", "../../escape", ok=False)
    assert not state.exists()
    call("init")
    assert (state / "selection.json").stat().st_mode & 0o777 == 0o600
    assert (state / "current").is_symlink()
    assert call("current")["id"] == theme["id"]
    unrelated = config / "ghostty/config"
    unrelated.parent.mkdir()
    unrelated.write_text("font-size = 13\n")
    # Publication must reach every include without writing app-owned settings.
    call("set", light["id"])
    selection = json.loads((state / "selection.json").read_text())
    assert selection["base"] == light["base"] and selection["fontFamily"] == catalog["fontFamily"]
    assert light["base"] in (state / "current/ghostty").read_text()
    assert light["accent"][1:] in (state / "current/fish.fish").read_text()
    assert light["accent"][1:] in (state / "current/hyprland.lua").read_text()
    assert light["base"] in (state / "current/tmux.conf").read_text()
    assert light["base"] in (state / "current/gtk.css").read_text()
    assert unrelated.read_text() == "font-size = 13\n"
    call("init")
    assert call("current")["id"] == light["id"], "Activation must preserve selection"
    assert len(list(state.glob(".theme-*"))) == 1
    # Concurrent pickers and activations serialize the entire publication.
    def change(i):
        return call("init") if i % 3 == 0 else call("set", catalog["themes"][i % 2]["id"])
    with concurrent.futures.ThreadPoolExecutor(max_workers=6) as pool:
        list(pool.map(change, range(18)))
    final = json.loads((state / "selection.json").read_text())
    assert final["base"] in (state / "current/ghostty").read_text()
    assert len(list(state.glob(".theme-*"))) == 1
    # Invalid data and unknown IDs cannot change the committed generation.
    before = (state / "selection.json").read_bytes()
    old_link = (state / "current").readlink()
    invalid = copy.deepcopy(catalog)
    invalid["themes"][0]["accent"] = "#fff; exec hostile"
    save(invalid)
    call("set", theme["id"], ok=False)
    assert (state / "selection.json").read_bytes() == before
    assert (state / "current").readlink() == old_link
    save()
    # If durable selection cannot publish, restore the old include target.
    (state / "selection.json").unlink()
    (state / "selection.json").mkdir()
    call("set", theme["id"], ok=False)
    assert (state / "current").readlink() == old_link
    (state / "selection.json").rmdir()
    (state / "selection.json").write_bytes(before)
    (state / "selection.json").chmod(0o600)
    # Never follow an unexpected current target or a private-state symlink.
    (state / "current").unlink()
    (state / "current").symlink_to(unrelated.parent)
    call("set", theme["id"], ok=False)
    assert unrelated.read_text() == "font-size = 13\n"
    (state / "current").unlink()
    (state / "current").symlink_to(old_link)
    (state / "selection.json").unlink()
    (state / "selection.json").symlink_to(unrelated)
    call("current", ok=False)
    call("reset")
    assert unrelated.read_text() == "font-size = 13\n"
    assert call("current")["id"] == theme["id"]
    # Bounded reload failures keep the new selection and name affected apps.
    log = root / "calls.jsonl"
    stub = root / "desktop tool"
    stub.write_text(f"#!{sys.executable}\n" + "import json,sys\n" + f"with open({str(log)!r}, 'a') as f: f.write(json.dumps(sys.argv[1:]) + '\\n')\n" + "print('error: synthetic')\n" + "sys.exit(0)\n")
    stub.chmod(0o700)
    catalog["commands"] = {key: str(stub) for key in ("hyprctl", "tmux", "systemctl", "gsettings", "vicinae")}
    save()
    desktop_env = dict(env, HYPRLAND_INSTANCE_SIGNATURE="fixture", DBUS_SESSION_BUS_ADDRESS="fixture")
    result = call("set", light["id"], environment=desktop_env)
    assert result["pending"] == ["Window borders"], result
    calls = [json.loads(line) for line in log.read_text().splitlines()]
    assert ["--user", "reload", "app-com.mitchellh.ghostty.service"] in calls
    assert ["set", "org.gnome.desktop.interface", "color-scheme", "prefer-light"] in calls
    assert ["vicinae://theme/set/catppuccin-latte"] in calls
    assert ["source-file", str(state / "current/tmux.conf")] in calls
    assert call("current")["id"] == light["id"]
print("Theme publication, persistence, concurrency, rollback, validation and reload fixtures passed")
