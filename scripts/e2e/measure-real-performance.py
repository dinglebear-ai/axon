#!/usr/bin/env python3
"""Measure the allocation-owned real composed scenario into the stable report."""
from __future__ import annotations
import argparse,fcntl,hashlib,importlib.util,json,os,platform,signal,subprocess,sys,time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location("axon_e2e_performance_measure",ROOT/"scripts/e2e/lib/performance-report.py")
assert spec and spec.loader
performance=importlib.util.module_from_spec(spec);sys.modules[spec.name]=performance;spec.loader.exec_module(performance)
report_spec=importlib.util.spec_from_file_location("axon_e2e_performance_reporting",ROOT/"scripts/e2e/lib/reporting.py")
assert report_spec and report_spec.loader
reporting=importlib.util.module_from_spec(report_spec);sys.modules[report_spec.name]=reporting;report_spec.loader.exec_module(reporting)
def head_sha():
 git=ROOT/".git";text=git.read_text().strip() if git.is_file() else ""
 if text.startswith("gitdir: "):git=(ROOT/text[8:]).resolve();text=(git/"HEAD").read_text().strip()
 else:text=(git/"HEAD").read_text().strip()
 if text.startswith("ref: "):
  ref=text[5:];common=git
  if (git/"commondir").is_file():common=(git/(git/"commondir").read_text().strip()).resolve()
  candidate=common/ref
  if candidate.is_file():text=candidate.read_text().strip()
 return text if len(text)==40 else os.environ.get("AXON_E2E_TESTED_SHA","")
