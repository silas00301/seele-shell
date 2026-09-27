#!/usr/bin/env python3
"""Exercise the real CLI with private config/state and fake desktop tools."""
import concurrent.futures
import copy
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import time

binary = str(Path(sys.argv[1]).resolve())
with tempfile.TemporaryDirectory(prefix="seele-themes-") as temporary:
    root = Path(temporary)
    config = root / "config with spaces"
    state_home = root / "state with spaces"
    state = state_home / "seele-theme"
    catalog_file = config / "seele-theme/catalog.json"
    catalog_file.parent.mkdir(parents=True)
    env = {"PATH": os.environ.get("PATH", ""), "HOME": str(root), "XDG_CONFIG_HOME": str(config), "XDG_STATE_HOME": str(state_home)}
    launcher = root / "stylix-vicinae.toml"
    launcher.write_text('[meta]\nname = "Stylix fixture"\nvariant = "light"\n[colors.core]\nbackground = "#eff1f5"\n')
    asset_files = {}
    for name, content in {
        "gtkCss": "@define-color theme_bg_color #eff1f5;\n",
        "gtkSourceView": '<style-scheme id="stylix"/>\n',
        "zenChrome": ":root { --zen-primary-color: #eff1f5; }\n",
        "zenContent": ":root { --color-accent-primary: #eff1f5; }\n",
        "spicetify": "[base]\nmain = eff1f5\n",
        "kvantumConfig": "[General]\nframe_width=2\n",
        "kvantumSvg": '<svg xmlns="http://www.w3.org/2000/svg"/>\n',
        "kdeColors": "[General]\nName=Fixture\n",
    }.items():
        path = root / name
        path.write_text(content)
        asset_files[name] = str(path)
    palette = {f"base{i:02X}": f"#{i * 4096:06x}" for i in range(16)}
    theme = dict(id="catppuccin-mocha", name="Catppuccin Mocha", mode="dark", palette=palette, vicinaeTheme=str(launcher), assets=asset_files)
    light = dict(theme, id="flexoki-light", name="Flexoki Light", mode="light", palette=dict(palette, base00="#eff1f5", base0D="#7287fd"))
    vesktop = config / "vesktop"
    vesktop_settings = vesktop / "settings"
    vesktop_settings.mkdir(parents=True)
    (vesktop_settings / "settings.json").write_text(json.dumps({"themeLinks": ["https://catppuccin.github.io/discord/dist/catppuccin-mocha.theme.css", "https://example.invalid/other.css"], "useQuickCss": True, "other": 7}))
    quick_css = vesktop_settings / "quickCss.css"
    quick_css.write_text(".user-rule { color: red; }\n")
    catalog = dict(version=2, default=theme["id"], fontFamily="Maple Mono NF CN", wallpaper="/test/background.jpg", themes=[theme, light], commands={}, vesktopDir=str(vesktop))
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
    assert "catppuccin-mocha.theme.css" in quick_css.read_text()
    assert ".user-rule { color: red; }" in quick_css.read_text()
    settings = json.loads((vesktop_settings / "settings.json").read_text())
    assert settings == {"themeLinks": ["https://example.invalid/other.css"], "useQuickCss": True, "other": 7}
    quick_css_inode = quick_css.stat().st_ino
    with socket.socket() as reservation:
        reservation.bind(("127.0.0.1", 0))
        port = reservation.getsockname()[1]
    serve_env = dict(env, SEELE_THEME_PALETTE_PORT=str(port))
    server = subprocess.Popen([binary, "serve"], env=serve_env, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
    def palette_response():
        with socket.create_connection(("127.0.0.1", port), timeout=2) as connection:
            connection.sendall(b"GET /palette HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
            chunks = []
            while chunk := connection.recv(4096):
                chunks.append(chunk)
        headers, body = b"".join(chunks).split(b"\r\n\r\n", 1)
        assert b"200 OK" in headers and b"Access-Control-Allow-Origin: *" in headers
        return json.loads(body)
    try:
        for _ in range(40):
            try:
                first = palette_response()
                break
            except OSError:
                assert server.poll() is None, server.stderr.read().decode()
                time.sleep(0.05)
        else:
            raise AssertionError("Palette service did not start")
        assert first == {"id": theme["id"], "palette": theme["palette"]}
        call("set", light["id"])
        assert palette_response() == {"id": light["id"], "palette": light["palette"]}
        assert quick_css.stat().st_ino == quick_css_inode, "Vencord watches the existing QuickCSS inode"
        assert "--background-primary: #eff1f5" in quick_css.read_text()
        assert "catppuccin-mocha.theme.css" not in quick_css.read_text()
        assert ".user-rule { color: red; }" in quick_css.read_text()
    finally:
        server.terminate()
        server.wait(timeout=3)
    call("set", theme["id"])
    assert (state / "selection.json").stat().st_mode & 0o777 == 0o600
    assert (state / "current").is_symlink()
    assert call("current")["id"] == theme["id"]
    unrelated = config / "ghostty/config"
    unrelated.parent.mkdir()
    unrelated.write_text("font-size = 13\n")
    # Publication must reach every include without writing app-owned settings.
    call("set", light["id"])
    selection = json.loads((state / "selection.json").read_text())
    assert selection["base"] == light["palette"]["base00"] and selection["fontFamily"] == catalog["fontFamily"]
    assert light["palette"]["base00"] in (state / "current/ghostty").read_text()
    assert light["palette"]["base0D"][1:] in (state / "current/fish.fish").read_text()
    assert light["palette"]["base0D"][1:] in (state / "current/hyprland.lua").read_text()
    assert light["palette"]["base00"] in (state / "current/tmux.conf").read_text()
    assert light["palette"]["base00"] in (state / "current/gtk.css").read_text()
    assert unrelated.read_text() == "font-size = 13\n"
    assert (state / "current/vicinae.toml").read_bytes() == launcher.read_bytes()
    for name, published in {"gtkCss": "gtk.css", "gtkSourceView": "gtksourceview.xml", "zenChrome": "zen-chrome.css", "zenContent": "zen-content.css", "spicetify": "spicetify.ini", "kvantumConfig": "kvantum.kvconfig", "kvantumSvg": "kvantum.svg", "kdeColors": "kde.colors"}.items():
        assert (state / "current" / published).read_bytes() == Path(asset_files[name]).read_bytes()
    assert "*background: #eff1f5" in (state / "current/Xresources").read_text()
    assert selection["palette"] == light["palette"] and selection["mode"] == "light" and selection["version"] == 2
    assert "vicinaeTheme" not in selection and "flavor" not in selection
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
    invalid["themes"][0]["palette"]["base0D"] = "#fff; exec hostile"
    save(invalid)
    call("set", theme["id"], ok=False)
    assert (state / "selection.json").read_bytes() == before
    assert (state / "current").readlink() == old_link
    save()
    # Missing/extra slots, invalid modes and unreadable generated assets fail
    # before publication, including for non-Catppuccin theme IDs.
    for key in ("base00", "base0F"):
        invalid = copy.deepcopy(catalog)
        del invalid["themes"][1]["palette"][key]
        save(invalid); call("set", light["id"], ok=False)
    invalid = copy.deepcopy(catalog); invalid["themes"][1]["palette"]["base10"] = "#123456"
    save(invalid); call("list", ok=False)
    invalid = copy.deepcopy(catalog); invalid["themes"][1]["mode"] = "invalid"
    save(invalid); call("list", ok=False)
    invalid = copy.deepcopy(catalog); invalid["themes"][1]["vicinaeTheme"] = str(root / "missing")
    save(invalid); call("set", light["id"], ok=False)
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
    # A v1 saved selection is migrated by ID rather than reset to the default.
    (state / "selection.json").write_text(json.dumps({"id": theme["id"], "flavor": "mocha"}))
    call("init")
    assert json.loads((state / "selection.json").read_text())["version"] == 2
    assert call("current")["id"] == theme["id"]
    # Bounded reload failures keep the new selection and name affected apps.
    log = root / "calls.jsonl"
    stub = root / "desktop tool"
    stub.write_text(f"#!{sys.executable}\n" + "import json,sys\n" + f"with open({str(log)!r}, 'a') as f: f.write(json.dumps(sys.argv[1:]) + '\\n')\n" + "print('error: synthetic')\n" + "sys.exit(0)\n")
    stub.chmod(0o700)
    catalog["commands"] = {key: str(stub) for key in ("hyprctl", "tmux", "pgrep", "pkill", "xrdb", "gsettings", "vicinae", "kdecolors")}
    save()
    desktop_env = dict(env, HYPRLAND_INSTANCE_SIGNATURE="fixture", DBUS_SESSION_BUS_ADDRESS="fixture", DISPLAY="fixture")
    result = call("set", light["id"], environment=desktop_env)
    assert result["pending"] == ["Window borders"], result
    calls = [json.loads(line) for line in log.read_text().splitlines()]
    uid = str(os.geteuid())
    ghostty = "^/nix/store/[a-z0-9]+-ghostty-[^/]+/bin/ghostty([[:space:]]|$)"
    assert ["-u", uid, "-f", ghostty] in calls
    assert ["-USR2", "-u", uid, "-f", ghostty] in calls
    assert ["set", "org.gnome.desktop.interface", "color-scheme", "prefer-light"] in calls
    assert ["vicinae://theme/set/seele-current"] in calls
    assert ["source-file", str(state / "current/tmux.conf")] in calls
    assert ["-merge", str(state / "current/Xresources")] in calls
    assert ["Seele-" + light["id"]] in calls
    assert call("current")["id"] == light["id"]
print("Theme publication, persistence, concurrency, rollback, validation and reload fixtures passed")
