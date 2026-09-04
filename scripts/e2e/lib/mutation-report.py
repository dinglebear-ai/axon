#!/usr/bin/env python3
"""Project mutation outcomes into the sole canonical E2E report schema."""
import datetime as dt,importlib.util,json,os,sys,tempfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
spec=importlib.util.spec_from_file_location("axon_mutation_canonical_reporting",ROOT/"scripts/e2e/lib/reporting.py");canonical=importlib.util.module_from_spec(spec);sys.modules[spec.name]=canonical;spec.loader.exec_module(canonical)
class MutationReportError(RuntimeError):pass
def exceptions(path):
 body=json.loads(path.read_text());today=dt.date.today();valid={}
 for item in body.get("tracked_defects",[]):
  if not item.get("mutant") or not item.get("tracker"):raise MutationReportError("tracked survivor defect requires mutant and tracker")
  valid[item["mutant"]]={"owner":"tracked-defect","expires":"n/a","tracker":item["tracker"]}
 for item in body.get("exceptions",[]):
  if not item.get("mutant") or not item.get("owner") or not item.get("reason"):raise MutationReportError("mutation exception must have mutant, owner, and reason")
  try:expiry=dt.date.fromisoformat(item["expires"])
  except (KeyError,ValueError):raise MutationReportError("mutation exception expiry is invalid")
  if expiry<today:raise MutationReportError(f"mutation exception expired: {item['mutant']}")
  valid[item["mutant"]]=item
 return valid
def build(results,*,policy,duration_ms,exceptions_path):
 allowed=exceptions(exceptions_path);scenarios=[];survivors=[]
 for item in sorted(results,key=lambda x:x["mutant"]):
  outcome=item["outcome"];exception=allowed.get(item["mutant"])
  if outcome=="survived" and exception is None:survivors.append(item["mutant"])
  status="passed" if outcome=="killed" or (outcome=="survived" and exception) else "failed";scenario=canonical.Scenario("mutation."+item["mutant"],"hermetic","mutation-sensitivity","oracle")
  scenario.attempt(status,item["runtime_ms"],classification=None if status=="passed" else "harness",summary=None if status=="passed" else outcome);scenario.cleanup={"success":True,"residual":[],"refused":[]}
  detail={key:item[key] for key in ("mutant","codepath","scenario","invariant","outcome","runtime_ms","shard","baseline_passed","restoration_passed")}
  if exception:detail["exception"]={"owner":exception["owner"],"expires":exception["expires"]}
  scenario.invariants=[detail];scenarios.append(scenario)
 policy={**policy,"mutation_score_percent":round(100*sum(x["outcome"]=="killed" for x in results)/len(results),1),"mutation_exceptions":len(allowed),"parallel_wall_duration_ms":duration_ms,"summed_mutant_runtime_ms":sum(x["runtime_ms"] for x in results),"unowned_survivors":survivors}
 return canonical.suite_report(scenarios,tested_sha="0"*40,provider_versions={"axon":"workspace"},policy=policy)
def validate(report):canonical.validate_report(report)
def write(report,path):
 """Atomically replace stale evidence only after canonical validation."""
 path.parent.mkdir(parents=True,exist_ok=True)
 with tempfile.NamedTemporaryFile("w",encoding="utf-8",dir=path.parent,prefix=path.name+".",suffix=".tmp",delete=False) as handle:temporary=Path(handle.name)
 try:
  canonical.write_json(report,temporary);os.replace(temporary,path)
 finally:
  temporary.unlink(missing_ok=True)
