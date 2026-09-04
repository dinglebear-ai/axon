#!/usr/bin/env python3
"""Restore the newest available completed trusted-branch history artifact."""
from __future__ import annotations
import argparse,json,os,re,subprocess,tempfile
from pathlib import Path
def attest(payload,runs,repository,workflow,trusted_ref):
 branch=trusted_ref.removeprefix("refs/heads/");by_id={str(item["databaseId"]):item for item in runs};evidence=[]
 for report in payload.get("reports",[]):
  policy=report.get("policy",{});run_id=policy.get("workflow_run_id");run=by_id.get(run_id)
  if run is None:raise SystemExit("history report does not map to an actual GitHub run")
  expected=(policy.get("workflow_run_attempt"),report.get("tested_sha"),run.get("headBranch"),run.get("conclusion"))
  actual=(run.get("attempt"),run.get("headSha"),branch,"success")
  if expected!=actual:raise SystemExit("history report disagrees with GitHub run metadata")
  if (policy.get("workflow_repository"),policy.get("workflow_file"),policy.get("workflow_ref"))!=(repository,workflow,trusted_ref):raise SystemExit("history report workflow provenance mismatch")
  evidence.append({"run_id":run_id,"run_attempt":run["attempt"],"head_sha":run["headSha"],"head_branch":run["headBranch"],"event":run["event"],"conclusion":run["conclusion"],"repository":repository,"workflow":workflow,"trusted_ref":trusted_ref,
                   "artifact_id":run.get("artifact_id"),"artifact_name":run.get("artifact_name"),"artifact_digest":run.get("artifact_digest"),"artifact_expired":run.get("artifact_expired")})
 return {"schema":1,"source":"github-actions-api","repository":repository,"workflow":workflow,"trusted_ref":trusted_ref,"runs":evidence}
def main():
 p=argparse.ArgumentParser();p.add_argument("--repository",required=True);p.add_argument("--workflow",required=True);p.add_argument("--artifact",required=True);p.add_argument("--evidence-artifact-template",default="e2e-hermetic-{run_id}-{run_attempt}");p.add_argument("--trusted-ref",default="refs/heads/main");p.add_argument("--out",type=Path,required=True);p.add_argument("--attestations-out",type=Path,required=True);a=p.parse_args()
 branch=a.trusted_ref.removeprefix("refs/heads/");empty={"schema":1,"repository":a.repository,"workflow":a.workflow,"trusted_ref":a.trusted_ref,"reports":[]}
 query=subprocess.run(["gh","run","list","--repo",a.repository,"--workflow",a.workflow,"--branch",branch,"--status","completed","--limit","100","--json","databaseId,attempt,conclusion,headBranch,headSha,event"],capture_output=True,text=True)
 if query.returncode:raise SystemExit("trusted history lookup failed")
 try:runs=json.loads(query.stdout)
 except json.JSONDecodeError as error:raise SystemExit("trusted history lookup returned invalid JSON") from error
 if not isinstance(runs,list):raise SystemExit("trusted history lookup returned invalid provenance")
 with tempfile.TemporaryDirectory() as directory:
  payload=empty
  missing=("not found","no valid artifacts","artifact not found")
  for run in runs:
   if (not isinstance(run,dict) or set(run)!={"databaseId","attempt","conclusion","headBranch","headSha","event"} or not isinstance(run.get("databaseId"),int)
       or not isinstance(run.get("attempt"),int) or run["attempt"]<1 or run.get("headBranch")!=branch or not isinstance(run.get("headSha"),str) or len(run["headSha"])!=40 or not run.get("conclusion")):raise SystemExit("trusted history run provenance is malformed")
   # Failed/cancelled runs may still upload an `always()` history artifact. They
   # are observations, never trusted inputs, and must not shadow an older good
   # rolling history during recovery.
   if run["conclusion"]!="success":continue
   target=Path(directory)/str(run["databaseId"]);target.mkdir()
   result=subprocess.run(["gh","run","download",str(run["databaseId"]),"--repo",a.repository,"--name",a.artifact,"--dir",str(target)],capture_output=True,text=True)
   if result.returncode:
    if any(value in result.stderr.lower() for value in missing):continue
    raise SystemExit("trusted history download failed")
   candidates=list(target.rglob("history.json"))
   if len(candidates)!=1:continue
   try:candidate=json.loads(candidates[0].read_text())
   except json.JSONDecodeError:continue
   if (candidate.get("repository"),candidate.get("workflow"),candidate.get("trusted_ref"))!=(a.repository,a.workflow,a.trusted_ref):continue
   # A newly uploaded history may contain the just-failed observation. Skip it
   # instead of poisoning restoration of the previous verified window.
   run_index={(str(item["databaseId"]),item["attempt"]):item for item in runs if isinstance(item,dict)}
   identities=[(item.get("policy",{}).get("workflow_run_id"),item.get("policy",{}).get("workflow_run_attempt")) for item in candidate.get("reports",[])]
   if any(identity not in run_index or run_index[identity].get("conclusion")!="success" for identity in identities):continue
   enriched=[dict(item) for item in runs];enriched_index={(str(item["databaseId"]),item["attempt"]):item for item in enriched};suitable=True
   for identity in identities:
    trusted_run=enriched_index[identity]
    artifact_query=subprocess.run(["gh","api",f"repos/{a.repository}/actions/runs/{trusted_run['databaseId']}/artifacts"],capture_output=True,text=True)
    if artifact_query.returncode:raise SystemExit("trusted run artifact provenance lookup failed")
    try:artifacts=json.loads(artifact_query.stdout).get("artifacts",[])
    except (json.JSONDecodeError,AttributeError) as error:raise SystemExit("trusted run artifact provenance is invalid") from error
    expected_name=a.evidence_artifact_template.format(run_id=trusted_run["databaseId"],run_attempt=trusted_run["attempt"]);matches=[item for item in artifacts if item.get("name")==expected_name]
    if len(matches)!=1 or not isinstance(matches[0].get("id"),int):suitable=False;break
    artifact=matches[0];digest=artifact.get("digest")
    if artifact.get("expired") is not False or not isinstance(digest,str) or re.fullmatch(r"sha256:[0-9a-fA-F]{64}",digest) is None:suitable=False;break
    trusted_run.update(artifact_id=artifact["id"],artifact_name=expected_name,artifact_digest=digest,artifact_expired=False)
   if not suitable:continue
   payload=candidate;runs=enriched;break
  if (payload.get("repository"),payload.get("workflow"),payload.get("trusted_ref"))!=(a.repository,a.workflow,a.trusted_ref):raise SystemExit("downloaded history provenance mismatch")
  attestation=attest(payload,runs,a.repository,a.workflow,a.trusted_ref)
  a.out.parent.mkdir(parents=True,exist_ok=True);a.out.write_text(json.dumps(payload,indent=2,sort_keys=True)+"\n")
  a.attestations_out.parent.mkdir(parents=True,exist_ok=True);a.attestations_out.write_text(json.dumps(attestation,indent=2,sort_keys=True)+"\n")
 return 0
if __name__=="__main__":raise SystemExit(main())
