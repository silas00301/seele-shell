#!/usr/bin/env python3
"""Real worker against a synthetic hwmon class; no host sensor is read."""
import json
import os
from pathlib import Path
import selectors
import shutil
import subprocess
import sys
import tempfile
import time

binary = str(Path(sys.argv[1]).resolve())
with tempfile.TemporaryDirectory() as temp:
    root = Path(temp) / "hwmon"
    devices = Path(temp) / "devices"
    root.mkdir()

    def write(path, value):
        path.parent.mkdir(parents=True, exist_ok=True)
        new = path.with_name(path.name + ".new")
        new.write_text(str(value))
        new.replace(path)

    def hwmon(name, chip, device=None, link=False):
        # Real /sys/class/hwmon entries are symlinks into /sys/devices.
        if link:
            target = devices / "virtual" / name
            target.mkdir(parents=True)
            (root / name).symlink_to(target)
        path = root / name
        write(path / "name", chip)
        if device is not None:
            device.mkdir(parents=True, exist_ok=True)
            (path / "device").symlink_to(device)
        return path

    cpu = hwmon("hwmon2", "k10temp", devices / "pci0000:00/0000:00:18.3", link=True)
    write(cpu / "temp1_input", 48250)
    write(cpu / "temp1_label", "Tctl")
    write(cpu / "temp3_input", 44000)
    write(cpu / "temp3_label", "Tccd1")
    nvme_device = devices / "pci0000:00/0000:01:00.0/nvme/nvme0"
    write(nvme_device / "model", "Example NVMe 2TB‮\n")
    drive = hwmon("hwmon1", "nvme", nvme_device)
    write(drive / "temp1_input", 39850)
    write(drive / "temp1_label", "Composite")
    write(drive / "temp1_max", 81850)
    write(drive / "temp1_crit", 84850)
    write(drive / "temp1_min", -273150)
    board = hwmon("hwmon4", "nct6798")
    write(board / "fan1_input", 0)
    write(board / "fan2_input", 1180)
    write(board / "fan2_min", 300)
    (board / "temp7_input").mkdir()  # a read error, like ENODATA
    disk = hwmon("hwmon5", "drivetemp")
    write(disk / "temp1_input", 31000)

    env = {**os.environ, "SEELE_SENSORS_SYSFS": str(root)}
    worker = subprocess.Popen([binary], env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    selector = selectors.DefaultSelector()
    selector.register(worker.stdout, selectors.EVENT_READ)

    def snapshot():
        assert selector.select(timeout=5), "worker stopped responding"
        line = worker.stdout.readline()
        assert line, worker.stderr.read().decode()
        value = json.loads(line)
        assert value["version"] == 1 and value["cadenceSeconds"] == 2
        assert "‮" not in line.decode()
        return value

    def rows(value):
        return {row["title"]: row for row in value["rows"]}

    first = snapshot()
    assert first["error"] == "" and first["skipped"] == 1 and not first["limited"]
    assert [row["title"] for row in first["rows"]] == ["CPU", "Mainboard", "Example NVMe 2TB"], first
    by = rows(first)
    assert by["CPU"]["detail"] == "k10temp · 0000:00:18.3"
    assert [r["label"] for r in by["CPU"]["readings"]] == ["Tctl", "Tccd1"]
    tctl = by["CPU"]["readings"][0]
    assert tctl["value"] == "48.3 °C" and tctl["peak"] == "48.3 °C" and tctl["limits"] == "" and tctl["ratio"] is None
    composite = by["Example NVMe 2TB"]["readings"][0]
    assert composite["limits"] == "High 81.9 °C · critical 84.9 °C" and composite["state"] == "normal"
    assert abs(composite["ratio"] - 39850 / 84850) < 1e-9
    fans = by["Mainboard"]["readings"]
    assert [(r["label"], r["value"], r["status"]) for r in fans] == [
        ("Temperature 7", "—", "No reading"), ("Fan 1", "0 RPM", "Stopped"), ("Fan 2", "1180 RPM", "")]
    assert first["summary"] == "Hottest 48.3 °C · CPU Tctl" and first["attention"] == 0
    cpu_id = by["CPU"]["id"]

    # Peaks rise and hold; a limit crossed is named by the reading's own state.
    write(cpu / "temp1_input", 71500)
    write(drive / "temp1_input", 83000)
    hot = rows(snapshot())
    assert hot["CPU"]["readings"][0]["peak"] == "71.5 °C"
    assert hot["Example NVMe 2TB"]["readings"][0]["state"] == "high"
    write(cpu / "temp1_input", 52000)
    write(drive / "temp1_input", 40000)
    cooled = snapshot()
    assert rows(cooled)["CPU"]["readings"][0]["value"] == "52 °C"
    assert rows(cooled)["CPU"]["readings"][0]["peak"] == "71.5 °C"
    assert rows(cooled)["CPU"]["id"] == cpu_id and cooled["elapsed"] >= 2

    # Reset starts a new session at once rather than at the next tick.
    started = time.monotonic()
    worker.stdin.write(b'{"op":"reset"}\n')
    worker.stdin.flush()
    reset = snapshot()
    assert time.monotonic() - started < 1.5
    assert reset["elapsed"] == 0 and rows(reset)["CPU"]["readings"][0]["peak"] == "52 °C"

    # The driver rebinds under a new hwmon number: same device, fresh peaks.
    write(cpu / "temp1_input", 90000)
    snapshot()
    # The new directory exists before the old one goes, as kernfs's cyclic
    # inode numbers guarantee, so the fixture cannot recycle an inode.
    rebound = hwmon("hwmon9", "k10temp", devices / "pci0000:00/0000:00:18.3", link=True)
    write(rebound / "temp1_input", 45000)
    shutil.rmtree(devices / "virtual" / "hwmon2")
    (root / "hwmon2").unlink()
    after = rows(snapshot())
    assert after["CPU"]["id"] == cpu_id
    assert after["CPU"]["readings"][0]["peak"] == "45 °C", after["CPU"]

    hidden = root.with_name("hidden")
    root.rename(hidden)
    missing = snapshot()
    assert missing["rows"] == [] and missing["error"] == "The kernel's sensor interface is unavailable"
    hidden.rename(root)
    assert len(snapshot()["rows"]) == 3

    worker.stdin.close()
    assert worker.wait(timeout=3) == 0
    assert worker.stderr.read() == b""

    # Oversized input is bounded and ends the session; nothing accumulates.
    huge = subprocess.run([binary], input=b"x" * 1025, env=env, capture_output=True, timeout=5)
    assert huge.returncode == 0 and len(huge.stdout.splitlines()) == 1
    assert subprocess.run([binary, "extra"], env=env, capture_output=True, timeout=5).returncode != 0
print("sensors: real worker names, limits, peaks, reset, rebind, drivetemp, unavailable root and EOF passed")
