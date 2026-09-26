#!/usr/bin/env python3
"""Exercise the production resident clock's meeting planner against system tzdata."""
import calendar
import json
import os
import subprocess
import sys
import tempfile
import time

with tempfile.TemporaryDirectory() as work:
    env = dict(os.environ, XDG_STATE_HOME=work, TZ="Europe/Berlin")
    pins = ["America/New_York", "Asia/Kathmandu", "Australia/Lord_Howe"]
    for zone in pins:
        subprocess.run([sys.argv[1], "pin", zone], env=env, check=True)
    worker = subprocess.Popen([sys.argv[1], "watch"], env=env, stdin=subprocess.PIPE,
                              stdout=subprocess.PIPE, text=True)
    assert len(json.loads(worker.stdout.readline())["zones"]) > 200
    sequence = 0

    def utc(text):
        return calendar.timegm(time.strptime(text, "%Y-%m-%d %H:%M"))

    def request(meeting):
        global sequence
        sequence += 1
        worker.stdin.write(json.dumps({"requestId": sequence, "meeting": meeting}) + "\n")
        worker.stdin.flush()
        reply = json.loads(worker.stdout.readline())
        assert reply["requestId"] == sequence
        return reply

    def plan(**meeting):
        result = request(meeting)["meeting"]
        assert [row["id"] for row in result["rows"]] == ["Europe/Berlin", *pins]
        assert result["rows"][0]["home"] and result["rows"][0]["label"] == "Berlin"
        day = result["day"]
        assert day["start"] <= result["start"] < day["end"]
        assert (result["start"] - day["start"]) % 900 == 0
        for row in result["rows"]:
            cells = row["cells"]
            assert cells[0]["from"] == 0 and cells[-1]["to"] == day["minutes"]
            assert all(left["to"] == right["from"] for left, right in zip(cells, cells[1:]))
        return result

    def row(result, zone):
        return next(entry for entry in result["rows"] if entry["id"] == zone)

    def labels(entry):
        return [cell["label"] for cell in entry["cells"]]

    # The axis is Berlin's own day: 23 hours in spring, 25 in autumn.
    spring = plan(start=utc("2026-03-29 12:00"))
    assert spring["day"]["minutes"] == 1380 and "02" not in labels(spring["rows"][0])
    autumn = plan(start=utc("2026-10-25 12:00"))
    assert autumn["day"]["minutes"] == 1500 and labels(autumn["rows"][0]).count("02") == 2

    # Every row is read at the selected instant, across each zone's own shift.
    before = row(plan(start=utc("2026-03-08 06:45")), pins[0])
    after = row(plan(start=utc("2026-03-08 07:00")), pins[0])
    assert before["range"] == "01:45–03:45" and "EST UTC-5 → EDT UTC-4" in before["caption"]
    assert after["range"] == "03:00–04:00" and "EDT UTC-4" in after["caption"]
    first = row(plan(start=utc("2026-11-01 05:30"), duration=30), pins[0])
    second = row(plan(start=utc("2026-11-01 06:30"), duration=30), pins[0])
    # 01:30 happens twice; the first meeting ends at the second 01:00.
    assert first["range"] == "01:30–01:00" and "EDT UTC-4 → EST UTC-5" in first["caption"]
    assert second["range"] == "01:30–02:00" and "EST UTC-5" in second["caption"]
    howe = plan(start=utc("2026-10-03 15:15"), duration=30)
    assert row(howe, pins[2])["range"] == "01:45–02:45"
    assert "UTC+10:30 → UTC+11" in row(howe, pins[2])["caption"]
    # Lord Howe's half-hour shift leaves a half-hour cell on Berlin's axis.
    widths = [cell["to"] - cell["from"] for cell in row(howe, pins[2])["cells"]]
    assert 30 in widths, widths
    kathmandu = plan(start=utc("2026-09-22 09:00"))
    assert row(kathmandu, pins[1])["range"] == "14:45–15:45"
    assert row(kathmandu, pins[1])["caption"] == "Tue 22 Sep · UTC+5:45"
    assert row(kathmandu, pins[1])["fit"] == "work"
    assert row(plan(start=utc("2026-09-22 10:30")), pins[1])["note"] == "late"
    assert any(cell["to"] - cell["from"] == 15 for cell in row(kathmandu, pins[1])["cells"])

    # A day move keeps Berlin's wall time across its own transitions.
    moved = plan(start=utc("2026-03-28 09:00"), days=1)
    assert moved["start"] == utc("2026-03-29 08:00") and moved["range"] == "10:00–11:00"
    gap = plan(start=utc("2026-03-28 01:30"), days=1)
    assert gap["range"] == "03:00–04:00", gap["range"]
    fold = plan(start=utc("2026-10-24 00:30"), days=1)
    assert fold["start"] == utc("2026-10-25 00:30") and "CEST" in fold["zone"]
    assert plan(start=utc("2026-01-01 09:00"), date="2028-02-29")["day"]["date"] == "2028-02-29"
    # A step past the day's last quarter hour is the next day's first (SIL-93).
    last = plan(start=utc("2026-09-22 21:45"))
    assert last["range"] == "23:45–00:45"
    step = plan(start=last["start"] + 900)
    assert step["day"]["date"] == "2026-09-23" and step["start"] == step["day"]["start"]

    # Suggestions rank the day and steer around busy time.
    tuesday = plan(start=utc("2030-10-08 12:00"))  # future, so nothing is past
    assert 1 <= len(tuesday["suggestions"]) <= 3
    starts = [suggestion["start"] for suggestion in tuesday["suggestions"]]
    assert all(abs(a - b) >= 3600 for a in starts for b in starts if a != b)
    best = tuesday["suggestions"][0]
    busy = [[best["start"], best["start"] + 3600]]
    avoided = plan(start=best["start"], busy=busy)
    assert avoided["suggestions"][0]["start"] != best["start"]
    assert avoided["conflicts"] == busy
    assert avoided["next"] is None or avoided["next"]["start"] >= busy[0][1]

    # Now is the next quarter hour of this computer's day.
    now = plan()
    moment = time.time()
    assert moment - 60 < now["start"] < moment + 960 and now["day"]["today"]
    assert "UTC" in now["summary"] and "Kathmandu" in now["summary"]
    assert "ctz=Europe%2FBerlin" in now["calendarUrl"]
    assert all(suggestion["start"] >= now["start"] for suggestion in now["suggestions"])

    for bad in [{"start": utc("2026-01-01 00:00"), "date": "2026-02-30"},
                {"start": utc("2100-12-31 12:00")}, {"start": 0},
                {"start": utc("2026-01-01 00:00"), "days": 1000},
                {"start": utc("2026-01-01 00:00"), "duration": 999},
                {"start": utc("2026-01-01 00:00"), "busy": [[1, 2]] * 97},
                {"start": utc("2026-01-01 00:00"), "minute": 5}]:
        assert "meetingError" in request(bad), bad
    # An oversized line is drained, and the worker still answers after it.
    worker.stdin.write("x" * 100_000 + "\n")
    worker.stdin.flush()
    # A bad request doesn't kill the resident worker or poison TZ for live clocks.
    worker.stdin.write("refresh\n")
    worker.stdin.flush()
    snapshot = json.loads(worker.stdout.readline())
    assert snapshot["pinned"] == pins
    assert snapshot["local"]["time"] == next(row["time"] for row in snapshot["zones"] if row["id"] == "Europe/Berlin")
    # Unpin immediately changes the next projection; no stale cached participant.
    subprocess.run([sys.argv[1], "unpin", pins[-1]], env=env, check=True)
    assert len(request({"start": utc("2026-09-22 12:00")})["meeting"]["rows"]) == 3
    worker.stdin.close()
    assert worker.wait(timeout=5) == 0
print("meeting planner: local-day axis, real transitions, day moves, suggestions, busy time, validation and worker recovery passed")
