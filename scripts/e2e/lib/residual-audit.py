#!/usr/bin/env python3
"""Exact-identity residual audit for an Axon E2E ownership manifest."""

from __future__ import annotations

import argparse
import importlib.util
import json
import sys
from pathlib import Path


def _load():
    path = Path(__file__).with_name("teardown.py")
    spec = importlib.util.spec_from_file_location("axon_e2e_teardown", path)
    if spec is None or spec.loader is None: raise RuntimeError("teardown module unavailable")
    module = importlib.util.module_from_spec(spec); sys.modules[spec.name] = module; spec.loader.exec_module(module)
    return module


teardown = _load()


def main() -> int:
    parser = argparse.ArgumentParser(); parser.add_argument("manifest", type=Path)
    parser.add_argument("--report", type=Path, required=True); args = parser.parse_args()
    engine = teardown.Engine(args.manifest)
    engine.audit(); report = engine.report.json()
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    return 0 if report["success"] else 2


if __name__ == "__main__": raise SystemExit(main())
