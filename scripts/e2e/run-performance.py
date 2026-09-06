#!/usr/bin/env python3
"""Validate and serialize bounded Axon E2E performance observations."""
from __future__ import annotations
import argparse,importlib.util,json,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location("axon_e2e_performance",ROOT/"scripts/e2e/lib/performance-report.py")
assert spec and spec.loader
performance=importlib.util.module_from_spec(spec);sys.modules[spec.name]=performance;spec.loader.exec_module(performance)
def main():
 parser=argparse.ArgumentParser();parser.add_argument("--input",type=Path,required=True);parser.add_argument("--out",type=Path,required=True)
 parser.add_argument("--baseline",type=Path);parser.add_argument("--budgets",type=Path,default=ROOT/"config/e2e/performance-budgets.json")
 args=parser.parse_args();report=json.loads(args.input.read_text());budgets=json.loads(args.budgets.read_text())
 performance.validate_budgets(budgets);comparison=None
 if args.baseline:comparison=performance.compare(report,json.loads(args.baseline.read_text()),budgets)
 performance.write_report(args.out,report)
 print(json.dumps(performance.release_projection(report,comparison),sort_keys=True));return 0
if __name__=="__main__":raise SystemExit(main())
