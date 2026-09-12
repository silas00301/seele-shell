#!/usr/bin/env python3
"""Verify Qt formatting against reviewed main@origin semantic fixtures."""
import json
import os
import pathlib
import subprocess
import sys

fixture = json.loads(pathlib.Path(sys.argv[2]).read_text())
run = subprocess.run([sys.argv[1]], input=json.dumps(fixture["documents"]).encode() + b"\n",
                     capture_output=True, check=True, timeout=30,
                     env={**os.environ, "QT_QPA_PLATFORM": "offscreen"})
actual = json.loads(run.stdout)
assert actual == fixture["formats"], "Qt character formats or block states differ from the reviewed baseline"
print(f"Qt Markdown format parity: {len(actual)} documents")
