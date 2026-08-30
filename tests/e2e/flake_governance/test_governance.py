from __future__ import annotations
import copy,datetime as dt,hashlib,importlib.util,json,subprocess,sys,tempfile,unittest
from unittest import mock
from pathlib import Path

ROOT=Path(__file__).resolve().parents[3]
spec=importlib.util.spec_from_file_location("governance",ROOT/"scripts/e2e/lib/flake-governance.py")
gov=importlib.util.module_from_spec(spec);spec.loader.exec_module(gov)

def scenario(identifier="source.page",tier="hermetic",status="passed",classification=None,attempts=None,capability="source"):
    attempts=attempts or [{"attempt":1,"status":status,"duration_ms":10,"classification":classification,"summary":None}]
    first=next((item for item in attempts if item["status"]!="passed"),None)
    return {"scenario_id":identifier,"tier":tier,"capability":capability,"surface":"cli","status":status,
            "attempts":attempts,"first_attempt_failure":first,"invariants":[],"evidence":[],"cleanup":{"success":True,"residuals":[]}}

def report(rows):
    total=sum(sum(a["duration_ms"] for a in r["attempts"]) for r in rows);failed=sum(r["status"]!="passed" for r in rows)
    value={"schema":1,"tested_sha":"a"*40,"provider_versions":{"qdrant":"1.18.2"},"policy":{},"scenarios":rows,
      "timing":{"total_ms":total,"scenario_count":len(rows)},"summary":{"passed":len(rows)-failed,"failed":failed,"status":"passed" if failed==0 else "failed"},
      "upload":{"status":"not_attempted","local_evidence_path":None}}
    value["report_sha256"]=hashlib.sha256(json.dumps(value,sort_keys=True,separators=(",",":")).encode()).hexdigest();return value

def catalog(capability="source",tags=None):return {"scenarios":[{"id":"source.page","capability":capability,"lifecycle":capability,"tags":tags or [],"semantic_oracles":[]}]}

def quarantine(**overrides):
    value={"scenario_id":"source.page","owner":"team-axon","rationale":"Intermittent owned provider outage","issue":"https://github.com/dinglebear-ai/axon/issues/123","tier":"live","environment":"homelab","created_on":"2026-08-01","expires_on":"2026-09-15","restoration_criteria":"Twenty consecutive live passes"}
    value.update(overrides);return {"schema":1,"quarantines":[value]}

