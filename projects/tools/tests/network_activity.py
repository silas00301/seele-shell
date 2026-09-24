#!/usr/bin/env python3
"""Real worker against synthetic kernel counters; no host traffic or services."""
import json
import os
from pathlib import Path
import selectors
import shutil
import subprocess
import sys
import tempfile

binary = str(Path(sys.argv[1]).resolve())
with tempfile.TemporaryDirectory() as temp:
    root = Path(temp) / "net"
    root.mkdir()
    def write(path, value):
        new = path.with_name(path.name + ".new")
        new.write_text(str(value))
        new.replace(path)
    def interface(name, index, rx, tx):
        p = root / name
        (p / "statistics").mkdir(parents=True, exist_ok=True)
        write(p / "ifindex", index)
        write(p / "operstate", "up")
        write(p / "statistics/rx_bytes", rx)
        write(p / "statistics/tx_bytes", tx)
        return p
    p = interface("test0", 2, 1000, 2000)
    worker = subprocess.Popen([binary], env={**os.environ, "SEELE_NETWORK_ACTIVITY_SYSFS": str(root)}, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    selector = selectors.DefaultSelector()
    selector.register(worker.stdout, selectors.EVENT_READ)
    def snapshot():
        assert selector.select(timeout=5), "worker stopped responding"
        line = worker.stdout.readline()
        assert line, worker.stderr.read().decode()
        value = json.loads(line)
        assert value["version"] == 1
        assert "address" not in line.decode().lower()
        return value
    initial = snapshot()
    assert initial["interfaceLimit"] == 256 and initial["historyCapacity"] == 60
    first = initial["rows"][0]
    assert first["rxRate"] is None and first["rxTotal"] == "0 B"
    identity = first["id"]
    interface("test0", 2, 3048, 3024)
    live = snapshot()["rows"][0]
    assert 1500 < live["rxRate"] < 2300 and live["rxTotal"] == "2.0 KiB", live
    assert 750 < live["txRate"] < 1150 and live["txTotal"] == "1.0 KiB"
    assert live["id"] == identity and live["rx"][0] is None
    # Explicit Reset resets every baseline and history, not the kernel counters.
    worker.stdin.write(b'{"op":"reset"}\n'); worker.stdin.flush()
    reset = snapshot()
    assert reset["elapsed"] == 0 and reset["rows"][0]["rx"] == [None]
    assert reset["rows"][0]["rxTotal"] == "0 B"
    interface("test0", 2, 1, 2)
    restarted = snapshot()["rows"][0]
    assert restarted["incomplete"] and restarted["rxRate"] is None
    (p / "statistics/rx_bytes").unlink()
    missing = snapshot()["rows"][0]
    assert missing["status"] == "Counters unavailable" and missing["rxRate"] is None
    interface("test0", 2, 500, 1000)
    assert snapshot()["rows"][0]["rxRate"] is None
    shutil.rmtree(p)
    assert snapshot()["rows"] == []
    interface("test0", 2, 999999, 999999)
    hotplug = snapshot()["rows"][0]
    assert hotplug["rxRate"] is None and hotplug["rxTotal"] == "0 B"
    interface("test1", 3, 888, 777)
    assert len(snapshot()["rows"]) == 2
    saved = root.with_name("hidden")
    root.rename(saved)
    assert snapshot()["error"] == "Interface counters are unavailable"
    saved.rename(root)
    assert all(row["rxRate"] is None for row in snapshot()["rows"])
    worker.stdin.close()
    assert worker.wait(timeout=3) == 0
    assert worker.stderr.read() == b""
    # Oversized input is bounded and terminates; a sender cannot allocate freely.
    huge = subprocess.run([binary], input=b"x" * 1025, env={**os.environ, "SEELE_NETWORK_ACTIVITY_SYSFS": str(root)}, capture_output=True, timeout=3)
    assert huge.returncode == 0
    assert len(huge.stdout.splitlines()) == 1
print("network activity: real worker rates, reset, unreadable counters, hotplug, discovery failure and EOF passed")
