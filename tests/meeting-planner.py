#!/usr/bin/env python3
"""Exercise the production resident clock against system IANA timezone data."""
import datetime
import json
import os
import subprocess
import sys
import tempfile

with tempfile.TemporaryDirectory() as work:
    env = dict(os.environ, XDG_STATE_HOME=work, TZ="Europe/Berlin")
    pins = ["America/New_York", "Asia/Kathmandu", "Australia/Lord_Howe"]
    for zone in pins:
        subprocess.run([sys.argv[1], "pin", zone], env=env, check=True)
    worker = subprocess.Popen([sys.argv[1], "watch"], env=env, stdin=subprocess.PIPE,
                              stdout=subprocess.PIPE, text=True)
    assert len(json.loads(worker.stdout.readline())["zones"]) > 200
    sequence = 0

    def request(date, minute=0, duration=60, **extra):
        global sequence
        sequence += 1
        worker.stdin.write(json.dumps({"requestId": sequence, "meeting": dict(
            date=date, minute=minute, duration=duration, **extra)}) + "\n")
        worker.stdin.flush()
        reply = json.loads(worker.stdout.readline())
        assert reply["requestId"] == sequence
        return reply

    def plan(date, minute=0, duration=60, **extra):
        result = request(date, minute, duration, **extra)["meeting"]
        assert len(result["rows"]) == len(pins) + 1
        assert len(result["overlap"]) == 96
        for row in result["rows"]:
            assert len(row["slots"]) == 96
        assert result["allWorking"] == all(row["working"] for row in result["rows"])
        assert result["overlap"] == [all(row["slots"][i] for row in result["rows"]) for i in range(96)]
        return result

    def zone(plan, id):
        return next(row for row in plan["rows"] if row["id"] == id)

    before = zone(plan("2026-03-08", 6 * 60 + 45), pins[0])
    after = zone(plan("2026-03-08", 7 * 60), pins[0])
    assert (before["time"], before["offset"]) == ("01:45", "UTC-05:00")
    assert (after["time"], after["offset"]) == ("03:00", "UTC-04:00")
    assert "UTC-04:00" in before["end"]  # summary names the transition at the end
    first = zone(plan("2026-11-01", 5 * 60 + 30), pins[0])
    second = zone(plan("2026-11-01", 6 * 60 + 30), pins[0])
    assert first["time"] == second["time"] == "01:30"
    assert first["offset"] == "UTC-04:00" and second["offset"] == "UTC-05:00"
    first = plan("2026-10-25", 30)["rows"][0]
    second = plan("2026-10-25", 90)["rows"][0]
    assert first["time"] == second["time"] == "02:30"
    assert first["offset"] == "UTC+02:00" and second["offset"] == "UTC+01:00"
    first = zone(plan("2026-10-03", 15 * 60 + 15), pins[2])
    second = zone(plan("2026-10-03", 15 * 60 + 30), pins[2])
    assert first["time"] == "01:45" and second["time"] == "02:30"
    assert first["offset"] == "UTC+10:30" and second["offset"] == "UTC+11:00"
    kathmandu = zone(plan("2026-09-22", 9 * 60), pins[1])
    assert (kathmandu["time"], kathmandu["offset"]) == ("14:45", "UTC+05:45")
    assert kathmandu["working"]
    assert kathmandu["slots"][41] and not kathmandu["slots"][42]
    assert not zone(plan("2026-09-22", 10 * 60 + 15, 90), pins[1])["working"]
    assert zone(plan("2026-09-22", 10 * 60 + 15, 60), pins[1])["working"]  # ends exactly 17:00
    assert not zone(plan("2026-09-22", 10 * 60 + 30, 60), pins[1])["working"]
    midnight = zone(plan("2026-09-22", 23 * 60 + 45), pins[1])
    assert midnight["date"] == "2026-09-23"
    weekend = plan("2026-09-26", 12 * 60)
    assert not any(weekend["overlap"])
    assert plan("2026-01-01", 0, shift=-1)["date"] == "2025-12-31"
    assert plan("2028-02-28", 0, shift=1)["date"] == "2028-02-29"
    now = plan("")
    assert abs(now["epoch"] - datetime.datetime.now(datetime.timezone.utc).timestamp()) < 61
    assert "UTC" in now["summary"] and "America/New_York" in now["summary"]
    for bad in [{"date": "2026-02-30"}, {"date": "2100-12-31", "shift": 1},
                {"date": "2026-01-01", "minute": -1}, {"date": "2026-01-01", "minute": 1440},
                {"date": "2026-01-01", "duration": 999}, {"date": "2026-01-01", "shift": 100}]:
        assert "meetingError" in request(**bad)
    # A bad request doesn't kill the resident worker or poison TZ for live clocks.
    worker.stdin.write("refresh\n")
    worker.stdin.flush()
    snapshot = json.loads(worker.stdout.readline())
    assert snapshot["pinned"] == pins
    assert snapshot["local"]["time"] == next(row["time"] for row in snapshot["zones"] if row["id"] == "Europe/Berlin")
    # Unpin immediately changes the next projection; no stale cached participant.
    subprocess.run([sys.argv[1], "unpin", pins[-1]], env=env, check=True)
    assert len(request("2026-09-22")["meeting"]["rows"]) == 3
    worker.stdin.close()
    assert worker.wait(timeout=5) == 0
print("meeting planner: real timezone transitions, full-duration coverage, validation and worker recovery passed")
