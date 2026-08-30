from __future__ import annotations
import copy,importlib.util,json,os,subprocess,sys,tempfile,time,unittest
from unittest import mock
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
SPEC=importlib.util.spec_from_file_location("axon_e2e_performance_test",ROOT/"scripts/e2e/lib/performance-report.py")
assert SPEC and SPEC.loader
perf=importlib.util.module_from_spec(SPEC);sys.modules[SPEC.name]=perf;SPEC.loader.exec_module(perf)
MEASURE_SPEC=importlib.util.spec_from_file_location("axon_e2e_performance_measure_test",ROOT/"scripts/e2e/measure-real-performance.py")
assert MEASURE_SPEC and MEASURE_SPEC.loader
measure=importlib.util.module_from_spec(MEASURE_SPEC);sys.modules[MEASURE_SPEC.name]=measure;MEASURE_SPEC.loader.exec_module(measure)
def report():
 fingerprint={"machine":{"runner_class":"macbook-primary","os":"Darwin","arch":"arm64","cpu":"M5","memory_bytes":1,"power_mode":"ac","thermal_state":"nominal"},
  "provider":{"provider_versions":{"tei":"double"},"model_versions":{"embedding":"double"},"endpoint_class":"hermetic-loopback"},
  "scenario":{"corpus_version":"v1","corpus_digest":"a"*64,"config_digest":"b"*64,"workload_cardinality":1,"concurrency":1,"queue_depth":0}}
 metrics=[]
 for metric_id in perf.REQUIRED_METRICS:
  values=[10.0,11.0,12.0,13.0,14.0]
  metrics.append({"id":metric_id,"status":"measured","mode":"cold" if metric_id=="cold_start_ms" else "warm","unit":"ms",
   "attribution":"provider" if metric_id=="llm_ms" else "axon","samples":values,"warmup_discarded":1,"timeout_ms":1000,
   "summary":perf.summarize(values,1,1000),"provenance":{"source":"fixture"}})
 return {"schema":perf.SCHEMA,"tested_sha":"0"*40,"fingerprint":fingerprint,"fingerprint_sha256":perf.fingerprint_digest(fingerprint),
  "policy":{"exclusive_group":"e2e-performance","correctness_retries":0,"baseline_retention":"last-20","timeout_censoring":"record","minimum_promotion_samples":5},
  "contention":{"exclusive_acquired":True,"pressure_detected":False,"baseline_eligible":True},"metrics":metrics,"cleanup":{"success":True,"residual":[]},
  "redaction":{"scanned":True,"oracle":"observe.redaction"},"evidence":[]}
