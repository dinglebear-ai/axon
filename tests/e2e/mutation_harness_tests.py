#!/usr/bin/env python3
import importlib.util,json,tempfile,unittest
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
def load(name,relative):
 path=ROOT/relative;spec=importlib.util.spec_from_file_location(name,path);module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module
harness=load("mutation_harness_tests_impl","scripts/e2e/lib/mutation_harness.py");runner=load("mutation_runner_tests_impl","scripts/e2e/run-mutations.py");reporting=load("mutation_reporting_tests_impl","scripts/e2e/lib/mutation-report.py")
class MutationHarnessTests(unittest.TestCase):
 def mutants(self):return harness.load_mutants(ROOT/"tests/e2e/mutations/mutants.json")
 def test_every_required_mutation_is_killed_by_named_oracle(self):
  results=runner.run(self.mutants(),workers=4,timeout=2);self.assertEqual(11,len(results));self.assertEqual({"killed"},{x["outcome"] for x in results});self.assertTrue(all(x["scenario"] and x["invariant"] and x["baseline_passed"] and x["restoration_passed"] for x in results))
 def test_ineffective_definition_survives_and_fails_report(self):
  item={**self.mutants()[0],"id":"ineffective","mutation":"ineffective"};result=runner.run([item],workers=1,timeout=2)[0];self.assertEqual("survived",result["outcome"])
  report=reporting.build([result],policy={},duration_ms=1,exceptions_path=ROOT/"tests/e2e/mutations/exceptions.json");self.assertEqual("failed",report["summary"]["status"]);self.assertEqual(["ineffective"],report["policy"]["unowned_survivors"])
 def test_worker_error_is_harness_failure_not_kill(self):
  item={**self.mutants()[0],"id":"broken","harness_behavior":"crash"};result=runner.run([item],workers=1,timeout=2)[0];self.assertEqual("harness_failure",result["outcome"])
  report=reporting.build([result],policy={},duration_ms=1,exceptions_path=ROOT/"tests/e2e/mutations/exceptions.json");self.assertEqual("failed",report["summary"]["status"])
 def test_timeout_is_harness_failure_not_kill(self):
  item={**self.mutants()[0],"id":"timeout","mutation":"ineffective","harness_behavior":"timeout","sleep_seconds":0.1};result=runner.run([item],workers=1,timeout=0.01)[0];self.assertEqual("harness_failure",result["outcome"])
 def test_parallel_report_has_no_scratch_or_private_runtime_data(self):
  report=reporting.build(runner.run(self.mutants(),workers=8,timeout=2),policy={"subset":"full"},duration_ms=3,exceptions_path=ROOT/"tests/e2e/mutations/exceptions.json");encoded=json.dumps(report);self.assertNotIn("/tmp/",encoded);self.assertNotIn("http://",encoded);self.assertNotIn("secret-canary",encoded)
 def test_report_round_trip(self):
  report=reporting.build(runner.run(self.mutants()[:7],workers=4,timeout=2),policy={"subset":"representative"},duration_ms=4,exceptions_path=ROOT/"tests/e2e/mutations/exceptions.json")
  with tempfile.TemporaryDirectory() as directory:
   path=Path(directory)/"report.json";reporting.write(report,path);saved=json.loads(path.read_text());reporting.validate(saved);self.assertEqual(100.0,saved["policy"]["mutation_score_percent"])
 def test_exception_requires_owner_reason_and_future_expiry(self):
  with tempfile.TemporaryDirectory() as directory:
   path=Path(directory)/"exceptions.json"
   for body in ({"exceptions":[{"mutant":"x","expires":"2099-01-01"}]},{"exceptions":[{"mutant":"x","owner":"team","reason":"tracked","expires":"2020-01-01"}]}):
    path.write_text(json.dumps(body))
    with self.assertRaises(reporting.MutationReportError):reporting.exceptions(path)
 def test_parallel_wall_timing_is_separate_from_summed_mutant_runtime(self):
  results=runner.run(self.mutants(),workers=4,timeout=2);report=reporting.build(results,policy={},duration_ms=17,exceptions_path=ROOT/"tests/e2e/mutations/exceptions.json")
  self.assertEqual(17,report["policy"]["parallel_wall_duration_ms"]);self.assertEqual(sum(x["runtime_ms"] for x in results),report["policy"]["summed_mutant_runtime_ms"]);self.assertEqual(report["timing"]["total_ms"],report["policy"]["summed_mutant_runtime_ms"])
 def test_failed_run_atomically_replaces_stale_passing_evidence(self):
  with tempfile.TemporaryDirectory() as directory:
   path=Path(directory)/"mutation-report.json";good,_=runner.run_and_report(self.mutants()[:1],workers=1,timeout=2,mode="representative",report_path=path);self.assertEqual("passed",good["summary"]["status"])
   ineffective={**self.mutants()[0],"id":"ineffective","mutation":"ineffective"};failed,_=runner.run_and_report([ineffective],workers=1,timeout=2,mode="full",report_path=path);saved=json.loads(path.read_text());self.assertEqual("failed",failed["summary"]["status"]);self.assertEqual("failed",saved["summary"]["status"]);self.assertEqual("full",saved["policy"]["subset"]);self.assertEqual("mutation.ineffective",saved["scenarios"][0]["scenario_id"])
if __name__=="__main__":unittest.main()
