#!/usr/bin/env python3
"""Compare equivalent offline input-report work; never fetch or evaluate Nix.

Usage: python3 tests/bench-helpers.py BASELINE_REPORT_PY NATIVE_INPUTS
The baseline must be the retained main@origin report or an extracted JJ file.
Both outputs must agree before timing. Results describe local process startup
plus one synthetic graph traversal with warm filesystem caches, not UI latency.
"""
import json
import pathlib
import random
import statistics
import subprocess
import sys
import tempfile
import time


def main():
    baseline, native = map(pathlib.Path, sys.argv[1:3])
    nodes = {"root": {"inputs": {f"input-{n}": f"node-{n}" for n in range(200)}}}
    for n in range(200):
        nodes[f"node-{n}"] = {
            "locked": {"type": "github", "owner": "fixture", "repo": f"repo-{n}",
                       "rev": f"{n:040x}", "lastModified": 1735689600},
            "original": {"type": "github", "owner": "fixture", "repo": f"repo-{n}"},
        }
    with tempfile.TemporaryDirectory(prefix="seele-helper-benchmark-") as directory:
        root = pathlib.Path(directory)
        lock = root / "flake.lock"
        lock.write_text(json.dumps({"version": 7, "root": "root", "nodes": nodes}))
        args = ["--lock-file", str(lock), "--all", "--json"]
        commands = {"python": [sys.executable, str(baseline), *args],
                    "rust": [str(native), *args]}
        outputs = {name: json.loads(subprocess.check_output(command, timeout=10))
                   for name, command in commands.items()}
        assert outputs["python"] == outputs["rust"], "benchmark output differs"
        samples = {name: [] for name in commands}
        rss = {name: [] for name in commands}
        randomizer = random.Random(20260912)
        for _ in range(40):
            order = list(commands)
            randomizer.shuffle(order)
            for name in order:
                usage = root / "usage"
                start = time.perf_counter_ns()
                subprocess.run(["/usr/bin/time", "-f", "%M", "-o", str(usage), *commands[name]],
                               stdout=subprocess.DEVNULL, check=True, timeout=10)
                samples[name].append((time.perf_counter_ns() - start) / 1_000_000)
                rss[name].append(int(usage.read_text()))
        print(json.dumps({"workload": "200-node offline lock graph, fresh process, warm filesystem cache",
                          "samples": 40, "output_equal": True,
                          "results": {name: {"median_ms": statistics.median(values),
                                             "p95_ms": sorted(values)[37],
                                             "median_peak_rss_kib": statistics.median(rss[name])}
                                      for name, values in samples.items()}}, indent=2))


if __name__ == "__main__":
    main()
