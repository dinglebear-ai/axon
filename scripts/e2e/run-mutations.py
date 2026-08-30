#!/usr/bin/env python3
"""Run the bounded fixed Axon E2E oracle sensitivity program."""
import argparse,concurrent.futures,importlib.util,json,tempfile,time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2];REGISTRY=ROOT/"tests/e2e/mutations/mutants.json"
def load(name,path):
 spec=importlib.util.spec_from_file_location(name,path);module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module
harness=load("axon_e2e_mutation_harness",ROOT/"scripts/e2e/lib/mutation_harness.py")
reporting=load("axon_e2e_mutation_report",ROOT/"scripts/e2e/lib/mutation-report.py")
def execute(item,shard):
 started=time.monotonic()
 baseline_passed=False;restoration_passed=False
 # Every mutant gets a separate scratch ownership boundary. Only an in-memory
 # copy of the test fixture changes; no Axon binary, database, provider, or
 # retained evidence is exposed to mutation controls.
 with tempfile.TemporaryDirectory(prefix=f"axon-mutant-{shard}-") as scratch:
  (Path(scratch)/"ownership.json").write_text(json.dumps({"mutant":item["id"],"shard":shard}))
  original=harness.fixture(item);before=json.dumps(original,sort_keys=True)
  try:
   if item.get("harness_behavior")=="timeout":time.sleep(float(item.get("sleep_seconds",1)))
   if item.get("harness_behavior")=="crash":raise RuntimeError("controlled worker crash")
   # Crediting a kill requires the real oracle to accept the repository fixture
   # immediately before and after the isolated mutant.
   harness.oracle(item["oracle"],original);baseline_passed=True
   candidate=harness.mutate(item["mutation"],original)
   try:harness.oracle(item["oracle"],candidate);outcome="survived"
   except Exception:outcome="killed"
   restored=harness.fixture(item);harness.oracle(item["oracle"],restored);restoration_passed=True
   if before!=json.dumps(restored,sort_keys=True):outcome="harness_failure"
  except Exception:outcome="harness_failure"
 return {"mutant":item["id"],"codepath":item["codepath"],"scenario":item["scenario"],"invariant":item["invariant"],"outcome":outcome,"runtime_ms":int((time.monotonic()-started)*1000),"shard":shard,"baseline_passed":baseline_passed,"restoration_passed":restoration_passed}
def run(selected,*,workers,timeout):
 results=[]
 with concurrent.futures.ThreadPoolExecutor(max_workers=workers) as pool:
  futures=[(pool.submit(execute,item,index),item,index) for index,item in enumerate(selected)]
  for future,item,shard in futures:
   try:results.append(future.result(timeout=timeout))
   except (concurrent.futures.TimeoutError,BaseException):results.append({"mutant":item["id"],"codepath":item["codepath"],"scenario":item["scenario"],"invariant":item["invariant"],"outcome":"harness_failure","runtime_ms":int(timeout*1000),"shard":shard,"baseline_passed":False,"restoration_passed":False})
 return results
def self_check():
 baseline=harness.load_mutants(REGISTRY)[0];ineffective={**baseline,"id":"ineffective","mutation":"ineffective"}
 broken={**baseline,"id":"worker-crash","harness_behavior":"crash"}
 outcomes=[run([item],workers=1,timeout=1)[0]["outcome"] for item in (ineffective,broken)]
 if outcomes != ["survived","harness_failure"]:raise RuntimeError("mutation harness self-check failed")
def run_and_report(selected,*,workers,timeout,mode,report_path):
 started=time.monotonic();results=run(selected,workers=workers,timeout=timeout);wall_ms=int((time.monotonic()-started)*1000)
 report=reporting.build(results,policy={"subset":mode,"fixed_repository_oracles":True,"production_controls":False,"workers":workers},duration_ms=wall_ms,exceptions_path=ROOT/"tests/e2e/mutations/exceptions.json")
 reporting.write(report,report_path)
 return report,wall_ms
def main():
 parser=argparse.ArgumentParser();parser.add_argument("--report",type=Path,default=ROOT/"target/e2e/mutation-report.json");parser.add_argument("--subset",choices=("representative","full"),default="representative");parser.add_argument("--workers",type=int,default=4);parser.add_argument("--timeout",type=float,default=10);args=parser.parse_args()
 self_check();mutants=harness.load_mutants(REGISTRY);harness.validate_registry(mutants)
 representative={"mcp_initial_progress_suppressed","vector_publication_skipped","job_transition_invalid","transport_envelope_wrong","citations_missing","canary_evidence_leak","teardown_disabled","provider_failure_swallowed"}
 selected=[item for item in mutants if item["id"] in representative] if args.subset=="representative" else mutants;workers=max(1,min(args.workers,8))
 report,duration=run_and_report(selected,workers=workers,timeout=args.timeout,mode=args.subset,report_path=args.report)
 print(json.dumps({"report":str(args.report),"score_percent":report["policy"]["mutation_score_percent"],"mutants":len(selected),"duration_ms":duration},sort_keys=True));return 0 if report["summary"]["status"]=="passed" else 1
if __name__=="__main__":raise SystemExit(main())
