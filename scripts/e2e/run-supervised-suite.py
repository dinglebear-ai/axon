#!/usr/bin/env python3
"""Execute a plan through supervised teardown and emit the canonical report."""
from __future__ import annotations

import argparse
import importlib.util
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("axon_e2e_suite_supervisor", ROOT / "scripts/e2e/lib/run-with-teardown.py")
if spec is None or spec.loader is None: raise RuntimeError("supervisor unavailable")
supervisor = importlib.util.module_from_spec(spec); sys.modules[spec.name] = supervisor; spec.loader.exec_module(supervisor)
governance_spec = importlib.util.spec_from_file_location("axon_e2e_suite_governance", ROOT / "scripts/e2e/lib/flake-governance.py")
if governance_spec is None or governance_spec.loader is None: raise RuntimeError("flake governance unavailable")
governance = importlib.util.module_from_spec(governance_spec); governance_spec.loader.exec_module(governance)


def main() -> int:
    parser = argparse.ArgumentParser(); parser.add_argument("plan", type=Path); parser.add_argument("--report", type=Path, required=True)
    parser.add_argument("--junit", type=Path, required=True);parser.add_argument("--reliability-out",type=Path)
    parser.add_argument("--quarantine",type=Path,default=ROOT/"config/e2e/quarantine.json");args = parser.parse_args()
    plan = json.loads(args.plan.read_text())
    report = supervisor.supervise_suite(plan["scenarios"], tested_sha=plan["tested_sha"],
                                        provider_versions=plan.get("provider_versions", {}), policy=plan.get("policy", {}))
    supervisor.reporting.write_json(report, args.report); supervisor.reporting.write_junit(report, args.junit)
    reliability=governance.govern(report,json.loads((ROOT/"tests/e2e/catalog/catalog.json").read_text()),json.loads(args.quarantine.read_text()),environment=plan.get("environment","local"))
    reliability_out=args.reliability_out or args.report.with_name("reliability.json")
    reliability_out.write_text(json.dumps(reliability,indent=2,sort_keys=True)+"\n")
    print(json.dumps({"status": report["summary"]["status"], "report": str(args.report), "junit": str(args.junit),"reliability":str(reliability_out)}, sort_keys=True))
    return 0 if report["summary"]["status"] == "passed" and not reliability["escalations"] else 2


if __name__ == "__main__": raise SystemExit(main())
