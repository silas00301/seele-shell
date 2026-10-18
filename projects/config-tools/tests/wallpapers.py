#!/usr/bin/env python3
"""The host's wallpaper mapping, through the real CLI.

Usage: python3 projects/config-tools/tests/wallpapers.py <seele-theme> <pictured theme folder>

A catalog maps images to light and dark themes, to outputs and to a theme. The
helper publishes, for the theme on screen, the image every output draws and
the outputs that draw another, and a mode switch changes both. A rebuild is a
new catalog followed by activation's `init`: the mapping it declares is the
one published, the chosen themes and mode stay, and a mapping the helper would
refuse fails `check`, which the flake runs on the catalog it installs, and
leaves the published theme alone.
"""
import copy
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import zlib

binary = str(Path(sys.argv[1]).resolve())
pictured = Path(sys.argv[2]).resolve()


def png(path, rgb):
    """A one-pixel PNG of colour `rgb`, a real image rather than its magic."""
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
    path.write_bytes(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 1, 1, 8, 2, 0, 0, 0))
                     + chunk(b"IDAT", zlib.compress(b"\0" + bytes(rgb))) + chunk(b"IEND", b""))
    return str(path)


with tempfile.TemporaryDirectory(prefix="seele-wallpapers-") as temporary:
    root = Path(temporary)
    config = root / "config"
    state = root / "state/seele-theme"
    catalog_file = config / "seele-theme/catalog.json"
    catalog_file.parent.mkdir(parents=True)
    env = {"PATH": os.environ.get("PATH", ""), "HOME": str(root), "XDG_CONFIG_HOME": str(config),
           "XDG_STATE_HOME": str(root / "state")}

    def run(*args):
        return subprocess.run([binary, *args], env=env, capture_output=True, text=True, timeout=15)

    def call(*args, ok=True):
        result = run(*args)
        assert (result.returncode == 0) == ok, (args, result.stdout, result.stderr)
        return json.loads(result.stdout) if ok else result

    # The pictured fixture folder, registered as the flake registers it.
    registered = root / "registered/pictured"
    result = run("register", "pictured", str(pictured), str(registered))
    assert result.returncode == 0, result.stderr
    source = json.loads((pictured / "theme.json").read_text())
    own = str(registered / "wallpaper.png")

    images = root / "images"
    images.mkdir()
    day, night = png(images / "day.png", (250, 240, 220)), png(images / "night.png", (16, 16, 32))
    wide, side_night = png(images / "wide.png", (90, 120, 160)), png(images / "side-night.png", (40, 30, 60))
    latte_side = png(images / "latte-side.png", (230, 200, 210))
    configured = "/test/background.jpg"

    launcher = root / "vicinae.toml"
    launcher.write_text('[meta]\nname = "Fixture"\nvariant = "dark"\n')
    palette = {f"base{i:02X}": f"#{i * 4096:06x}" for i in range(16)}
    mocha = dict(id="catppuccin-mocha", name="Catppuccin Mocha", mode="dark", palette=palette, vicinaeTheme=str(launcher))
    latte = dict(mocha, id="catppuccin-latte", name="Catppuccin Latte", mode="light", palette=dict(palette, base00="#eff1f5"))
    folder = dict(id="pictured", name=source["name"], mode=source["mode"], palette=source["palette"],
                  vicinaeTheme=str(launcher), document=str(registered / "theme.json"))
    mapping = {
        "light": day,
        "dark": night,
        "outputs": {"DP-1": {"image": wide}, "HDMI-A-1": {"dark": side_night}},
        "themes": {"catppuccin-latte": {"outputs": {"HDMI-A-1": latte_side}}},
    }
    catalog = dict(version=2, default="catppuccin-mocha", fontFamily="Maple Mono NF CN", wallpaper=configured,
                   wallpapers=mapping, themes=[mocha, latte, folder], commands={})

    def rebuild(value):
        """What a new generation does: a new catalog, then activation."""
        catalog_file.write_text(json.dumps(value))
        return call("init")

    def published():
        selection = json.loads((state / "selection.json").read_text())
        return selection["id"], selection["wallpaper"], selection.get("wallpaperOutputs")

    catalog_file.write_text(json.dumps(catalog))
    assert call("check", str(catalog_file)) == {"themes": 3}
    # Listing describes themes, not this host: no document carries the mapping.
    assert all("wallpaperOutputs" not in theme for theme in call("list")["themes"])
    assert all("wallpaper" not in theme for theme in call("list")["themes"][:2])
    # Beside the documents, each theme's linked image for the pickers to show:
    # its mode's here, and the pictured folder its own. An output's image is
    # that output's, and the configured wallpaper links nothing.
    assert call("list")["wallpapers"] == {"catppuccin-mocha": night, "catppuccin-latte": day, "pictured": own}
    unmapped = dict(catalog, wallpapers={"outputs": {"DP-1": {"image": wide}}})
    catalog_file.write_text(json.dumps(unmapped))
    assert call("list")["wallpapers"] == {"pictured": own}
    # A linked file that is not there yet is not offered; the theme shows
    # without a picture rather than with a broken one.
    broken = dict(catalog, wallpapers={"light": str(root / "not-yet.png")})
    catalog_file.write_text(json.dumps(broken))
    assert call("list")["wallpapers"] == {"pictured": own}
    catalog_file.write_text(json.dumps(catalog))
    assert not state.exists()

    # A dark theme takes the dark images, and each output its own.
    rebuild(catalog)
    assert published() == ("catppuccin-mocha", night, {"DP-1": wide, "HDMI-A-1": side_night}), published()
    # A mode switch brings the light theme and the light images with it. The
    # side output has no light image of its own, but Latte is given one there.
    call("mode", "light")
    assert published() == ("catppuccin-latte", day, {"DP-1": wide, "HDMI-A-1": latte_side}), published()
    call("mode", "dark")
    assert published() == ("catppuccin-mocha", night, {"DP-1": wide, "HDMI-A-1": side_night}), published()
    # A theme with its own image keeps it on every output the mapping does not
    # give that theme another one for.
    call("pick", "pictured")
    assert published() == ("pictured", own, None), published()
    selection = json.loads((state / "selection.json").read_text())
    assert selection["radius"] == source["radius"] and "wallpaperOutputs" not in selection

    # A rebuild that declares the same mapping publishes the same theme, byte
    # for byte, and keeps the chosen themes and mode.
    call("slot", "dark", "catppuccin-mocha")
    before = (state / "selection.json").read_bytes()
    preferences = (state / "preferences.json").read_bytes()
    rebuild(catalog)
    assert (state / "selection.json").read_bytes() == before
    assert (state / "preferences.json").read_bytes() == preferences

    # A rebuild that changes the mapping, with images at new paths as a new
    # generation's store would have them, publishes the new mapping for the
    # same choice: no command has to be run again.
    second = root / "generation-2"
    second.mkdir()
    changed = copy.deepcopy(catalog)
    changed["wallpapers"] = {
        "dark": png(second / "night.png", (8, 8, 24)),
        "outputs": {"DP-1": {"dark": png(second / "wide-night.png", (20, 40, 80))},
                    "eDP-1": {"image": png(second / "laptop.png", (60, 60, 60))}},
        "themes": {"pictured": {"outputs": {"DP-1": png(second / "pictured-wide.png", (200, 120, 40))}}},
    }
    rebuild(changed)
    assert published() == ("catppuccin-mocha", str(second / "night.png"),
                           {"DP-1": str(second / "wide-night.png"), "eDP-1": str(second / "laptop.png")}), published()
    assert (state / "preferences.json").read_bytes() == preferences
    # The theme's own image beats the laptop's, which is mapped for every
    # theme; the wide output's, mapped for this theme, beats its own.
    call("pick", "pictured")
    assert published() == ("pictured", own, {"DP-1": str(second / "pictured-wide.png")}), published()
    call("pick", "catppuccin-mocha")

    # A mapped image that is not there yet draws the base colour, as the
    # configured wallpaper would; it does not stop the switcher.
    missing = copy.deepcopy(catalog)
    missing["wallpapers"] = {"dark": str(root / "not-yet.png")}
    rebuild(missing)
    assert published() == ("catppuccin-mocha", str(root / "not-yet.png"), None), published()

    # A rebuild without a mapping goes back to the configured wallpaper.
    plain = copy.deepcopy(catalog)
    del plain["wallpapers"]
    rebuild(plain)
    assert published() == ("catppuccin-mocha", configured, None), published()

    # A mapping the helper would refuse fails `check`, naming every rule it
    # broke, and refuses activation without touching what is published.
    gif = images / "animated.gif"
    gif.write_bytes(b"GIF89a\x01\x00\x01\x00")
    broken = copy.deepcopy(catalog)
    broken["wallpapers"] = {"light": "day.png", "dark": str(gif), "outputs": {"DP 1": {"image": wide}},
                            "themes": {"catppuccin-mocah": {"image": wide}}}
    before = (state / "selection.json").read_bytes()
    current = (state / "current").readlink()
    catalog_file.write_text(json.dumps(broken))
    refused = call("check", str(catalog_file), ok=False)
    assert refused.stderr.strip().endswith("\n".join([
        "Invalid wallpaper mapping:",
        '  light: "day.png" is not an absolute path',
        f"  dark: {json.dumps(str(gif))} is not a PNG or JPEG image",
        '  themes."catppuccin-mocah" is not a theme in the catalog',
        "  \"DP 1\" is not an output name: letters, digits, '.', '_' and '-', at most 64",
    ])), refused.stderr
    call("init", ok=False)
    call("mode", "light", ok=False)
    assert (state / "selection.json").read_bytes() == before
    assert (state / "current").readlink() == current
    # An image cut short still starts as a PNG: every read accepts it, as a
    # registered folder's copy is rechecked by its signature, but the build's
    # `check` decodes it and refuses it before it can draw as nothing.
    cut = images / "cut.png"
    cut.write_bytes(Path(day).read_bytes()[:-12])
    catalog_file.write_text(json.dumps(dict(catalog, wallpapers={"dark": str(cut)})))
    refused = call("check", str(catalog_file), ok=False)
    assert f"  dark: {json.dumps(str(cut))} is not a complete PNG image" in refused.stderr, refused.stderr
    call("list")
    for wrong in ({"sepia": day}, {"outputs": {"DP-1": {"sepia": day}}}, {"themes": {"catppuccin-mocha": {"dark": day}}}):
        catalog_file.write_text(json.dumps(dict(catalog, wallpapers=wrong)))
        call("check", str(catalog_file), ok=False)
    catalog_file.write_text(json.dumps(catalog))
    call("init")
    assert published() == ("catppuccin-mocha", night, {"DP-1": wide, "HDMI-A-1": side_night}), published()
print("Wallpaper mapping: light and dark, per output and per theme, through mode switches, rebuilds and refusals")
