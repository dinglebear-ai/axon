#!/usr/bin/env python3
"""Fail-closed promotion preflight for the stable hermetic required check."""
from __future__ import annotations
import argparse,hashlib,importlib.util,json,math,re,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
REPORT_SPEC=importlib.util.spec_from_file_location("axon_required_gate_reporting",ROOT/"scripts/e2e/lib/reporting.py")
assert REPORT_SPEC and REPORT_SPEC.loader
reporting=importlib.util.module_from_spec(REPORT_SPEC);sys.modules[REPORT_SPEC.name]=reporting;REPORT_SPEC.loader.exec_module(reporting)
class QualificationError(AssertionError):pass
def require(value,message):
 if not value:raise QualificationError(message)
def load(path):
 try:return json.loads(path.read_text())
 except (OSError,json.JSONDecodeError) as error:raise QualificationError(f"required evidence unavailable: {path.name}") from error
def qualify(report,canonical,reliability,mutations,policy,catalog,quarantine,history,attestations,artifact_bytes,mode="enforce"):
 require(mode in {"observe","enforce"},"qualification mode is invalid")
 require(policy.get("status")=="promoted" and policy.get("required_check")=="E2E Hermetic Required","promotion policy/check name drift")
 require(report.get("required") is True and report.get("success") is True,"hermetic execution did not pass as required")
 require(report.get("network_policy")=="loopback-only" and report.get("provider_mode")=="double","hermetic trust boundary drift")
 require(report.get("evidence",{}).get("sanitized") is True,"redaction evidence is absent")
 for name,ceiling in policy["budgets"].items():
  if name in {"p95_wall_seconds","artifact_bytes"}:continue
  require(report.get("resource_observed",{}).get(name,ceiling+1)<=ceiling,f"runner ceiling exceeded: {name}")
 require(artifact_bytes<=policy["budgets"]["artifact_bytes"],"sanitized artifact size budget exceeded")
 expected=report.get("expected_stages",[]);stages=report.get("stages",[]);require(expected and [item.get("name") for item in stages]==expected,"missing/unknown stage cannot pass")
 by_name={item["name"]:item for item in stages}
 for critical in ("catalog","mutation-sensitivity","real-composed-retrieval","teardown","isolation"):
  require(by_name.get(critical,{}).get("status")=="passed",f"critical stage failed or absent: {critical}")
 require(all(item.get("status")=="passed" for item in report.get("cleanup",{}).values()),"cleanup uncertainty cannot pass")
 require(canonical.get("summary",{}).get("status")=="passed" and all(item.get("cleanup",{}).get("success") for item in canonical.get("scenarios",[])),"canonical parity/reporting failed")
 require(mutations.get("summary",{}).get("status")=="passed" and mutations.get("policy",{}).get("fixed_repository_oracles") is True,"oracle sensitivity failed")
 require(len(mutations.get("scenarios",[]))>=policy["required_oracle_mutants"] and all(item.get("status")=="passed" for item in mutations["scenarios"]),"fixed oracle set survived or is incomplete")
 require(quarantine.get("quarantines")==[],"promotion requires zero quarantines")
 allowed={"behavioral_e2e","contract_only","out_of_scope"};classifications={item.get("classification") for item in catalog.get("operations",[])}
 require(classifications<=allowed and "behavioral_e2e" in classifications,"catalog has missing/unknown classifications")
 denominator=len(catalog["operations"]);behavioral=sum(item["classification"]=="behavioral_e2e" for item in catalog["operations"])
 percent=100*behavioral/denominator;require(percent>=policy["coverage"]["minimum_behavioral_percent"],"catalog coverage threshold failed")
 lifecycles={item.get("lifecycle") for item in catalog.get("scenarios",[])};require(set(policy["coverage"]["critical_lifecycles"])<=lifecycles,"critical lifecycle coverage is incomplete")
 segments=reliability.get("segments",[]);minimum=policy["observation_window"]["minimum_trusted_main_runs"]
 require(not reliability.get("escalations") and reliability.get("quarantined_scenarios")==0,"reliability escalation/quarantine blocks promotion")
 require(set(history)=={"schema","repository","workflow","trusted_ref","reports"} and history.get("schema")==1,"trusted workflow history envelope is malformed")
 require((history.get("repository"),history.get("workflow"),history.get("trusted_ref"))==("dinglebear-ai/axon","e2e-hermetic.yml","refs/heads/main"),"workflow history provenance is untrusted")
 require(set(attestations)=={"schema","source","repository","workflow","trusted_ref","runs"} and attestations.get("schema")==1 and attestations.get("source")=="github-actions-api","GitHub run attestations are malformed")
 require((attestations.get("repository"),attestations.get("workflow"),attestations.get("trusted_ref"))==("dinglebear-ai/axon","e2e-hermetic.yml","refs/heads/main"),"GitHub run attestation provenance is untrusted")
 attested={}
 for item in attestations.get("runs",[]):
  require(set(item)=={"run_id","run_attempt","head_sha","head_branch","event","conclusion","repository","workflow","trusted_ref","artifact_id","artifact_name","artifact_digest","artifact_expired"},"GitHub run attestation entry is malformed")
  key=(item.get("run_id"),item.get("run_attempt"));require(key not in attested,"duplicate GitHub run attestation")
  require((item.get("repository"),item.get("workflow"),item.get("trusted_ref"),item.get("head_branch"),item.get("conclusion"))==("dinglebear-ai/axon","e2e-hermetic.yml","refs/heads/main","main","success"),"GitHub run attestation is not a successful trusted-main run")
  require(item.get("event") in {"push","schedule","workflow_dispatch"},"untrusted GitHub event in workflow history")
  require(isinstance(item.get("artifact_id"),int) and item.get("artifact_name")==f"e2e-hermetic-{key[0]}-{key[1]}","GitHub evidence artifact identity is absent")
  digest=item.get("artifact_digest");require(isinstance(digest,str) and re.fullmatch(r"sha256:[0-9a-fA-F]{64}",digest) is not None,"GitHub evidence artifact digest is absent or malformed")
  require(item.get("artifact_expired") is False,"GitHub evidence artifact is expired or expiry is unknown")
  attested[key]=item
 runs=history.get("reports",[])
 if len(runs)<minimum and mode=="observe":
  return {"schema":1,"status":"observation_pending","required_check":policy["required_check"],"observed_trusted_main_runs":len(runs),"minimum_trusted_main_runs":minimum,"remaining_trusted_main_runs":minimum-len(runs),"enforced":False,"rollback":policy["rollback"]}
 require(len(runs)>=minimum,"forged run count cannot satisfy observation window")
 require(segments and all(item.get("runs",0)>=minimum and item.get("failures")==0 and not item.get("quarantined") for item in segments),"trusted reliability window is insufficient")
 durations=[];identities=set()
 for run in runs:
  try:reporting.validate_report(run)
  except reporting.ReportingError as error:raise QualificationError(f"trusted workflow history digest/provenance failed: {error}") from error
  require(run.get("summary",{}).get("status")=="passed","failed workflow history cannot promote")
  provenance=run.get("policy",{})
  require((provenance.get("workflow_repository"),provenance.get("workflow_file"),provenance.get("workflow_ref"))==("dinglebear-ai/axon","e2e-hermetic.yml","refs/heads/main"),"canonical workflow-run provenance is untrusted")
  run_id=provenance.get("workflow_run_id");attempt=provenance.get("workflow_run_attempt")
  require(isinstance(run_id,str) and run_id.isdigit() and isinstance(attempt,int) and attempt>=1,"canonical workflow-run identity is absent")
  identity=(run_id,attempt);require(identity not in identities,"duplicate canonical workflow run cannot satisfy observation window");identities.add(identity)
  attestation=attested.get(identity);require(attestation is not None,"canonical history run is absent from GitHub API attestations")
  require(attestation.get("head_sha")==run.get("tested_sha"),"canonical tested SHA disagrees with GitHub run attestation")
  durations.append(run["timing"]["total_ms"])
 ordered=sorted(durations);workflow_p95=ordered[min(len(ordered)-1,max(0,math.ceil(len(ordered)*.95)-1))]
 require(workflow_p95<=policy["budgets"]["p95_wall_seconds"]*1000,"trusted workflow p95 wall budget exceeded")
 require(all(item.get("runtime_ms",{}).get("p95",10**12)<=policy["budgets"]["p95_wall_seconds"]*1000 for item in segments),"per-scenario p95 budget exceeded")
 return {"schema":1,"status":"passed","required_check":policy["required_check"],"coverage":{"numerator":behavioral,"denominator":denominator,"percent":round(percent,2),"critical_lifecycles":sorted(lifecycles)},
         "budgets":policy["budgets"],"workflow_history_runs":len(runs),"workflow_p95_ms":workflow_p95,"reliability_segments":len(segments),"oracle_mutants":len(mutations["scenarios"]),"rollback":policy["rollback"]}
