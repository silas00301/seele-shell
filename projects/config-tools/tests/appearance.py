#!/usr/bin/env python3
"""Drive the real CLI's light and dark slots, mode and schedule with private
config and state, a fixed clock and a synthetic tz table."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import calendar
import time

binary = str(Path(sys.argv[1]).resolve())
with tempfile.TemporaryDirectory(prefix="seele-appearance-") as temporary:
    root = Path(temporary)
    config = root / "config"
    state = root / "state/seele-theme"
    zoneinfo = root / "zoneinfo"
    zoneinfo.mkdir()
    # Berlin's reference city, without its TZif file: the C library then keeps
    # local time at UTC, so every clock below is a UTC clock.
    (zoneinfo / "zone1970.tab").write_text("DE,DK,NO,SE,SJ\t+5230+01322\tEurope/Berlin\tmost of Germany\n")
    catalog_file = config / "seele-theme/catalog.json"
    catalog_file.parent.mkdir(parents=True)
    launcher = root / "launcher.toml"
    launcher.write_text('[meta]\nname = "fixture"\n')
    palette = {f"base{i:02X}": f"#{i * 4096:06x}" for i in range(16)}
    def preset(id, name, mode):
        return dict(id=id, name=name, mode=mode, palette=palette, vicinaeTheme=str(launcher))
    themes = [
        preset("catppuccin-mocha", "Catppuccin Mocha", "dark"),
        preset("nord", "Nord", "dark"),
        preset("flexoki-light", "Flexoki Light", "light"),
        preset("catppuccin-latte", "Catppuccin Latte", "light"),
    ]
    catalog_file.write_text(json.dumps(dict(version=2, default="catppuccin-mocha", fontFamily="Mono", wallpaper="/w.jpg", themes=themes, commands={})))
    base = {"PATH": os.environ.get("PATH", ""), "HOME": str(root), "XDG_CONFIG_HOME": str(config),
            "XDG_STATE_HOME": str(root / "state"), "TZ": "UTC", "TZDIR": str(zoneinfo)}
    def at(y, m, d, hour, minute=0):
        return calendar.timegm((y, m, d, hour, minute, 0))
    def call(*args, ok=True, now=None, tz=None):
        env = dict(base)
        if now is not None:
            env["SEELE_THEME_NOW"] = str(now)
        if tz is not None:
            env["TZ"] = tz
        result = subprocess.run([binary, *args], env=env, capture_output=True, text=True, timeout=15)
        assert (result.returncode == 0) == ok, (args, result.stdout, result.stderr)
        return json.loads(result.stdout) if ok else result
    def shown():
        return json.loads((state / "selection.json").read_text())["id"]
    def saved():
        return json.loads((state / "preferences.json").read_text())

    # Before anything is saved, the defaults: the configured theme is its own
    # mode's slot, and the other mode starts at the same family's variant.
    listed = call("list")["appearance"]
    assert (listed["mode"], listed["dark"], listed["light"]) == ("dark", "catppuccin-mocha", "catppuccin-latte"), listed
    assert listed["auto"] == {"source": "off", "lightAt": "07:00", "darkAt": "19:00"}
    assert listed["next"] is None
    assert not state.exists(), "listing must not create state"

    # An earlier single selection is migrated into its own mode's slot.
    call("init")
    call("set", "flexoki-light")
    (state / "preferences.json").unlink()
    migrated = call("list")["appearance"]
    assert (migrated["mode"], migrated["light"], migrated["dark"]) == ("light", "flexoki-light", "catppuccin-mocha"), migrated
    call("init")
    assert saved()["light"] == "flexoki-light" and saved()["mode"] == "light", "activation records the migration"
    assert (state / "preferences.json").stat().st_mode & 0o777 == 0o600

    # The mode picks a slot; `set` writes the slot of the mode on screen.
    reply = call("mode", "dark")
    assert shown() == "catppuccin-mocha" and reply["appearance"]["mode"] == "dark"
    call("set", "nord")
    assert shown() == "nord" and saved()["dark"] == "nord"
    assert saved()["light"] == "flexoki-light", "the other mode's slot is untouched"
    # Any preset may fill either slot, a light one in the dark slot included.
    call("set", "catppuccin-latte")
    assert saved()["dark"] == "catppuccin-latte"
    call("set", "nord")

    # A slot of the other mode is saved without touching the screen.
    generation = os.readlink(state / "current")
    call("slot", "light", "catppuccin-latte")
    assert saved()["light"] == "catppuccin-latte"
    assert os.readlink(state / "current") == generation, "an inactive slot republishes nothing"
    # Choosing the mode already on screen republishes nothing either.
    call("mode", "dark")
    assert os.readlink(state / "current") == generation
    call("mode", "light")
    assert shown() == "catppuccin-latte"

    # Restore puts back mode and both slots at once.
    call("restore", "dark", "catppuccin-mocha", "flexoki-light")
    assert (shown(), saved()["dark"], saved()["light"], saved()["mode"]) == ("catppuccin-mocha", "catppuccin-mocha", "flexoki-light", "dark")

    # Refusals leave everything as it was.
    before = saved()
    for args in [("mode", "dusk"), ("slot", "light", "missing"), ("set", "missing"), ("restore", "dark", "missing", "nord"),
                 ("auto", "schedule", "7:00", "19:00"), ("auto", "schedule", "07:00", "07:00"), ("auto", "schedule"),
                 ("auto", "sometimes"), ("slot", "grey", "nord")]:
        call(*args, ok=False)
    assert saved() == before

    # A fixed schedule: turning it on puts the desktop where the schedule says.
    morning = at(2026, 9, 26, 9)
    reply = call("auto", "schedule", "07:00", "19:00", now=morning)
    assert saved()["mode"] == "light" and shown() == "flexoki-light", "09:00 is inside 07:00 to 19:00"
    assert reply["appearance"]["next"] == {"mode": "dark", "at": at(2026, 9, 26, 19), "clock": "19:00"}
    # A choice by hand holds until the next boundary...
    call("mode", "dark", now=at(2026, 9, 26, 10))
    call("tick", now=at(2026, 9, 26, 12))
    assert saved()["mode"] == "dark", "noon is still the morning's boundary, already acted on"
    # ...and the boundary then decides.
    call("tick", now=at(2026, 9, 26, 19, 1))
    assert saved()["mode"] == "dark" and shown() == "catppuccin-mocha"
    call("mode", "light", now=at(2026, 9, 26, 20))
    call("tick", now=at(2026, 9, 27, 7, 30))
    assert saved()["mode"] == "light", "the morning's boundary agrees with the hand's choice"
    call("tick", now=at(2026, 9, 27, 19, 30))
    assert saved()["mode"] == "dark", "the evening's boundary switches"
    # A missed boundary (a suspended laptop) is caught up at the next tick,
    # and only the most recent one counts.
    call("tick", now=at(2026, 9, 29, 8))
    assert saved()["mode"] == "light"
    # Off keeps the times for next time and plans nothing.
    reply = call("auto", "off", now=at(2026, 9, 29, 21))
    assert reply["appearance"]["next"] is None and saved()["auto"]["lightAt"] == "07:00"
    assert saved()["mode"] == "light", "turning the schedule off changes no mode"
    call("tick", now=at(2026, 9, 29, 21))
    assert saved()["mode"] == "light"

    # Following the sun, from the timezone's reference city.
    winter_night = at(2026, 12, 21, 18)
    reply = call("auto", "sun", now=winter_night, tz="Europe/Berlin")
    appearance = reply["appearance"]
    assert appearance["place"] == "Berlin" and appearance["auto"]["source"] == "sun"
    assert saved()["mode"] == "dark", "18:00 UTC in December is after Berlin's sunset"
    assert appearance["next"]["mode"] == "light"
    rise = [int(part) for part in appearance["sun"]["rise"].split(":")]
    assert (7, 5) <= tuple(rise) <= (7, 25), appearance["sun"]
    call("tick", now=at(2026, 12, 22, 10), tz="Europe/Berlin")
    assert saved()["mode"] == "light", "the sun rose"
    # A zone the table does not place cannot follow the sun.
    result = call("auto", "sun", ok=False, now=winter_night, tz="Etc/UTC")
    assert "no city" in result.stderr
    assert saved()["auto"]["source"] == "sun", "the refusal changed nothing"

    # The scheduler runs as a loop; one pass of it applies the schedule.
    call("mode", "dark", now=at(2026, 12, 22, 11), tz="Europe/Berlin")
    call("auto", "schedule", "07:00", "19:00", now=at(2026, 12, 22, 11))
    call("mode", "light", now=at(2026, 12, 22, 12))
    assert saved()["mode"] == "light" and shown() == "flexoki-light"
    env = dict(base, SEELE_THEME_NOW=str(at(2026, 12, 22, 19, 30)))
    follower = subprocess.Popen([binary, "follow"], env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    try:
        deadline = 50
        while saved()["mode"] != "dark" or shown() != "catppuccin-mocha":
            deadline -= 1
            assert deadline > 0, "follow applied the evening boundary"
            time.sleep(0.1)
    finally:
        follower.terminate()
        follower.wait(timeout=5)

    # Reset returns both slots, the mode and the schedule to the defaults.
    call("reset")
    assert saved() == {"version": 1, "dark": "catppuccin-mocha", "light": "catppuccin-latte", "mode": "dark",
                       "auto": {"source": "off", "lightAt": "07:00", "darkAt": "19:00", "last": 0}}

print("Light and dark slots, migration, mode, restore, fixed and solar schedules, manual holds, catch-up and follow passed")