class GovernanceTests(unittest.TestCase):
    today=dt.date(2026,8,30)
    def test_repository_starts_with_zero_quarantines(self):
        self.assertEqual([],json.loads((ROOT/"config/e2e/quarantine.json").read_text())["quarantines"])
    def test_schema_requires_owned_linked_bounded_restoration_record(self):
        for field in gov.REQUIRED:
            item=quarantine();del item["quarantines"][0][field]
            with self.assertRaises(gov.GovernanceError):gov.validate_quarantines(item,catalog(),today=self.today)
    def test_expired_unknown_duplicate_and_protected_fail_closed(self):
        with self.assertRaisesRegex(gov.GovernanceError,"expired"):gov.validate_quarantines(quarantine(expires_on="2026-08-29"),catalog(),today=self.today)
        with self.assertRaisesRegex(gov.GovernanceError,"unknown"):gov.validate_quarantines(quarantine(),{"scenarios":[]},today=self.today)
        with self.assertRaisesRegex(gov.GovernanceError,"protected"):gov.validate_quarantines(quarantine(),catalog(capability="security"),today=self.today)
        for protected in ("cleanup","trust-boundary","secret-redaction","auth"):
            with self.assertRaisesRegex(gov.GovernanceError,"protected"):gov.validate_quarantines(quarantine(),catalog(capability=protected),today=self.today)
    def test_quarantined_scenario_must_run_and_is_not_healthy_coverage(self):
        with self.assertRaisesRegex(gov.GovernanceError,"did not execute"):gov.govern(report([]),catalog(),quarantine(),environment="homelab",today=self.today)
        result=gov.govern(report([scenario(tier="live")]),catalog(),quarantine(),environment="homelab",today=self.today)
        self.assertEqual(1,result["quarantined_scenarios"]);self.assertEqual(0,result["healthy_scenarios"])
        self.assertEqual({"observed":1,"denominator":0,"healthy":0,"percent":0,"quarantined_excluded_from_denominator":True},result["coverage"])
    def test_missing_first_attempt_and_invisible_or_hermetic_retry_fail(self):
        row=scenario(status="failed",classification="provider");row["first_attempt_failure"]=None
        with self.assertRaisesRegex(gov.GovernanceError,"first-attempt"):gov.validate_attempts(report([row]))
        attempts=[{"attempt":1,"status":"failed","duration_ms":1,"classification":"provider","summary":"outage","namespace":"axon_e2e_first_attempt_1"},{"attempt":2,"status":"passed","duration_ms":1,"classification":None,"summary":None,"namespace":"axon_e2e_second_attempt_2","serialized":True,"backoff_ms":50,"teardown_verified":True}]
        with self.assertRaisesRegex(gov.GovernanceError,"hermetic"):gov.validate_attempts(report([scenario(attempts=attempts)]))
    def test_governance_rejects_digest_tamper_missing_attempt_one_and_reordering(self):
        valid=report([scenario()]);valid["scenarios"][0]["attempts"][0]["attempt"]=2
        with self.assertRaisesRegex(gov.GovernanceError,"canonical report invalid"):gov.govern(valid,catalog(),{"schema":1,"quarantines":[]},environment="ci",today=self.today)
        tampered=report([scenario()]);tampered["scenarios"][0]["status"]="failed"
        with self.assertRaisesRegex(gov.GovernanceError,"canonical report invalid"):gov.govern(tampered,catalog(),{"schema":1,"quarantines":[]},environment="ci",today=self.today)
    def test_live_retry_requires_declared_safe_serialized_namespace_and_budget(self):
        attempts=[{"attempt":1,"status":"failed","duration_ms":1,"classification":"provider","summary":"outage"},{"attempt":2,"status":"passed","duration_ms":1,"classification":None,"summary":None}]
        row=scenario(tier="live",attempts=attempts);policy=gov.retry_evidence(scenario_id="source.page",lifecycle="source",retry_class="provider_transient",tier="live",classification="provider",budget_remaining=1,teardown_verified=True,seed="run",suite_budget_declared=1)
        attempts[0]["namespace"]=policy["previous_attempt_namespace"];attempts[1].update(namespace=policy["attempt_namespace"],serialized=True,backoff_ms=policy["backoff_ms"],teardown_verified=True)
        governed=report([row]);governed["policy"]["suite_retry_budget"]=1
        row["invariants"]=[{"retry_policy":policy}];gov.validate_attempts(governed);self.assertEqual(0,policy["suite_budget_remaining"]);self.assertTrue(50<=policy["backoff_ms"]<=500);self.assertNotEqual(policy["previous_attempt_namespace"],policy["attempt_namespace"])
        self.assertIsNone(gov.retry_evidence(scenario_id="x",lifecycle="upload",retry_class="diagnostic",tier="live",classification="provider",budget_remaining=1,teardown_verified=False,seed="x"))
    def test_actual_live_retry_path_records_cross_checked_attempt_evidence(self):
        item=gov.reporting.Scenario("source.page","live","source","cli");calls=[]
        def invoke(namespace):calls.append(namespace);return (("failed","provider","outage") if len(calls)==1 else ("passed",None,None))
        with mock.patch.object(gov.time,"sleep") as sleep:
            policy=gov.run_live_diagnostic(scenario=item,lifecycle="source",retry_class="provider_transient",budget_remaining=1,seed="run",invoke=invoke,verify_teardown=lambda _ns:True)
        self.assertEqual([policy["previous_attempt_namespace"],policy["attempt_namespace"]],calls);sleep.assert_called_once_with(policy["backoff_ms"]/1000)
        item.cleanup={"success":True};canonical=gov.reporting.suite_report([item],tested_sha="a"*40,provider_versions={},policy={"suite_retry_budget":1})
        gov.govern(canonical,catalog(),{"schema":1,"quarantines":[]},environment="live",today=self.today)
    def test_quarantine_cannot_mask_product_or_cleanup_and_provider_cannot_mask_assertion(self):
        row=scenario(tier="live",status="failed",classification="product")
        with self.assertRaisesRegex(gov.GovernanceError,"cannot mask"):gov.govern(report([row]),catalog(),quarantine(),environment="homelab",today=self.today)
        row=scenario(status="failed",classification="provider");row["invariants"]=[{"id":"product.semantic","passed":False}]
        with self.assertRaisesRegex(gov.GovernanceError,"masked product"):gov.validate_attempts(report([row]))
        row=scenario(status="failed",classification="provider");row["invariants"]=[{"id":"retrieved_answer_contains_expected_fact","passed":False,"details":"missing grounded answer"}]
        with self.assertRaisesRegex(gov.GovernanceError,"masked product"):gov.validate_attempts(report([row]))
        row["invariants"]=[{"schema":1,"kind":"provider_health","provider":"tei","passed":False,"classification":"provider"}]
        gov.validate_attempts(report([row]))
        row["invariants"]=[{"schema":1,"kind":"provider_health","provider":"tei","passed":False,"classification":"provider","unexpected":True}]
        with self.assertRaisesRegex(gov.GovernanceError,"masked product"):gov.validate_attempts(report([row]))
    def test_quarantine_inspects_every_attempt_and_budget_cannot_be_forged(self):
        attempts=[{"attempt":1,"status":"failed","duration_ms":1,"classification":"provider","summary":"outage"},{"attempt":2,"status":"failed","duration_ms":1,"classification":"product","summary":"assertion"}]
        policy=gov.retry_evidence(scenario_id="source.page",lifecycle="source",retry_class="provider_transient",tier="live",classification="provider",budget_remaining=1,teardown_verified=True,seed="run",suite_budget_declared=1)
        attempts[0]["namespace"]=policy["previous_attempt_namespace"];attempts[1].update(namespace=policy["attempt_namespace"],serialized=True,backoff_ms=policy["backoff_ms"],teardown_verified=True)
        row=scenario(tier="live",status="failed",attempts=attempts);row["invariants"]=[{"retry_policy":policy}];masked=report([row]);masked["policy"]["suite_retry_budget"]=1
        masked["report_sha256"]=hashlib.sha256(json.dumps({k:v for k,v in masked.items() if k!="report_sha256"},sort_keys=True,separators=(",",":")).encode()).hexdigest()
        with self.assertRaisesRegex(gov.GovernanceError,"cannot mask"):gov.govern(masked,catalog(),quarantine(),environment="homelab",today=self.today)
        attempts[1].update(status="passed",classification=None,summary=None,namespace=policy["attempt_namespace"],serialized=True,backoff_ms=policy["backoff_ms"],teardown_verified=True);attempts[0]["namespace"]=policy["previous_attempt_namespace"]
        row=scenario(tier="live",attempts=attempts);row["invariants"]=[{"retry_policy":policy}];forged=report([row]);forged["policy"]["suite_retry_budget"]=1
        policy["suite_budget_remaining"]=1
        with self.assertRaisesRegex(gov.GovernanceError,"forged"):gov.validate_attempts(forged)
    def test_canonical_rejects_resealed_aggregate_and_status_forgery(self):
        for mutate in (lambda value:value["summary"].update(status="passed"),lambda value:value["timing"].update(total_ms=999),lambda value:value["scenarios"][0].update(status="passed")):
            value=report([scenario(status="failed",classification="product")]);mutate(value);value["report_sha256"]=hashlib.sha256(json.dumps({k:v for k,v in value.items() if k!="report_sha256"},sort_keys=True,separators=(",",":")).encode()).hexdigest()
            with self.assertRaises(gov.reporting.ReportingError):gov.reporting.validate_report(value)
    def test_queue_expiry_circuit_breaker_are_nonpass(self):
        row=scenario();row["attempts"][0]["summary"]="queue_expired"
        with self.assertRaisesRegex(gov.GovernanceError,"reported as pass"):gov.validate_attempts(report([row]))
    def test_rolling_segments_and_recurrent_failure_escalation(self):
        failures=[report([scenario(tier="live",status="failed",classification="provider")]) for _ in range(3)]
        passes=[report([scenario(tier="live")]) for _ in range(2)]
        result=gov.reliability([*failures,*passes],{},environment="homelab")
        self.assertEqual(.4,result["segments"][0]["pass_rate"]);self.assertEqual("tracked_defect_required",result["escalations"][0]["signal"]);self.assertFalse(result["escalations"][0]["tracked"])
        self.assertEqual({"qdrant":"1.18.2"},result["segments"][0]["provider_versions"])
        self.assertEqual({"observed":1,"denominator":1,"healthy":0,"percent":0.0,"quarantined_excluded_from_denominator":True},result["coverage"])
    def test_history_provenance_and_each_canonical_digest_fail_closed(self):
        valid=report([scenario()]);envelope=gov.history_envelope([valid],repository="dinglebear-ai/axon",workflow="e2e-platform-smoke.yml",trusted_ref="refs/heads/main")
        self.assertEqual([valid],gov.validate_history(envelope,repository="dinglebear-ai/axon",workflow="e2e-platform-smoke.yml",trusted_ref="refs/heads/main"))
        wrong=copy.deepcopy(envelope);wrong["trusted_ref"]="refs/pull/7/merge"
        with self.assertRaisesRegex(gov.GovernanceError,"untrusted"):gov.validate_history(wrong,repository="dinglebear-ai/axon",workflow="e2e-platform-smoke.yml",trusted_ref="refs/heads/main")
        tampered=copy.deepcopy(envelope);tampered["reports"][0]["tested_sha"]="b"*40
        with self.assertRaisesRegex(gov.GovernanceError,"invalid canonical"):gov.validate_history(tampered,repository="dinglebear-ai/axon",workflow="e2e-platform-smoke.yml",trusted_ref="refs/heads/main")
    def test_workflows_restore_trusted_history_and_invoke_governance(self):
        for name in ("e2e-hermetic.yml","e2e-platform-smoke.yml"):
            text=(ROOT/".github/workflows"/name).read_text()
            self.assertIn("restore-reliability-history.py",text);self.assertIn("flake-governance.py",text)
            self.assertIn("refs/heads/main",text);self.assertNotIn("actions/cache",text)
            self.assertIn("if: always()",text)
        restore=(ROOT/"scripts/e2e/restore-reliability-history.py").read_text();self.assertIn('"completed"',restore)
        self.assertIn('"databaseId,attempt,conclusion,headBranch,headSha,event"',restore);self.assertIn("/artifacts",restore)
    def test_documented_restoration_requires_evidence_and_removal(self):
        text=(ROOT/"config/e2e/README.md").read_text()
        self.assertIn("remove the entry",text);self.assertIn("new evidence",text);self.assertIn("suite-wide budget",text)
    def test_cli_validates_zero_quarantine_and_writes_reliability(self):
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory);rp=path/"report.json";out=path/"reliability.json";rp.write_text(json.dumps(report([scenario()])))
            result=subprocess.run([sys.executable,str(ROOT/"scripts/e2e/flake-governance.py"),"--report",str(rp),"--environment","ci-linux","--reliability-out",str(out)],cwd=ROOT,capture_output=True,text=True)
            self.assertEqual(0,result.returncode,result.stderr);self.assertTrue(out.is_file())

if __name__=="__main__":unittest.main()