class PerformanceReportTests(unittest.TestCase):
 def test_stable_schema_cold_warm_samples_and_percentiles(self):
  value=report();perf.validate_report(value);self.assertEqual(13.6,value["metrics"][0]["summary"]["p90"])
 def test_every_locked_metric_or_reason_is_required(self):
  value=report();value["metrics"].pop()
  with self.assertRaisesRegex(perf.PerformanceError,"inventory"):perf.validate_report(value)
 def test_environment_model_and_corpus_mismatch_blocks_comparison(self):
  current,baseline=report(),report();current["fingerprint"]["provider"]["model_versions"]={"embedding":"changed"}
  current["fingerprint_sha256"]=perf.fingerprint_digest(current["fingerprint"])
  result=perf.compare(current,baseline,{"schema":"axon-e2e-performance-budgets/v1","mode":"report_only","promotion":{},"budgets":[]})
  self.assertEqual("incomparable",result["status"]);self.assertIn("provider.model_versions",result["mismatches"])
 def test_unpromoted_threshold_never_hard_gates(self):
  baseline,current=report(),report()
  for metric in current["metrics"]:metric["samples"]=[100.0]*5;metric["summary"]=perf.summarize(metric["samples"],1,1000)
  result=perf.compare(current,baseline,{"schema":"axon-e2e-performance-budgets/v1","mode":"report_only","promotion":{},"budgets":[]})
  self.assertEqual("reported",result["status"])
 def test_synthetic_slowdown_is_detected_only_after_valid_promotion(self):
  baseline,current=report(),report();target=next(item for item in current["metrics"] if item["id"]=="retrieval_ms")
  target["samples"]=[20.0]*5;target["summary"]=perf.summarize(target["samples"],1,1000)
  budgets={"schema":"axon-e2e-performance-budgets/v1","mode":"gating","promotion":{"minimum_baselines":10,"minimum_samples_per_mode":5,"maximum_cv":.15},
   "budgets":[{"metric":"retrieval_ms","state":"gate","owner_approval":"perf-owner","baseline_count":10,"sample_count":5,"baseline_cv":.1,"max_regression_ratio":.2}]}
  self.assertEqual("regressed",perf.compare(current,baseline,budgets)["status"])
 def test_provider_regression_is_not_classified_as_product(self):
  baseline,current=report(),report();target=next(item for item in current["metrics"] if item["id"]=="llm_ms")
  target["samples"]=[20.0]*5;target["summary"]=perf.summarize(target["samples"],1,1000)
  budgets={"schema":"axon-e2e-performance-budgets/v1","mode":"gating","promotion":{"minimum_baselines":10,"minimum_samples_per_mode":5,"maximum_cv":.15},
   "budgets":[{"metric":"llm_ms","state":"gate","owner_approval":"provider-owner","baseline_count":10,"sample_count":5,"baseline_cv":.1,"max_regression_ratio":.2}]}
  self.assertEqual("provider",perf.compare(current,baseline,budgets)["classification"])
 def test_cleanup_contention_and_no_hidden_retry_fail_closed(self):
  for mutation,phrase in ((lambda x:x["cleanup"].update(success=False),"cleanup"),(lambda x:x["contention"].update(pressure_detected=True),"contention"),(lambda x:x["policy"].update(correctness_retries=1),"retry")):
   value=report();mutation(value)
   with self.assertRaisesRegex(perf.PerformanceError,phrase):perf.validate_report(value)
 def test_contended_measurement_is_infrastructure_not_product(self):
  current,baseline=report(),report();current["contention"].update(pressure_detected=True,baseline_eligible=False)
  budgets={"schema":"axon-e2e-performance-budgets/v1","mode":"report_only","promotion":{},"budgets":[]}
  result=perf.compare(current,baseline,budgets);self.assertEqual("infrastructure",result["classification"]);self.assertEqual("incomparable",result["status"])
 def test_release_projection_is_bounded_and_consumable(self):
  value=report();projection=perf.release_projection(value);encoded=json.dumps(projection);self.assertEqual(value["tested_sha"],projection["tested_sha"])
  self.assertNotIn('"samples"',encoded);self.assertNotIn("canonical_execution",encoded)
 def test_timeout_is_a_bounded_censored_infrastructure_report(self):
  value=perf.censored_report("0"*40,1,[{"classification":perf.classify_censor("",timed_out=True),"reason":"scenario_timeout","timeout_ms":10}])
  self.assertEqual("censored",value["status"]);self.assertEqual("infrastructure",value["classification"]);self.assertEqual(0,value["correctness_retries"])
 def test_sample_timeout_kills_descendant_group_then_runs_canonical_teardown(self):
  if sys.platform=="win32":self.skipTest("POSIX process group contract")
  with tempfile.TemporaryDirectory() as directory:
   pid_file=Path(directory)/"descendant.pid";teardowns=[]
   script=("import pathlib,subprocess,sys,time; "
           "p=subprocess.Popen([sys.executable,'-c','import time; time.sleep(60)']); "
           "pathlib.Path(sys.argv[1]).write_text(str(p.pid)); time.sleep(60)")
   with self.assertRaises(subprocess.TimeoutExpired):
    measure.run_process([sys.executable,"-c",script,str(pid_file)],dict(os.environ),timeout=.2,
                        teardown=lambda:teardowns.append("canonical"))
   self.assertEqual(["canonical"],teardowns);pid=int(pid_file.read_text())
   for _ in range(40):
    try:os.kill(pid,0)
    except ProcessLookupError:break
    time.sleep(.025)
   else:self.fail("timed-out performance descendant survived process-group escalation")
 def test_timeout_teardown_uses_private_unsanitized_handle_and_removes_it(self):
  with tempfile.TemporaryDirectory() as directory:
   root=Path(directory);handle=root/"active-handle.json";marker=root/"called"
   command=[sys.executable,"-c",f"import json,pathlib; pathlib.Path({str(marker)!r}).write_text('yes'); print(json.dumps({{'success':True,'residual':[],'refused':[]}}))"]
   handle.write_text(json.dumps({"schema":1,"run_id":"axon_e2e_test","manifest":str(root/"resources.jsonl"),"command":command}))
   with mock.patch.dict(os.environ,{"AXON_E2E_PERFORMANCE_TEARDOWN_HANDLE":str(handle)}):
    measure.canonical_sample_teardown()
   self.assertEqual("yes",marker.read_text());self.assertFalse(handle.exists())
 def test_gate_requires_history_variance_and_owner(self):
  config={"schema":"axon-e2e-performance-budgets/v1","mode":"gating","promotion":{"minimum_baselines":10,"minimum_samples_per_mode":5,"maximum_cv":.15},
   "budgets":[{"metric":"retrieval_ms","state":"gate","baseline_count":1,"sample_count":1,"baseline_cv":.5,"max_regression_ratio":.2}]}
  with self.assertRaises(perf.PerformanceError):perf.validate_budgets(config)
 def test_promoted_gate_rejects_undersampled_actual_baseline_despite_declarative_count(self):
  baseline,current=report(),report();target=next(item for item in baseline["metrics"] if item["id"]=="retrieval_ms")
  target["samples"]=[10.0]*4;target["summary"]=perf.summarize(target["samples"],1,1000)
  baseline["contention"].update(pressure_detected=True,baseline_eligible=False);current["contention"].update(pressure_detected=True,baseline_eligible=False)
  budgets={"schema":"axon-e2e-performance-budgets/v1","mode":"gating","promotion":{"minimum_baselines":10,"minimum_samples_per_mode":5,"maximum_cv":.15},
   "budgets":[{"metric":"retrieval_ms","state":"gate","owner_approval":"perf-owner","baseline_count":999,"sample_count":999,"baseline_cv":.01,"max_regression_ratio":.2}]}
  with self.assertRaisesRegex(perf.PerformanceError,"baseline samples"):perf.compare(current,baseline,budgets)
 def test_configured_minimum_cannot_weaken_actual_baseline_floor(self):
  baseline,current=report(),report();target=next(item for item in baseline["metrics"] if item["id"]=="retrieval_ms")
  target["samples"]=[10.0]*4;target["summary"]=perf.summarize(target["samples"],1,1000);baseline["contention"]["baseline_eligible"]=False;baseline["contention"]["pressure_detected"]=True
  current["contention"]["baseline_eligible"]=False;current["contention"]["pressure_detected"]=True
  budgets={"schema":"axon-e2e-performance-budgets/v1","mode":"gating","promotion":{"minimum_baselines":10,"minimum_samples_per_mode":1,"maximum_cv":.15},
   "budgets":[{"metric":"retrieval_ms","state":"gate","owner_approval":"owner","baseline_count":999,"sample_count":999,"baseline_cv":.01,"max_regression_ratio":.2}]}
  with self.assertRaisesRegex(perf.PerformanceError,"baseline samples"):perf.compare(current,baseline,budgets)
 def test_configured_minimum_cannot_weaken_actual_candidate_floor(self):
  baseline,current=report(),report();target=next(item for item in current["metrics"] if item["id"]=="retrieval_ms")
  target["samples"]=[20.0]*4;target["summary"]=perf.summarize(target["samples"],1,1000);baseline["contention"]["baseline_eligible"]=False;baseline["contention"]["pressure_detected"]=True
  current["contention"]["baseline_eligible"]=False;current["contention"]["pressure_detected"]=True
  budgets={"schema":"axon-e2e-performance-budgets/v1","mode":"gating","promotion":{"minimum_baselines":10,"minimum_samples_per_mode":1,"maximum_cv":.15},
   "budgets":[{"metric":"retrieval_ms","state":"gate","owner_approval":"owner","baseline_count":999,"sample_count":999,"baseline_cv":.01,"max_regression_ratio":.2}]}
  with self.assertRaisesRegex(perf.PerformanceError,"candidate samples"):perf.compare(current,baseline,budgets)
if __name__=="__main__":unittest.main()
