#!/usr/bin/env python3
"""Drop-in theme folders through the real CLI: registration, the catalog, publication.

Usage: python3 theme_folders.py <seele-theme> <tests/theme-folders> <tests/theme-snapshots/folders.json> [--update]

Registers the example and pictured folders and every folder under invalid/,
then lists and publishes them from a catalog beside a preset: a theme with its
own wallpaper publishes it, and one without keeps the configured image. The snapshot harness
renders the documents in folders.json, so they must be exactly what the helper
lists for the registered example; --update rewrites that file instead.

The folders the shell ships, those `themes/default.nix` lists beside
`tests/`, register, list and publish the same way, and registered.json beside
folders.json holds them as the helper lists them, for the harness to render.
A file a shipped folder names is listed at its registered path; registered.json
gives it as the same file in the repository, `themes/<id>/<name>`, which the
harness reads in its place.
"""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

binary = str(Path(sys.argv[1]).resolve())
folders = Path(sys.argv[2]).resolve()
snapshot = Path(sys.argv[3]).resolve()
update = "--update" in sys.argv[4:]

# Each invalid fixture, and every line its refusal has to name.
refusals = {
    "incomplete": [
        "palette.base0F: missing",
        "missing required slot accent",
        "missing required slot yellow",
    ],
    "misnamed": ['id: "example" is not the folder\'s name'],
    "unknown-slots": [
        'accent: "#80f2aa4c" is translucent; a palette role is an opaque #rrggbb',
        "colorLensSize: derived, so a theme cannot supply it",
        'radius: "round" is not a int',
        "sparkle: not a contract slot",
    ],
    "escaping-file": ['soundNotification: "../misnamed/theme.json" is not a path inside the theme folder'],
    "wallpaper-not-image": ['wallpaper: "wallpaper.png" is not a PNG or JPEG image'],
    "sound-not-wave": ['soundNotification: "sounds/ping.oga" is not a WAV sound'],
    "material-drift": [
        "material: theme.json is not what its seed grows (radius: 9 here, 16 from its seed); `seele-theme material` writes it again"
    ],
}

