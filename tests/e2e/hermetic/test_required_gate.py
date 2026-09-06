from __future__ import annotations
import copy,hashlib,importlib.util,json,re,sys,unittest
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
SPEC=importlib.util.spec_from_file_location("axon_required_gate_test",ROOT/"scripts/e2e/qualify-required-gate.py")
assert SPEC and SPEC.loader
gate=importlib.util.module_from_spec(SPEC);sys.modules[SPEC.name]=gate;SPEC.loader.exec_module(gate)
def fixtures():
 policy=json.loads((ROOT/"config/e2e/hermetic-required-policy.json").read_text());catalog=json.loads((ROOT/"tests/e2e/catalog/catalog.json").read_text())
 names=["catalog","mutation-sensitivity","real-composed-retrieval","teardown","isolation"]
 report={"required":True,"success":True,"network_policy":"loopback-only","provider_mode":"double","evidence":{"sanitized":True},"duration_ms":1000,
  "resource_observed":{"cpu_seconds":1,"memory_mib":1,"processes":1,"ports":1,"shards":1,"retries":0,"artifacts":1},
  "expected_stages":names,"stages":[{"name":name,"status":"passed"} for name in names],"cleanup":{"teardown":{"status":"passed"},"isolation":{"status":"passed"}}}
 scenario=gate.reporting.Scenario("source.inline.happy","hermetic","source","cli");scenario.attempt("passed",10);scenario.cleanup={"success":True}
 canonical=gate.reporting.suite_report([scenario],tested_sha="a"*40,provider_versions={"axon":"workspace"},policy={"workflow_repository":"dinglebear-ai/axon","workflow_file":"e2e-hermetic.yml","workflow_ref":"refs/heads/main","workflow_run_id":"100","workflow_run_attempt":1})
 reliability={"segments":[{"runs":5,"failures":0,"quarantined":False,"runtime_ms":{"p50":10,"p95":10}}],"escalations":[],"quarantined_scenarios":0}
 mutations={"summary":{"status":"passed"},"policy":{"fixed_repository_oracles":True},"scenarios":[{"status":"passed"} for _ in range(8)]}
 reports=[]
 for index in range(5):
  run=copy.deepcopy(canonical);run["policy"]["workflow_run_id"]=str(100+index);unsigned={k:v for k,v in run.items() if k!="report_sha256"};run["report_sha256"]=hashlib.sha256(json.dumps(unsigned,sort_keys=True,separators=(",",":")).encode()).hexdigest();reports.append(run)
 history={"schema":1,"repository":"dinglebear-ai/axon","workflow":"e2e-hermetic.yml","trusted_ref":"refs/heads/main","reports":reports}
 attestations={"schema":1,"source":"github-actions-api","repository":"dinglebear-ai/axon","workflow":"e2e-hermetic.yml","trusted_ref":"refs/heads/main","runs":[{"run_id":str(100+index),"run_attempt":1,"head_sha":"a"*40,"head_branch":"main","event":"push","conclusion":"success","repository":"dinglebear-ai/axon","workflow":"e2e-hermetic.yml","trusted_ref":"refs/heads/main","artifact_id":1000+index,"artifact_name":f"e2e-hermetic-{100+index}-1","artifact_digest":"sha256:"+"b"*64,"artifact_expired":False} for index in range(5)]}
 return report,canonical,reliability,mutations,policy,catalog,{"quarantines":[]},history,attestations
