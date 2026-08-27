#!/usr/bin/env python3
"""Run one E2E child and unconditionally execute the authoritative teardown."""

from __future__ import annotations

import argparse
import importlib.util
import json
import os
import signal
import subprocess
import sys
from pathlib import Path


def _load():
    path = Path(__file__).with_name("teardown.py")
    spec = importlib.util.spec_from_file_location("axon_e2e_supervised_teardown", path)
    if spec is None or spec.loader is None: raise RuntimeError("teardown module unavailable")
    module = importlib.util.module_from_spec(spec); sys.modules[spec.name] = module; spec.loader.exec_module(module)
    return module


teardown = _load()


def supervise(manifest: Path, command: list[str], *, timeout: float, provider_config: Path | None = None,
              qdrant_url: str | None = None) -> dict:
    if not command: raise ValueError("a child command is required")
    # Ownership is provisioned and read back before the child can issue its
    # first query/upsert. The caller must create the empty isolated collection
    # before entering this supervisor; an absent collection fails setup closed.
    header, resources = teardown.manifest_api.load(manifest); provisioning = []
    if qdrant_url:
        qdrant = teardown.provider_api.QdrantAdapter({"base_url": qdrant_url}).bind(header, teardown.manifest_api)
        qdrant_types = {"collection", "qdrant_alias", "qdrant_snapshot", "point", "payload_index"}
        provisioning = [qdrant.provision_ownership_marker(item) for item in resources if item.resource_type in qdrant_types]
        if not provisioning: raise RuntimeError("no Qdrant collection ownership marker was provisioned")
    child = subprocess.Popen(command, start_new_session=(os.name != "nt"))
    interrupted: list[int] = []
    previous = {}

    def stop(signum, _frame):
        interrupted.append(signum)
        try:
            if os.name == "nt": child.terminate()
            else: os.killpg(child.pid, signal.SIGTERM)
        except ProcessLookupError: pass

    for sig in (signal.SIGINT, signal.SIGTERM):
        previous[sig] = signal.signal(sig, stop)
    timed_out = False
    try:
        try: returncode = child.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            timed_out = True; stop(signal.SIGTERM, None)
            try: returncode = child.wait(timeout=2)
            except subprocess.TimeoutExpired:
                if os.name == "nt": child.kill()
                else: os.killpg(child.pid, signal.SIGKILL)
                returncode = child.wait(timeout=2)
    finally:
        for sig, handler in previous.items(): signal.signal(sig, handler)
    # Re-open only after the child exits so resources registered at every setup
    # stage are included, including a final append immediately before a crash.
    header, _ = teardown.manifest_api.load(manifest)
    adapters = teardown.provider_api.build(provider_config, header, teardown.manifest_api) if provider_config else None
    cleanup = teardown.Engine(manifest, adapters).run().json()
    return {"success": cleanup["success"] and returncode == 0 and not interrupted and not timed_out,
            "child_returncode": returncode, "signal": interrupted[-1] if interrupted else None,
            "timed_out": timed_out, "qdrant_ownership": provisioning, "cleanup": cleanup}


def main() -> int:
    parser = argparse.ArgumentParser(); parser.add_argument("manifest", type=Path); parser.add_argument("--report", type=Path, required=True)
    parser.add_argument("--provider-config", type=Path); parser.add_argument("--timeout", type=float, default=900)
    parser.add_argument("--qdrant-url")
    parser.add_argument("command", nargs=argparse.REMAINDER); args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ["--"] else args.command
    try: report = supervise(args.manifest, command, timeout=args.timeout, provider_config=args.provider_config,
                            qdrant_url=args.qdrant_url)
    except Exception as error: report = {"success": False, "fatal": str(error)}
    args.report.parent.mkdir(parents=True, exist_ok=True); args.report.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    return 0 if report.get("success") else 2


if __name__ == "__main__": raise SystemExit(main())