with tempfile.TemporaryDirectory(prefix="seele-theme-folders-") as temporary:
    root = Path(temporary)
    env = {"PATH": os.environ.get("PATH", ""), "HOME": str(root)}

    def run(*args, environment=env):
        return subprocess.run([binary, *args], env=environment, capture_output=True, text=True, timeout=15)

    def register(folder, out, name=None):
        return run("register", name or folder.name, str(folder), str(out))

    # The example registers as it is written: it names no file of its own.
    example = folders / "example"
    registered = root / "registered/example"
    result = register(example, registered)
    assert result.returncode == 0, result.stderr
    source = json.loads((example / "theme.json").read_text())
    document = json.loads((registered / "theme.json").read_text())
    assert document == source
    assert sorted(path.name for path in registered.iterdir()) == ["theme.json"]

    # Every refusal names the folder and each rule it broke, and writes nothing.
    for name, lines in refusals.items():
        out = root / "refused" / name
        result = register(folders / "invalid" / name, out)
        assert result.returncode == 1, (name, result.stdout, result.stderr)
        message = result.stderr.splitlines()
        assert message[0] == f"Seele Themes: Invalid theme folder {name}:", result.stderr
        for line in lines:
            assert f"  {line}" in message, (name, line, result.stderr)
        assert len(message) == len(lines) + 1, (name, result.stderr)
        assert not out.exists(), name
    result = register(root / "missing", root / "refused/missing")
    assert result.returncode == 1 and "  no theme.json" in result.stderr, result.stderr
    result = run("register", "example", str(example), "relative/out")
    assert result.returncode == 1 and "absolute path" in result.stderr, result.stderr

    # A file the folder names is copied into the registered copy and named by
    # its absolute path there; an unnamed file stays behind.
    sounding = root / "sounding"
    (sounding / "sounds").mkdir(parents=True)
    (sounding / "sounds/ping.wav").write_bytes(b"RIFF\x24\0\0\0WAVEfmt ")
    (sounding / "draft.txt").write_text("not named\n")
    (sounding / "theme.json").write_text(json.dumps(dict(source, id="sounding", soundNotification="sounds/ping.wav")))
    out = root / "registered/sounding"
    result = register(sounding, out)
    assert result.returncode == 0, result.stderr
    sounded = json.loads((out / "theme.json").read_text())
    assert sounded["soundNotification"] == str(out / "sounds/ping.wav")
    assert (out / "sounds/ping.wav").read_bytes() == b"RIFF\x24\0\0\0WAVEfmt "
    assert not (out / "draft.txt").exists()

    # So is a theme's own wallpaper, once it is known to be a PNG or a JPEG.
    pictured = folders / "pictured"
    pictured_out = root / "registered/pictured"
    result = register(pictured, pictured_out)
    assert result.returncode == 0, result.stderr
    pictured_source = json.loads((pictured / "theme.json").read_text())
    assert pictured_source["wallpaper"] == "wallpaper.png"
    pictured_document = json.loads((pictured_out / "theme.json").read_text())
    assert pictured_document == dict(pictured_source, wallpaper=str(pictured_out / "wallpaper.png")), pictured_document
    assert (pictured_out / "wallpaper.png").read_bytes() == (pictured / "wallpaper.png").read_bytes()
    assert sorted(path.name for path in pictured_out.iterdir()) == ["theme.json", "wallpaper.png"]
    # An image cut short still starts as a PNG, but Qt would draw nothing.
    cut = root / "cut"
    cut.mkdir()
    image = (pictured / "wallpaper.png").read_bytes()
    (cut / "wallpaper.png").write_bytes(image[: len(image) // 2])
    (cut / "theme.json").write_text(json.dumps(dict(pictured_source, id="cut")))
    result = register(cut, root / "refused/cut")
    assert result.returncode == 1, result.stdout
    assert '  wallpaper: "wallpaper.png" is not a complete PNG image' in result.stderr, result.stderr
    assert not (root / "refused/cut").exists()

    # The catalog names the registered document beside a preset.
    config = root / "config"
    (config / "seele-theme").mkdir(parents=True)
    launcher = root / "vicinae.toml"
    launcher.write_text('[meta]\nname = "Fixture"\nvariant = "dark"\n')
    preset_palette = {f"base{i:02X}": f"#{i * 4096:06x}" for i in range(16)}
    preset = dict(id="catppuccin-mocha", name="Catppuccin Mocha", mode="dark", palette=preset_palette, vicinaeTheme=str(launcher))
    entry = dict(id="example", name=source["name"], mode=source["mode"], palette=source["palette"],
                 vicinaeTheme=str(launcher), document=str(registered / "theme.json"))
    catalog = dict(version=2, default="catppuccin-mocha", fontFamily="Maple Mono NF CN",
                   wallpaper="/test/background.jpg", themes=[preset, entry], commands={})
    catalog_env = dict(env, XDG_CONFIG_HOME=str(config), XDG_STATE_HOME=str(root / "state"))

    def call(*args, ok=True):
        (config / "seele-theme/catalog.json").write_text(json.dumps(catalog))
        result = run(*args, environment=catalog_env)
        assert (result.returncode == 0) == ok, (args, result.stdout, result.stderr)
        return json.loads(result.stdout) if ok else result

    listed = call("list")["themes"]
    assert [theme["id"] for theme in listed] == ["catppuccin-mocha", "example"]
    folder = listed[1]
    # The folder's own document, as registered, plus the configured typeface
    # and the terminal colours of its Base16 palette. Its roles are its own.
    assert {key: value for key, value in folder.items() if key not in ("fontFamily", "terminal")} == document
    assert folder["fontFamily"] == "Maple Mono NF CN"
    assert folder["terminal"][0] == source["palette"]["base00"] and folder["terminal"][15] == source["palette"]["base07"]
    assert folder["subtext"] == source["subtext"] != source["palette"]["base04"]
    if update:
        snapshot.write_text(json.dumps({"themes": [folder]}, indent=2, ensure_ascii=False) + "\n")
    else:
        assert json.loads(snapshot.read_text()) == {"themes": [folder]}, f"{snapshot} is not what seele-theme lists: run with --update"

    # Publishing it gives the shell every slot it supplies, and the
    # applications its roles.
    call("set", "example")
    state = root / "state/seele-theme"
    selection = json.loads((state / "selection.json").read_text())
    assert selection == dict(folder, version=2, wallpaper="/test/background.jpg"), selection
    assert selection["radius"] == 14 and selection["radiusSmall"] == 10
    ghostty = (state / "current/ghostty").read_text()
    assert f"background = {source['base']}\n" in ghostty and f"foreground = {source['text']}\n" in ghostty
    assert f"set -g fish_pager_color_description {source['subtext'][1:]}\n" in (state / "current/fish.fish").read_text()
    assert (state / "current/vicinae.toml").read_bytes() == launcher.read_bytes()

    # A folder that names its own typeface keeps it.
    named = root / "named"
    named.mkdir()
    (named / "theme.json").write_text(json.dumps(dict(source, id="named", fontFamily="Example Sans")))
    assert register(named, root / "registered/named").returncode == 0
    catalog["themes"].append(dict(entry, id="named", document=str(root / "registered/named/theme.json")))
    assert call("list")["themes"][2]["fontFamily"] == "Example Sans"
    catalog["themes"].pop()

    # The catalog is refused whole when an entry and its document disagree,
    # or the document is gone, naming the theme.
    catalog["themes"][1] = dict(entry, palette=dict(source["palette"], base00="#000000"))
    result = call("list", ok=False)
    assert "Invalid theme document example:\n  palette: not the catalog's" in result.stderr, result.stderr
    catalog["themes"][1] = dict(entry, document=str(root / "missing.json"))
    result = call("list", ok=False)
    assert "Invalid theme document example:\n  its registered document cannot be read" in result.stderr, result.stderr
    catalog["themes"][1] = dict(entry, document="theme.json")
    assert "must be an absolute path" in call("list", ok=False).stderr
    catalog["themes"][1] = entry
    assert call("current")["id"] == "example"

    # Switching to a theme with its own wallpaper publishes that image for the
    # desktop and the lock screen; switching to one without, folder or preset,
    # publishes the configured image again, so the wallpaper on screen stays.
    catalog["themes"].append(dict(entry, id="pictured", name=pictured_source["name"],
                                  document=str(pictured_out / "theme.json")))
    listed = call("list")["themes"]
    assert listed[2]["wallpaper"] == str(pictured_out / "wallpaper.png")
    assert "wallpaper" not in listed[0] and "wallpaper" not in listed[1]
    published = lambda: json.loads((state / "selection.json").read_text())["wallpaper"]
    for theme, wallpaper in [("pictured", str(pictured_out / "wallpaper.png")), ("example", "/test/background.jpg"),
                             ("pictured", str(pictured_out / "wallpaper.png")), ("catppuccin-mocha", "/test/background.jpg")]:
        call("set", theme)
        assert published() == wallpaper, (theme, published())
    call("pick", "pictured")
    assert published() == str(pictured_out / "wallpaper.png")
    # A registered image that is no longer one refuses the catalog, naming it.
    (pictured_out / "wallpaper.png").chmod(0o644)
    (pictured_out / "wallpaper.png").write_text("not an image\n")
    result = call("list", ok=False)
    assert f'Invalid theme document pictured:\n  wallpaper: "{pictured_out / "wallpaper.png"}" is not a PNG or JPEG image' in result.stderr, result.stderr

    # The folders the shell ships: each registers, lists after the preset in
    # the order themes/default.nix gives, and publishes every slot it supplies.
    shipped_root = folders.parent.parent / "themes"
    shipped = [line.strip()[2:] for line in (shipped_root / "default.nix").read_text().splitlines()
               if line.strip().startswith("./")]
    entries = []
    for name in shipped:
        out = root / "registered" / name
        result = register(shipped_root / name, out)
        assert result.returncode == 0, (name, result.stderr)
        written = json.loads((shipped_root / name / "theme.json").read_text())
        entries.append(dict(id=name, name=written["name"], mode=written["mode"], palette=written["palette"],
                            vicinaeTheme=str(launcher), document=str(out / "theme.json")))
    catalog["themes"] = [preset] + entries
    # A state of their own: the one above has selected a folder this catalog
    # no longer lists.
    catalog_env["XDG_STATE_HOME"] = str(root / "shipped-state")
    state = root / "shipped-state/seele-theme"
    listed = call("list")["themes"][1:]
    assert [theme["id"] for theme in listed] == shipped
    registered_root = f"{root / 'registered'}/"

    def in_repository(theme):
        """The listed document, with each registered file named where it lies in the repository."""
        named = {}
        for key, value in theme.items():
            if isinstance(value, str) and value.startswith(registered_root):
                relative = value[len(registered_root):]
                copy = root / "registered" / relative
                source = shipped_root / relative
                assert source.read_bytes() == copy.read_bytes(), (key, value)
                value = f"themes/{relative}"
            named[key] = value
        return named

    repository = [in_repository(theme) for theme in listed]
    registered_snapshot = snapshot.parent / "registered.json"
    if update:
        registered_snapshot.write_text(json.dumps({"themes": repository}, indent=2, ensure_ascii=False) + "\n")
    else:
        assert json.loads(registered_snapshot.read_text()) == {"themes": repository}, f"{registered_snapshot} is not what seele-theme lists: run with --update"
    for theme in listed:
        call("set", theme["id"])
        selection = json.loads((state / "selection.json").read_text())
        # A folder with an image of its own publishes it; any other the configured one.
        assert selection == dict(theme, version=2, wallpaper=theme.get("wallpaper", "/test/background.jpg")), theme["id"]

print(f"Theme folders: the example and pictured folders register, list and publish their wallpaper; {len(refusals) + 2} refusals name every broken rule; {len(shipped)} shipped folders register and publish")
