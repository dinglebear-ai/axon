#!/usr/bin/env python3
"""Spawn a descendant and stay alive so platform tree teardown can prove ownership."""
import json
import subprocess
import sys
import time

print(json.dumps({"ready": True}), flush=True)
if not sys.stdin.readline():
    raise SystemExit("missing spawn signal")
child = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(300)"])
print(json.dumps({"child_pid": child.pid}), flush=True)
time.sleep(300)
