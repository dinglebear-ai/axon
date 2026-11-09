"""Exercise launcher subprocess boundaries without privilege or live providers."""
from __future__ import annotations
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[3]
LAUNCHER = ROOT / "scripts/e2e/run-hermetic-local.sh"
FAKE = r'''
import json, os, pathlib, sys
name = pathlib.Path(sys.argv[0]).name
args = sys.argv[1:]
with open(os.environ["LAUNCHER_TRACE"], "a") as out:
    out.write(json.dumps([name, args]) + "\n")
if name == "id":
    print(os.environ["LAUNCHER_UID"] if args == ["-u"] else "invoker")
elif name == "uname":
    print("Linux")
elif name == "sudo":
    assert args.pop(0) == "--preserve-env=PATH"
    os.environ["SUDO_USER"] = "nested-sudo-must-not-select-this-user"
    os.execvpe(args[0], args, os.environ)
elif name == "unshare":
    assert args[:2] == ["--net", "--fork"]
    os.execvpe(args[2], args[2:], os.environ)
elif name == "runuser":
    assert args[:3] == ["-u", "invoker", "--"]
    os.environ["USER"] = "invoker"
    os.environ["HOME"] = "/fixture/invoker"
    os.execvpe(args[3], args[3:], os.environ)
elif name == "just":
    assert args == ["e2e-hermetic-inner"]
    assert os.environ["AXON_E2E_NATIVE_ISOLATION"] == "1"
    assert os.environ["USER"] == "invoker"
    assert os.environ["HOME"] == "/fixture/invoker"
elif name == "ip":
    assert args == ["link", "set", "lo", "up"]
elif name == "cargo":
    assert args == ["build", "--locked", "--bin", "axon"]
'''


class LocalLauncherTests(unittest.TestCase):
    def launch(self, uid):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            trace = root / "trace.jsonl"
            for name in ("id", "uname", "cargo", "sudo", "unshare", "ip", "runuser", "just"):
                command = root / name
                command.write_text(f"#!{sys.executable}\n" + FAKE)
                command.chmod(0o700)
            env = {**os.environ, "PATH": f"{root}:/usr/bin:/bin",
                   "LAUNCHER_TRACE": str(trace), "LAUNCHER_UID": str(uid),
                   "SUDO_USER": "stale-parent-user"}
            env.pop("OPENSSL_DIR", None)
            result = subprocess.run(["/bin/bash", str(LAUNCHER)], cwd=ROOT, env=env,
                                    text=True, capture_output=True, timeout=10)
            calls = [json.loads(line) for line in trace.read_text().splitlines()]
            return result, calls

    def test_network_setup_precedes_drop_to_original_invoker(self):
        result, calls = self.launch(1000)
        self.assertEqual(0, result.returncode, result.stderr)
        names = [name for name, _ in calls]
        self.assertLess(names.index("cargo"), names.index("sudo"))
        self.assertLess(names.index("ip"), names.index("runuser"))
        self.assertLess(names.index("runuser"), names.index("just"))
        sudo = next(args for name, args in calls if name == "sudo")
        self.assertEqual("invoker", sudo[-1])
        self.assertIn('runuser -u "$1" -- env PATH="$PATH"', sudo[-3])

    def test_root_is_rejected_before_build_or_escalation(self):
        result, calls = self.launch(0)
        self.assertEqual(2, result.returncode)
        self.assertIn("non-root user", result.stderr)
        self.assertEqual(["id"], [name for name, _ in calls])
