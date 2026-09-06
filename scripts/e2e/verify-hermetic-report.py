#!/usr/bin/env python3
import argparse,json
from pathlib import Path
parser=argparse.ArgumentParser();parser.add_argument("report",type=Path);parser.add_argument("--allow-failure",action="store_true");parser.add_argument("--expected-required",choices=("true","false"));args=parser.parse_args()
report=json.loads(args.report.read_text())
required={"schema","mode","required","network_policy","provider_mode","stage_gates","total_budget_seconds","duration_ms","stages","expected_stages","cleanup","evidence","resource_budgets","resource_observed","cleanup_contract","success","budget_exhausted"}
if not required.issubset(report): raise SystemExit("hermetic report is incomplete")
if not isinstance(report["required"],bool) or report["mode"]!="hermetic": raise SystemExit("hermetic promotion marker is malformed")
if args.expected_required is not None and report["required"] is not (args.expected_required=="true"): raise SystemExit("hermetic promotion marker disagrees with workflow policy")
if report["network_policy"]!="loopback-only" or report["provider_mode"]!="double" or report["stage_gates"] is not True: raise SystemExit("hermetic policy contract changed")
if report["cleanup_contract"]!="teardown-stages-plus-run-wide-residual-audit": raise SystemExit("cleanup contract changed")
canonical={"cpu_seconds":220,"memory_mib":4096,"processes":128,"ports":64,"shards":16,"retries":32,"artifacts":256}
if report["resource_budgets"] != canonical or set(report["resource_observed"]) != set(canonical): raise SystemExit("resource budget schema changed")
measurement=report.get("measurement",{})
if not isinstance(measurement.get("process_samples"),int) or measurement["process_samples"] < 1 or measurement.get("errors") != []: raise SystemExit("resource measurement failed closed")
if not report["success"] and not args.allow_failure: raise SystemExit("hermetic cleanup/audit contract failed")
if any(stage.get("duration_ms",0)>stage.get("budget_seconds",0)*1000 for stage in report["stages"]): raise SystemExit("stage budget exceeded")
actual=[stage.get("name") for stage in report["stages"]];expected=report["expected_stages"]
if report["success"] and actual != expected: raise SystemExit("hermetic stage set/order is incomplete")
if not report["success"] and any(name not in expected for name in actual): raise SystemExit("hermetic stage evidence is unknown")
if any(report["cleanup"].get(name,{}).get("status")!="passed" for name in ("teardown","isolation")): raise SystemExit("cleanup/audit evidence is absent")
if report["evidence"] != {"sanitized":True,"artifact_count":1}: raise SystemExit("sanitized evidence contract failed")
if any("stdout_tail" in stage or "stderr_tail" in stage or stage.get("sanitized") is not True for stage in report["stages"]): raise SystemExit("raw or unsanitized stage evidence found")
for key,limit in report["resource_budgets"].items():
    if key not in report["resource_observed"] or report["resource_observed"][key]>limit: raise SystemExit(f"resource budget exceeded: {key}")