class RequiredGateTests(unittest.TestCase):
 def test_all_proven_inputs_promote_and_seal(self):
  decision=gate.seal(gate.qualify(*fixtures(),artifact_bytes=100));gate.verify(decision);self.assertEqual("E2E Hermetic Required",decision["required_check"])
 def test_observation_mode_bootstraps_without_weakening_enforcement(self):
  values=list(fixtures());values[7]["reports"]=values[7]["reports"][:2];values[8]["runs"]=values[8]["runs"][:2]
  decision=gate.seal(gate.qualify(*values,artifact_bytes=100,mode="observe"));self.assertEqual("observation_pending",decision["status"]);self.assertEqual(3,decision["remaining_trusted_main_runs"])
  gate.verify(decision,allow_observation=True)
  with self.assertRaises(gate.QualificationError):gate.verify(decision)
  with self.assertRaisesRegex(gate.QualificationError,"forged run count"):gate.qualify(*values,artifact_bytes=100,mode="enforce")
 def test_missing_unknown_or_failed_critical_stage_fails(self):
  for mutation in (lambda r:r["stages"].pop(),lambda r:r["stages"].append({"name":"unknown","status":"passed"}),lambda r:r["stages"][0].update(status="failed")):
   values=list(fixtures());mutation(values[0])
   with self.assertRaises(gate.QualificationError):gate.qualify(*values,artifact_bytes=100)
 def test_parity_cleanup_redaction_and_catalog_drift_fail(self):
  mutations=(lambda v:v[1]["summary"].update(status="failed"),lambda v:v[1]["scenarios"][0]["cleanup"].update(success=False),lambda v:v[0]["evidence"].update(sanitized=False),lambda v:v[5]["operations"][0].update(classification=None))
  for mutate in mutations:
   values=list(fixtures());mutate(values)
   with self.assertRaises(gate.QualificationError):gate.qualify(*values,artifact_bytes=100)
 def test_unknown_nonbehavioral_classification_fails_closed(self):
  values=list(fixtures());target=next(item for item in values[5]["operations"] if item["classification"]!="behavioral_e2e");target["classification"]="future_unknown"
  with self.assertRaisesRegex(gate.QualificationError,"classification"):gate.qualify(*values,artifact_bytes=100)
 def test_oracle_survival_expired_quarantine_and_reliability_fail(self):
  for mutate in (lambda v:v[3]["scenarios"].pop(),lambda v:v[6]["quarantines"].append({"expires_on":"2026-01-01"}),lambda v:v[2]["segments"][0].update(runs=4),lambda v:v[2]["segments"][0].update(failures=1)):
   values=list(fixtures());mutate(values)
   with self.assertRaises(gate.QualificationError):gate.qualify(*values,artifact_bytes=100)
 def test_wall_resource_and_artifact_budgets_fail_closed(self):
  for mutate,size in ((lambda v:v[0]["resource_observed"].update(memory_mib=999999),100),(lambda v:None,999999999)):
   values=list(fixtures());mutate(values)
   with self.assertRaises(gate.QualificationError):gate.qualify(*values,artifact_bytes=size)
 def test_workflow_shape_required_trigger_fork_safety_and_always_run_evidence(self):
  text=(ROOT/".github/workflows/e2e-hermetic.yml").read_text();self.assertIn("pull_request:",text);self.assertIn("branches: [main]",text)
  self.assertNotIn("pull_request_target",text);self.assertNotIn("secrets.",text);self.assertIn("permissions:\n  contents: read\n  actions: read",text)
  self.assertIn("name: E2E Hermetic Required",text);self.assertIn("cancel-in-progress: false",text);self.assertNotIn("cancel-in-progress: true",text);self.assertIn("if: always()",text)
  self.assertIn("persist-credentials: false",text);self.assertNotIn("actions/cache",text)
  for action in re.findall(r"uses:\s*([^\s]+)",text):self.assertRegex(action,r"^[^@]+@[0-9a-f]{40}$")
  self.assertIn("--mode observe",text);self.assertIn("--verify-decision target/e2e/required-gate-decision.json --allow-observation",text)
 def test_policy_bypass_and_rollback_are_narrow(self):
  policy=fixtures()[4];self.assertEqual(["repository-administrator"],policy["bypass"]["actors"]);self.assertTrue(policy["bypass"]["requires_incident"])
  self.assertEqual({"remove_required_context_only":True,"preserve_workflow":True},policy["rollback"])
 def test_forged_five_run_claim_with_one_canonical_duration_fails(self):
  values=list(fixtures());values[2]["segments"][0]["runs"]=5;values[7]["reports"]=values[7]["reports"][:1]
  with self.assertRaisesRegex(gate.QualificationError,"forged run count"):gate.qualify(*values,artifact_bytes=100)
 def test_duplicate_or_untrusted_workflow_run_provenance_fails(self):
  values=list(fixtures());values[7]["reports"][1]=copy.deepcopy(values[7]["reports"][0])
  with self.assertRaisesRegex(gate.QualificationError,"duplicate"):gate.qualify(*values,artifact_bytes=100)
  values=list(fixtures());values[7]["reports"][0]["policy"]["workflow_ref"]="refs/pull/7/merge";unsigned={k:v for k,v in values[7]["reports"][0].items() if k!="report_sha256"};values[7]["reports"][0]["report_sha256"]=hashlib.sha256(json.dumps(unsigned,sort_keys=True,separators=(",",":")).encode()).hexdigest()
  with self.assertRaisesRegex(gate.QualificationError,"provenance"):gate.qualify(*values,artifact_bytes=100)
 def test_nonexistent_run_wrong_attempt_and_sha_fail_api_binding(self):
  for mutate in (lambda v:v[7]["reports"][0]["policy"].update(workflow_run_id="999999999999"),lambda v:v[7]["reports"][0]["policy"].update(workflow_run_attempt=999),lambda v:v[8]["runs"][0].update(head_sha="b"*40)):
   values=list(fixtures());mutate(values);unsigned={k:v for k,v in values[7]["reports"][0].items() if k!="report_sha256"};values[7]["reports"][0]["report_sha256"]=hashlib.sha256(json.dumps(unsigned,sort_keys=True,separators=(",",":")).encode()).hexdigest()
   with self.assertRaises(gate.QualificationError):gate.qualify(*values,artifact_bytes=100)
 def test_expired_missing_or_malformed_artifact_digest_fails(self):
  for mutate in (lambda item:item.update(artifact_expired=True),lambda item:item.update(artifact_digest=None),lambda item:item.update(artifact_digest="sha256:short"),lambda item:item.update(artifact_digest="sha256:"+"z"*64)):
   values=list(fixtures());mutate(values[8]["runs"][0])
   with self.assertRaises(gate.QualificationError):gate.qualify(*values,artifact_bytes=100)
 def test_history_digest_and_computed_workflow_p95_fail(self):
  values=list(fixtures());values[7]["reports"][0]["timing"]["total_ms"]=999999999
  with self.assertRaisesRegex(gate.QualificationError,"digest"):gate.qualify(*values,artifact_bytes=100)
  values=list(fixtures())
  for run in values[7]["reports"]:
   run["scenarios"][0]["attempts"][0]["duration_ms"]=999999999
   run["timing"]["total_ms"]=999999999
   unsigned={k:v for k,v in run.items() if k!="report_sha256"};run["report_sha256"]=hashlib.sha256(json.dumps(unsigned,sort_keys=True,separators=(",",":")).encode()).hexdigest()
  with self.assertRaisesRegex(gate.QualificationError,"p95"):gate.qualify(*values,artifact_bytes=100)
if __name__=="__main__":unittest.main()
