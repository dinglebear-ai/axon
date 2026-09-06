#!/usr/bin/env python3
from __future__ import annotations
import argparse,importlib.util,json,sys
from pathlib import Path

ROOT=Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location("axon_e2e_flake_governance",ROOT/"scripts/e2e/lib/flake-governance.py")
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)

def main()->int:
    parser=argparse.ArgumentParser();parser.add_argument("--report",type=Path,required=True);parser.add_argument("--catalog",type=Path,default=ROOT/"tests/e2e/catalog/catalog.json")
    parser.add_argument("--quarantine",type=Path,default=ROOT/"config/e2e/quarantine.json");parser.add_argument("--history",type=Path)
    parser.add_argument("--environment",required=True);parser.add_argument("--reliability-out",type=Path,required=True);parser.add_argument("--history-out",type=Path)
    parser.add_argument("--repository",default="dinglebear-ai/axon");parser.add_argument("--workflow",default="local");parser.add_argument("--trusted-ref",default="refs/heads/main");args=parser.parse_args()
    try:
        history=(module.validate_history(json.loads(args.history.read_text()),repository=args.repository,workflow=args.workflow,trusted_ref=args.trusted_ref) if args.history else [])
        current=json.loads(args.report.read_text())
        result=module.govern(current,json.loads(args.catalog.read_text()),json.loads(args.quarantine.read_text()),environment=args.environment,history=history)
        args.reliability_out.parent.mkdir(parents=True,exist_ok=True);args.reliability_out.write_text(json.dumps(result,indent=2,sort_keys=True)+"\n")
        if args.history_out:
            args.history_out.parent.mkdir(parents=True,exist_ok=True);args.history_out.write_text(json.dumps(module.history_envelope([*history,current],repository=args.repository,workflow=args.workflow,trusted_ref=args.trusted_ref),indent=2,sort_keys=True)+"\n")
        if any(not item["tracked"] for item in result["escalations"]):raise module.GovernanceError("recurrent failures require tracked defects")
        print(json.dumps({"status":"passed","reliability":str(args.reliability_out)},sort_keys=True));return 0
    except (OSError,ValueError,json.JSONDecodeError,module.GovernanceError) as error:
        print(f"flake governance error: {error}",file=sys.stderr);return 2

if __name__=="__main__":raise SystemExit(main())