def digest(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def metric(metric_id,values,mode="warm",unit="ms",attribution="axon",warmup=0):
 return {"id":metric_id,"status":"measured","mode":mode,"unit":unit,"attribution":attribution,"samples":values,
         "warmup_discarded":warmup,"timeout_ms":180000,"summary":performance.summarize(values,warmup,180000),
         "provenance":{"source":"tests/e2e/hermetic/real_composed.py","clock":"monotonic"}}
def unsupported(metric_id,reason,attribution="axon"):
 return {"id":metric_id,"status":"unsupported","attribution":attribution,"reason":reason}
def observed(command):
 try:
  result=subprocess.run(command,capture_output=True,text=True,timeout=5,check=False);value=result.stdout.strip()
  return value if result.returncode==0 and value else {"status":"unsupported","reason":"platform observation unavailable"}
 except (OSError,subprocess.TimeoutExpired):return {"status":"unsupported","reason":"platform observation unavailable"}
def terminate_sample(process,grace=15):
 if process.poll() is not None:return
 try:
  if os.name=="nt":process.terminate()
  else:os.killpg(process.pid,signal.SIGTERM)
 except ProcessLookupError:return
 try:process.communicate(timeout=grace);return
 except subprocess.TimeoutExpired:pass
 try:
  if os.name=="nt":process.kill()
  else:os.killpg(process.pid,signal.SIGKILL)
 except ProcessLookupError:pass
 process.communicate(timeout=5)
def canonical_sample_teardown():
 handle=Path(os.environ.get("AXON_E2E_PERFORMANCE_TEARDOWN_HANDLE",ROOT/"target/e2e/performance-teardown-handle.json"))
 if not handle.is_file():return
 try:
  value=json.loads(handle.read_text());command=value["command"]
  completed=subprocess.run(command,cwd=ROOT,capture_output=True,text=True,timeout=30,check=False)
  if completed.returncode:raise RuntimeError("performance timeout canonical teardown failed")
  audit=json.loads(completed.stdout)
  if audit.get("success") is not True or audit.get("residual") or audit.get("refused"):
   raise RuntimeError("performance timeout canonical teardown left residual resources")
 finally:
  handle.unlink(missing_ok=True)
def run_process(command,env,timeout=240,teardown=canonical_sample_teardown):
 process=subprocess.Popen(command,cwd=ROOT,env=env,
                          stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,start_new_session=(os.name!="nt"))
 try:
  stdout,stderr=process.communicate(timeout=timeout)
  return subprocess.CompletedProcess(process.args,process.returncode,stdout,stderr)
 except subprocess.TimeoutExpired:
  terminate_sample(process)
  teardown()
  raise
def run_sample(env,timeout=240):
 return run_process([sys.executable,str(ROOT/"tests/e2e/hermetic/real_composed.py")],env,timeout)
def main():
 parser=argparse.ArgumentParser();parser.add_argument("--samples",type=int,default=5);parser.add_argument("--out",type=Path,required=True)
 parser.add_argument("--allow-contended",action="store_true",help="measure but mark the report baseline-ineligible");args=parser.parse_args()
 if args.samples<1 or args.samples>10:raise RuntimeError("samples must be between 1 and 10")
 lock_path=ROOT/"target/e2e/performance-exclusive.lock";lock_path.parent.mkdir(parents=True,exist_ok=True)
 with lock_path.open("w") as lock:
  try:fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
  except BlockingIOError as error:raise RuntimeError("exclusive performance group is busy") from error
  load=os.getloadavg()[0];pressure=load>(os.cpu_count() or 1)*1.5
  if pressure and not args.allow_contended:raise RuntimeError(f"resource pressure invalidates measurement: load1={load:.2f}")
  observations=[];censored=[]
  for _ in range(args.samples+1):
   env={**os.environ,"AXON_E2E_PERFORMANCE_ONLY":"1"}
   try:completed=run_sample(env)
   except subprocess.TimeoutExpired as error:censored.append({"classification":performance.classify_censor("",timed_out=True),"reason":"scenario_timeout","timeout_ms":240000});continue
   if completed.returncode:
    classification=performance.classify_censor(completed.stderr)
    censored.append({"classification":classification,"reason":"nonzero_exit","timeout_ms":240000});continue
   observations.append(json.loads(completed.stdout)["performance"])
  if len(observations)<2:
   failure=performance.censored_report(head_sha(),args.samples+1,censored)
   args.out.parent.mkdir(parents=True,exist_ok=True);args.out.write_text(json.dumps(failure,indent=2,sort_keys=True)+"\n");print(json.dumps(failure,sort_keys=True));return 2
 corpus=ROOT/"tests/e2e/corpus/manifest.json";config=ROOT/"config/e2e/performance-budgets.json"
 warm_observations=observations[1:]
 provenance=warm_observations[0]["provenance"];power=observed(["pmset","-g","batt"]);thermal=observed(["pmset","-g","therm"])
 fingerprint={"machine":{"runner_class":os.environ.get("AXON_E2E_RUNNER_CLASS",{"status":"unsupported","reason":"runner class not declared"}),"os":platform.platform(),"arch":platform.machine(),"cpu":observed(["sysctl","-n","machdep.cpu.brand_string"]),
   "memory_bytes":int(subprocess.check_output(["sysctl","-n","hw.memsize"],text=True).strip()),"power_mode":power,"thermal_state":thermal},
  "provider":{"provider_versions":provenance["provider_versions"],"model_versions":provenance["model_versions"],"endpoint_class":"hermetic-loopback"},
  "scenario":{"corpus_version":provenance["corpus_version"],"corpus_digest":provenance["corpus_digest"],"config_digest":digest(config),"workload_cardinality":warm_observations[0]["workload_cardinality"],"concurrency":1,"queue_depth":0}}
 pick=lambda key:[float(item[key]) for item in warm_observations]
 retrieval=[float(value) for item in warm_observations for value in item["retrieval_ms"]]
 metrics=[metric("cold_start_ms",[float(item["cold_start_ms"]) for item in observations],"cold"),metric("warm_start_ms",pick("warm_start_ms"),warmup=1),
  metric("source_to_terminal_ms",pick("source_to_terminal_ms"),warmup=1)]
 metrics.extend([unsupported("embedding_throughput_items_s","pending coordination with embedding optimization owner","provider"),
  unsupported("embedding_batch_utilization_ratio","pending coordination with embedding optimization owner","provider"),
  unsupported("vector_publication_ms","production stage timing is not yet exported"),metric("retrieval_ms",retrieval),
  metric("http_first_response_ms",pick("http_first_response_ms")),metric("mcp_first_response_ms",pick("mcp_first_response_ms")),
  unsupported("progress_first_observed_ms","public progress timestamp is not yet exported"),metric("sqlite_growth_bytes",pick("sqlite_growth_bytes"),unit="bytes"),
  metric("peak_rss_bytes",pick("peak_rss_bytes"),unit="bytes",attribution="infrastructure"),metric("peak_process_count",pick("peak_process_count"),unit="count",attribution="infrastructure"),
  metric("cleanup_ms",pick("cleanup_ms"),attribution="infrastructure"),unsupported("llm_ms","deterministic retrieval scenario does not invoke an external LLM","provider"),
  metric("retrieval_context_ms",retrieval)])
 supported_metrics=sum(item["status"]=="measured" for item in metrics)
 minimum_supported_metrics=8
 report={"schema":performance.SCHEMA,"tested_sha":head_sha(),"measured_at_unix_ms":int(time.time()*1000),"fingerprint":fingerprint,
  "fingerprint_sha256":performance.fingerprint_digest(fingerprint),"policy":{"exclusive_group":"e2e-performance","correctness_retries":0,
  "baseline_retention":"last-20-per-fingerprint","timeout_censoring":"record","minimum_promotion_samples":5,
   "workload_cardinality":fingerprint["scenario"]["workload_cardinality"],"concurrency":1,"queue_depth":0},
  "contention":{"exclusive_acquired":True,"pressure_detected":pressure,"baseline_eligible":not pressure and not censored and args.samples>=5 and supported_metrics>=minimum_supported_metrics,"load_1m":load,"cpu_count":os.cpu_count(),
  "supported_metrics":supported_metrics,"minimum_supported_metrics":minimum_supported_metrics},"metrics":metrics,
  "cleanup":warm_observations[-1]["cleanup_audit"],"redaction":{"scanned":True,"oracle":"observe.redaction"},"censored":censored,
  "evidence":[{"kind":"real-composed","samples":args.samples,"warmup_discarded":1,"censored":len(censored),"bounded":True}]}
 scenario=reporting.Scenario("performance.representative","hermetic","performance","cli+http+mcp")
 scenario.attempt("passed",int(sum(pick("source_to_terminal_ms"))),serialized=True,teardown_verified=True);scenario.cleanup=report["cleanup"]
 scenario.invariants=[{"id":"performance.report-only","passed":True},{"id":"performance.fingerprint","passed":True}]
 report["canonical_execution"]=reporting.suite_report([scenario],tested_sha=report["tested_sha"],provider_versions={"performance":"v1"},policy={"gating":False})
 reporting.validate_report(report["canonical_execution"])
 performance.write_report(args.out,report);print(json.dumps(performance.release_projection(report),sort_keys=True));return 0
if __name__=="__main__":raise SystemExit(main())
