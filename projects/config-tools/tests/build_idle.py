#!/usr/bin/env python3
"""A nix build holds the scheduled theme step, then one toast reports it.

The process list is a fixture directory. `SEELE_BUILD_IDLE_PROC` is absolute
and unset by the user service; an unreadable list is not idle.
"""
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import calendar
import tempfile

theme = str(Path(sys.argv[1]).resolve())
idle = str(Path(sys.argv[2]).resolve())
python = Path(sys.executable).resolve()


def write_script(path, body):
    path.write_text(f"#!{python}\n{body}")
    path.chmod(path.stat().st_mode | stat.S_IEXEC)


def at(y, m, d, hour, minute=0):
    return calendar.timegm((y, m, d, hour, minute, 0))


with tempfile.TemporaryDirectory(prefix="seele-build-idle-") as temporary:
    root = Path(temporary)
    proc = root / "proc"
    proc.mkdir()
    config = root / "config"
    state = root / "state" / "seele-theme"
    catalog_file = config / "seele-theme" / "catalog.json"
    catalog_file.parent.mkdir(parents=True)
    launcher = root / "launcher.toml"
    launcher.write_text('[meta]\nname = "fixture"\n')
    palette = {f"base{i:02X}": f"#{i * 4096:06x}" for i in range(16)}

    def preset(identifier, name, mode):
        return dict(id=identifier, name=name, mode=mode, palette=palette, vicinaeTheme=str(launcher))

    catalog_file.write_text(json.dumps(dict(
        version=2,
        default="catppuccin-mocha",
        fontFamily="Mono",
        wallpaper="/w.jpg",
        themes=[
            preset("catppuccin-mocha", "Catppuccin Mocha", "dark"),
            preset("catppuccin-latte", "Catppuccin Latte", "light"),
        ],
        commands={},
    )))
    base = {
        "PATH": os.environ.get("PATH", ""),
        "HOME": str(root),
        "XDG_CONFIG_HOME": str(config),
        "XDG_STATE_HOME": str(root / "state"),
        "TZ": "UTC",
        "SEELE_BUILD_IDLE_PROC": str(proc),
    }

    def call(*args, now=None):
        env = dict(base)
        if now is not None:
            env["SEELE_THEME_NOW"] = str(now)
        result = subprocess.run([theme, *args], env=env, capture_output=True, text=True, timeout=15)
        assert result.returncode == 0, (args, result.stdout, result.stderr)
        return json.loads(result.stdout)

    def saved():
        return json.loads((state / "preferences.json").read_text())

    def shown():
        return json.loads((state / "selection.json").read_text())["id"]

    def active():
        result = subprocess.run([idle, "active"], env=base, capture_output=True, text=True, timeout=15)
        return result.returncode, result.stdout.strip(), result.stderr

    # No job yet. Turning the schedule on lands on the morning side and records
    # that boundary; the evening one is still waiting.
    code, word, err = active()
    assert (code, word) == (1, "idle"), (code, word, err)
    morning = at(2026, 9, 26, 9)
    evening = at(2026, 9, 26, 19, 1)
    call("init")
    call("auto", "schedule", "07:00", "19:00", now=morning)
    assert saved()["mode"] == "light" and shown() == "catppuccin-latte"
    acted = saved()["auto"]["last"]

    job = proc / "42"
    job.mkdir()
    (job / "cmdline").write_bytes(b"/nix/store/xx-nix/bin/nix\0build\0.#fixture\0")
    (job / "comm").write_text("nix\n")
    code, word, err = active()
    assert (code, word) == (0, "running"), (code, word, err)
    held = call("tick", now=evening)
    assert held["deferred"] == "nix-build", held
    assert saved()["mode"] == "light" and shown() == "catppuccin-latte", "the build holds the publication"
    assert saved()["auto"]["last"] == acted, "the boundary stays pending"

    # A command line that is not the build, and a legacy nix-build, stay exact.
    (job / "cmdline").write_bytes(b"nix\0eval\0")
    code, word, _err = active()
    assert (code, word) == (1, "idle"), word
    legacy = proc / "43"
    legacy.mkdir()
    (legacy / "cmdline").write_bytes(b"nix-build\0./release.nix\0")
    (legacy / "comm").write_text("nix-build\n")
    code, word, _err = active()
    assert (code, word) == (0, "running"), word
    held = call("tick", now=evening)
    assert held["deferred"] == "nix-build" and saved()["auto"]["last"] == acted
    for child in (job, legacy):
        for path in child.iterdir():
            path.unlink()
        child.rmdir()
    released = call("tick", now=evening)
    assert "deferred" not in released
    assert saved()["mode"] == "dark" and shown() == "catppuccin-mocha", "the held boundary applies once"

    missing = subprocess.run(
        [idle, "active"],
        env={**base, "SEELE_BUILD_IDLE_PROC": str(root / "missing")},
        capture_output=True,
        text=True,
        timeout=15,
    )
    assert missing.returncode == 2, missing.stderr

    # The terminal chain and a stand-in for the focused window.
    for pid, text in (
        (30, "30 (fish) S 20 30 0"),
        (20, "20 (tmux: server) S 10 20 0"),
        (10, "10 (ghostty) S 1 10 0"),
    ):
        directory = proc / str(pid)
        directory.mkdir()
        (directory / "stat").write_text(text)
    record = root / "sent"
    hyprctl = root / "hyprctl"
    notify = root / "notify-send"
    write_script(hyprctl, "print('{\"pid\": 99}')\n")
    write_script(
        notify,
        "import os, pathlib, sys\npathlib.Path(os.environ['RECORD']).write_text('\\n'.join(sys.argv[1:]))\n",
    )

    def notify_build(status, command, extra=None):
        if record.exists():
            record.unlink()
        env = dict(base, RECORD=str(record), **(extra or {}))
        result = subprocess.run(
            [
                idle, "notify",
                "--status", status,
                "--pid", "30",
                "--hyprctl", str(hyprctl),
                "--notify", str(notify),
                "--", command,
            ],
            env=env,
            capture_output=True,
            text=True,
            timeout=15,
        )
        assert result.returncode == 0, (result.stdout, result.stderr)
        return record.read_text() if record.exists() else ""

    sent = notify_build("1", "nix build .#fixture")
    assert "--app-name=Seele" in sent
    assert "--urgency=normal" in sent
    assert "--expire-time=30000" in sent
    assert "Build failed" in sent
    assert "nix build exited 1" in sent
    assert ".#fixture" not in sent, "arguments stay out of the toast"
    write_script(hyprctl, "print('{\"pid\": 10}')\n")
    assert notify_build("0", "nix build .#fixture") == ""
    write_script(hyprctl, "print('{}')\n")
    sent = notify_build("0", "nix build .#fixture")
    assert "Build finished" in sent and sent.endswith("nix build"), sent
    assert notify_build("1", "echo nix build") == ""
    assert notify_build("1", "nix build && echo done") == ""
    assert notify_build("1", "nix build .#fixture", {"SSH_TTY": "/dev/pts/9"}) == ""
    sent = notify_build("4", "/run/current-system/sw/bin/nix-build ./release.nix")
    assert "Build failed" in sent and "nix-build exited 4" in sent

print("nix build detection, deferred theme publication and unfocused done/fail notification passed")