def seal(value):
 value=dict(value);value["decision_sha256"]=hashlib.sha256(json.dumps(value,sort_keys=True,separators=(",",":")).encode()).hexdigest();return value
def verify(value,allow_observation=False):
 digest=value.get("decision_sha256");unsigned={k:v for k,v in value.items() if k!="decision_sha256"}
 allowed={"passed","observation_pending"} if allow_observation else {"passed"}
 require(value.get("status") in allowed and value.get("required_check")=="E2E Hermetic Required","required decision did not pass")
 if value.get("status")=="observation_pending":require(value.get("enforced") is False and isinstance(value.get("remaining_trusted_main_runs"),int) and value["remaining_trusted_main_runs"]>0,"observation decision is malformed")
 require(digest==hashlib.sha256(json.dumps(unsigned,sort_keys=True,separators=(",",":")).encode()).hexdigest(),"required decision digest mismatch")
def main():
 parser=argparse.ArgumentParser();parser.add_argument("--mode",choices=("observe","enforce"),default="enforce");parser.add_argument("--report",type=Path);parser.add_argument("--canonical",type=Path);parser.add_argument("--reliability",type=Path);parser.add_argument("--mutations",type=Path);parser.add_argument("--history",type=Path);parser.add_argument("--attestations",type=Path);parser.add_argument("--out",type=Path);parser.add_argument("--verify-decision",type=Path);parser.add_argument("--allow-observation",action="store_true");args=parser.parse_args()
 if args.verify_decision:verify(load(args.verify_decision),args.allow_observation);return 0
 require(all((args.report,args.canonical,args.reliability,args.mutations,args.history,args.attestations,args.out)),"qualification inputs are required")
 value=seal(qualify(load(args.report),load(args.canonical),load(args.reliability),load(args.mutations),load(ROOT/"config/e2e/hermetic-required-policy.json"),load(ROOT/"tests/e2e/catalog/catalog.json"),load(ROOT/"config/e2e/quarantine.json"),load(args.history),load(args.attestations),args.report.stat().st_size+args.canonical.stat().st_size+args.reliability.stat().st_size+args.mutations.stat().st_size+args.history.stat().st_size+args.attestations.stat().st_size,args.mode))
 args.out.parent.mkdir(parents=True,exist_ok=True);args.out.write_text(json.dumps(value,indent=2,sort_keys=True)+"\n");print(json.dumps(value,sort_keys=True));return 0
if __name__=="__main__":
 try:raise SystemExit(main())
 except QualificationError as error:print(f"required gate qualification failed: {error}",file=sys.stderr);raise SystemExit(2)
