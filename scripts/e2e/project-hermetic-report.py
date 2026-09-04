#!/usr/bin/env python3
"""Project the measured stage report into the canonical reporting contract."""
from __future__ import annotations
import argparse,importlib.util,json,os,subprocess,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location("axon_e2e_projection_reporting",ROOT/"scripts/e2e/lib/reporting.py")
reporting=importlib.util.module_from_spec(spec);sys.modules[spec.name]=reporting;spec.loader.exec_module(reporting)
def main():
 p=argparse.ArgumentParser();p.add_argument("source",type=Path);p.add_argument("--out",type=Path,required=True);a=p.parse_args();source=json.loads(a.source.read_text());items=[]
 cleanup_ok=all(value and value.get("status")=="passed" for value in source.get("cleanup",{}).values())
 for stage in source.get("stages",[]):
  item=reporting.Scenario(f"hermetic.{stage['name']}","hermetic",stage["name"],"harness");status=stage["status"]
  item.attempt(status,int(stage.get("duration_ms",0)),classification=None if status=="passed" else "harness",summary=None if status=="passed" else f"measured stage {status}",namespace=f"axon_e2e_measured_{stage['name']}_attempt_1")
  item.cleanup={"success":cleanup_ok,"residual":[] if cleanup_ok else [{"class":"cleanup","identity":"measured"}]};items.append(item)
 sha=os.environ.get("GITHUB_SHA") or subprocess.run(["git","rev-parse","HEAD"],cwd=ROOT,capture_output=True,text=True,check=True).stdout.strip()
 policy={"suite_retry_budget":0,"source":"measured-hermetic-v1",
         "workflow_repository":os.environ.get("GITHUB_REPOSITORY"),"workflow_file":"e2e-hermetic.yml",
         "workflow_ref":os.environ.get("GITHUB_REF"),"workflow_run_id":os.environ.get("GITHUB_RUN_ID"),
         "workflow_run_attempt":int(os.environ["GITHUB_RUN_ATTEMPT"]) if os.environ.get("GITHUB_RUN_ATTEMPT","").isdigit() else None}
 report=reporting.suite_report(items,tested_sha=sha,provider_versions={"provider_mode":source.get("provider_mode","unknown")},policy=policy)
 reporting.write_json(report,a.out);return 0
if __name__=="__main__":raise SystemExit(main())
