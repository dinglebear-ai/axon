#!/usr/bin/env python3
"""Trusted-live entry for the production observability oracle contract."""
from __future__ import annotations

import argparse
import importlib.util
import json
import os
import shutil
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
SPEC = importlib.util.spec_from_file_location(
    "axon_e2e_live_observability", ROOT / "tests/e2e/hermetic/real_composed.py"
)
assert SPEC and SPEC.loader
composed = importlib.util.module_from_spec(SPEC); sys.modules[SPEC.name] = composed; SPEC.loader.exec_module(composed)


def main() -> int:
    parser = argparse.ArgumentParser(); parser.add_argument("--launcher-descriptor", type=Path, required=True)
    args = parser.parse_args()
    if os.environ.get("AXON_E2E_TRUSTED_LIVE") != "1":
        raise RuntimeError("live observability requires AXON_E2E_TRUSTED_LIVE=1")
    descriptor = json.loads(args.launcher_descriptor.read_text())
    env = {**os.environ, **descriptor["environment"], "AXON_E2E_TIER": "live",
           "AXON_E2E_RUN_ID": descriptor["run_id"]}
    binary = Path(descriptor["binary"]); mcporter = Path(shutil.which("mcporter") or "")
    if not binary.is_file() or not mcporter.is_file(): raise RuntimeError("live Axon binary and mcporter are required")
    result = composed.verify_observability(binary, mcporter, descriptor, env, descriptor["run_id"])
    print(json.dumps({"status": "passed", "tier": "live", "observability": result}, sort_keys=True))
    return 0


if __name__ == "__main__": raise SystemExit(main())
