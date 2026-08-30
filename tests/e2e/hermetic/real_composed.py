#!/usr/bin/env python3
"""Substantive retrieval through one allocation-bound launcher descriptor."""
from __future__ import annotations
import contextlib,hashlib,importlib.util,json,os,secrets,shutil,subprocess,sys,threading,time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
def load(name,path):
 spec=importlib.util.spec_from_file_location(name,path);module=importlib.util.module_from_spec(spec)
 assert spec and spec.loader;sys.modules[name]=module;spec.loader.exec_module(module);return module
execute=load("axon_e2e_real_composed_execute",ROOT/"tests/e2e/scenarios/retrieval/execute.py")
source=load("axon_e2e_real_composed_source",ROOT/"tests/e2e/scenarios/source/orchestrator.py")
observe=load("axon_e2e_real_composed_observe",ROOT/"scripts/e2e/lib/observability-assertions.py")
reporting=load("axon_e2e_real_composed_reporting",ROOT/"scripts/e2e/lib/reporting.py")
def retained_descriptor(descriptor):
 value=json.loads(json.dumps(descriptor));value["environment"]["AXON_HTTP_TOKEN"]="[REDACTED]"
 value["bindings"]["AXON_HTTP_TOKEN"]="[REDACTED]"
 private_roots=tuple(item for item in (descriptor.get("run_root"),str(Path(descriptor.get("descriptor_path","-")).parent)) if item)
 def scrub(item):
  if isinstance(item,dict):return {key:scrub(child) for key,child in item.items()}
  if isinstance(item,list):return [scrub(child) for child in item]
  if isinstance(item,str):
   for private_root in private_roots:item=item.replace(private_root,"[REDACTED_PATH]")
  return item
 return scrub(value)
def verify_observability(binary,mcporter,descriptor,env,run_id):
 started=time.monotonic_ns();http=source.HttpJobsClient(descriptor["http_base_url"],descriptor["environment"]["AXON_HTTP_TOKEN"],30)
 secret_canary=f"ObserveSecret-{secrets.token_hex(16)}-DO-NOT-LEAK"
 private_path_canary=f"/private/axon-e2e-observe/{secrets.token_hex(16)}"
 source_file=Path(env["AXON_DATA_DIR"])/"observe-canary-source.md"
 source_file.write_text(f"# Observable beacon\n\nProtected values must never enter telemetry: {secret_canary} {private_path_canary}\n")
 source_path=str(source_file.resolve())
 source_argv=[str(binary),"source",source_path,"--scope","file","--collection",run_id,"--wait","true","--json"]
 source_process=subprocess.run(source_argv,cwd=ROOT,env=env,capture_output=True,timeout=30,check=False)
 if source_process.returncode:raise RuntimeError(f"real source observability failed: {source_process.stderr[-500:]!r}")
 created=execute.parse_output(source_process.stdout);source_log=source_process.stderr.decode(errors="strict")
 job_id=created.get("job_id")
 if not isinstance(job_id,str) or created.get("status") not in {"completed","completed_degraded"}:raise RuntimeError("real source observability fixture did not complete")
 cli,_=execute.invoke(binary,["jobs","get",job_id,"--json"],env,30)
 cli_events,_=execute.invoke(binary,["jobs","events",job_id,"--after-sequence","0","--limit","200","--json"],env,30)
 http_value=http.request("GET",f"/v1/jobs/{job_id}");http_events=http.request("GET",f"/v1/jobs/{job_id}/events")
 old_config=os.environ.get("MCPORTER_CONFIG");os.environ["MCPORTER_CONFIG"]=env["MCPORTER_CONFIG"]
 try:
  def mcp_call(arguments):
   completed=subprocess.run([str(mcporter),*execute.mcp_adapter.mcporter_argv(descriptor["mcp_selector"],arguments)],cwd=ROOT,env=env,capture_output=True,text=True,timeout=30,check=False)
   if completed.returncode:raise RuntimeError(f"real MCP observability failed: {completed.stderr[-500:]}")
   return source.McpJobsClient.decode_content(json.loads(completed.stdout))
  mcp_value=mcp_call({"action":"jobs","subaction":"get","job_id":job_id})
  mcp_events=mcp_call({"action":"jobs","subaction":"events","job_id":job_id,"after_sequence":0,"limit":200})
 finally:
  if old_config is None:os.environ.pop("MCPORTER_CONFIG",None)
  else:os.environ["MCPORTER_CONFIG"]=old_config
 def values(value,key):
  found=[]
  if isinstance(value,dict):
   for name,item in value.items():
    if name==key:found.append(item)
    found.extend(values(item,key))
  elif isinstance(value,list):
   for item in value:found.extend(values(item,key))
  return found
 executions=[]
 for surface,detail,event_page in (("cli",cli,cli_events),("http",http_value,http_events),("mcp",mcp_value,mcp_events)):
  sequences=[item for item in values(event_page,"sequence") if isinstance(item,int)]
  if not sequences:
   last=[item for item in values(event_page,"last_sequence") if isinstance(item,int)]
   totals=[item for item in values(event_page,"total") if isinstance(item,int)]
   if last and totals and max(totals)>1:sequences=[1,max(last)]
  statuses=[item for item in values(detail,"status") if isinstance(item,str)]
  if not sequences or job_id not in values(detail,"job_id")+values(detail,"id"):raise RuntimeError(f"real {surface} lifecycle lost correlation/progress")
  terminal=next((item for item in statuses if item in {"completed","completed_degraded","failed","canceled"}),None)
  executions.append({"surface":surface,"job_id":job_id,"terminal_status":terminal,"failure_classification":None,
                     "progress_sequence":min(sequences),"terminal_sequence":max(sequences)})
 stats=http.request("GET","/v1/stats");source_total=values(stats,"sources")
 if not any(isinstance(item,int) and item>=1 for item in source_total):raise RuntimeError("real durable source metric is absent")
 log_root=Path(env["AXON_DATA_DIR"])/"logs";log_text=source_log
 if log_root.is_dir():
  for path in log_root.rglob("*"):
   if path.is_file() and path.suffix in {".log",".jsonl",".json"}:log_text+="\n"+path.read_text(errors="strict")
 if job_id not in log_text:raise RuntimeError("real structured CLI/file log lost source job correlation")
 duration=(time.monotonic_ns()-started)//1_000_000
 digest=hashlib.sha256(json.dumps(http_events,sort_keys=True).encode()).hexdigest()
 capture={"observation_mode":"multi_observer","owned_provider_ids":[],"executions":executions,
  "timing":{"started_monotonic_ms":0,"finished_monotonic_ms":duration,"reported_duration_ms":duration,"tolerance_ms":250},
  "logs":[{"job_id":job_id,"phase":"complete","message":"correlated structured server log observed"}],
  "metrics":[{"name":"stats.totals.sources","labels":{"phase":"complete","status":"completed"},"value":max(source_total)}],
  "evidence":[{"job_id":job_id,"path":"canonical-report-invariant","sha256":digest,"bytes":len(json.dumps(http_events).encode())}],
  "raw_channels":{"cli_source_stderr":source_log,"cli_job":cli,"cli_events":cli_events,
                  "http_job":http_value,"http_events":http_events,"http_stats":stats,
                  "mcp_job":mcp_value,"mcp_events":mcp_events,"server_logs":log_text},
  "protected_canaries":[secret_canary],"private_paths":[private_path_canary]}
 runtime=observe.load_runtime(Path(env["AXON_SQLITE_PATH"]),job_id)
 outcomes=observe.evaluate(capture,runtime)
 scenario=reporting.Scenario("source.observability",os.environ.get("AXON_E2E_TIER","hermetic"),"source","multi-observer")
 scenario.invariants.extend(outcomes);scenario.attempt("passed",duration);scenario.cleanup={"success":True,"residual":[],"refused":[]}
 report=reporting.suite_report([scenario],tested_sha="0"*40,provider_versions={"axon":"workspace"},policy={"stack":"allocation-owned"})
 reporting.validate_report(report)
 return {"job_id":job_id,"oracles":[item["id"] for item in outcomes],"report_status":report["summary"]["status"],
         "source_to_terminal_ms":duration}
def main():
 binary=(ROOT/"target/debug/axon").resolve();mcporter=Path(shutil.which("mcporter") or "")
 if not binary.is_file() or not mcporter.is_file():raise RuntimeError("built Axon and mcporter are mandatory")
 owned_root=ROOT/"target/e2e/owned-runs";owned_root.mkdir(parents=True,exist_ok=True)
 with contextlib.nullcontext(owned_root) as td:
  run_id=f"axon_e2e_{secrets.token_hex(12)}";run_root=Path(td)/run_id
  allocation={"run_id":run_id,"collection":run_id,"run_root":str(run_root),
              "ownership_generation":secrets.token_hex(32),"seed_retrieval":True,"seed_stateful":True}
  cold_started=time.monotonic_ns()
  launched=subprocess.run([sys.executable,str(ROOT/"scripts/e2e/launch-hermetic-stack.py")],input=json.dumps(allocation),
                           cwd=ROOT,text=True,capture_output=True,timeout=30,check=False)
  if launched.returncode:raise RuntimeError(f"allocation launcher failed: {launched.stderr[-2000:]}")
  cold_ms=(time.monotonic_ns()-cold_started)//1_000_000
  descriptor=json.loads(launched.stdout);descriptor_path=Path(descriptor["descriptor_path"])
  teardown_handle=Path(os.environ.get("AXON_E2E_PERFORMANCE_TEARDOWN_HANDLE",ROOT/"target/e2e/performance-teardown-handle.json"));teardown_handle.parent.mkdir(parents=True,exist_ok=True)
  teardown_handle.write_text(json.dumps({"schema":1,"run_id":run_id,"manifest":descriptor["ownership_manifest"],
                                         "command":descriptor["teardown_handle"]["command"]},sort_keys=True)+"\n")
  os.chmod(teardown_handle,0o600)
  owned_pids=descriptor.get("ownership",{}).get("process_ids",{}) or descriptor.get("process_ids",{})
  resource_peak={"rss_bytes":0,"process_count":0};resource_stop=threading.Event()
  def sample_resources():
   while not resource_stop.wait(.05):
    sampled=subprocess.run(["ps","-axo","pid=,ppid=,rss="],capture_output=True,text=True,check=False);rows=[]
    for line in sampled.stdout.splitlines():
     fields=line.split()
     if len(fields)==3 and all(item.isdigit() for item in fields):rows.append(tuple(map(int,fields)))
    roots={os.getpid(),*(int(pid) for pid in owned_pids.values())};selected=set(roots)
    changed=True
    while changed:
     before=len(selected);selected.update(pid for pid,ppid,_rss in rows if ppid in selected);changed=len(selected)>before
    rss=[rss_kib*1024 for pid,_ppid,rss_kib in rows if pid in selected]
    resource_peak["rss_bytes"]=max(resource_peak["rss_bytes"],sum(rss));resource_peak["process_count"]=max(resource_peak["process_count"],len(rss))
  resource_thread=threading.Thread(target=sample_resources,daemon=True);resource_thread.start()
  retained=ROOT/"target/e2e/launcher-descriptor.json";retained.parent.mkdir(parents=True,exist_ok=True)
  retained.write_text(json.dumps(retained_descriptor(descriptor),indent=2,sort_keys=True)+"\n");os.chmod(retained,0o600)
  env={**os.environ,**descriptor["environment"],"AXON_E2E_RUN_ID":run_id,"AXON_E2E_CORPUS_ROOT":str(execute.CORPUS)}
  result=None
  try:
   required_env={"AXON_COLLECTION","AXON_MEMORY_COLLECTION","AXON_LLM_BACKEND","AXON_SYNTHESIS_OPENAI_MODEL",
                 "AXON_OPENAI_BASE_URL","AXON_OPENAI_API_KEY","QDRANT_URL","TEI_URL","AXON_CHROME_REMOTE_URL"}
   if not required_env <= set(descriptor["environment"]):raise RuntimeError("launcher omitted effective direct-CLI settings")
   warm_started=time.monotonic_ns();doctor,_=execute.invoke(binary,["doctor","--json"],env,30);warm_ms=(time.monotonic_ns()-warm_started)//1_000_000
   if doctor.get("all_ok") is not True:raise RuntimeError("allocation-bound direct CLI doctor failed")
   graph_source,_=execute.invoke(binary,["graph","source",descriptor["fixture_source_id"],"--json"],env,30)
   if descriptor["fixture_source_id"] not in json.dumps(graph_source):raise RuntimeError("stateful source fixture was not persisted")
   def sqlite_footprint():
    database=Path(env["AXON_SQLITE_PATH"]);return sum(path.stat().st_size for path in (database,Path(str(database)+"-wal"),Path(str(database)+"-shm")) if path.exists())
   sqlite_before=sqlite_footprint()
   observability=verify_observability(binary,mcporter,descriptor,env,run_id)
   sqlite_growth=max(0,sqlite_footprint()-sqlite_before)
   representative_count=0;representative_ms=None
   item={"prompt":"What signal does the Atlas beacon emit?","max_results":1};provider=descriptor["environment"]["AXON_SEARXNG_URL"]
   before=execute.provider_stats(provider)
   query_started=time.monotonic_ns();cli,_=execute.invoke(binary,["query",item["prompt"],"--limit","1","--collection",run_id,"--json"],env,30);cli_ms=(time.monotonic_ns()-query_started)//1_000_000
   token=descriptor["environment"]["AXON_HTTP_TOKEN"]
   query_started=time.monotonic_ns();http,_=execute.invoke_http(descriptor["http_base_url"],token,"query",item,run_id,30);http_ms=(time.monotonic_ns()-query_started)//1_000_000
   query_started=time.monotonic_ns();mcp,_=execute.invoke_mcp(mcporter,descriptor["mcp_selector"],"query",item,run_id,env,30);mcp_ms=(time.monotonic_ns()-query_started)//1_000_000
   after=execute.provider_stats(provider);delta=execute.provider_delta(before,after);delta["retries"]=0
   for surface,value in (("cli",cli),("http",http),("mcp",mcp)):
    if "amber" not in json.dumps(value,ensure_ascii=False).casefold():raise RuntimeError(f"real {surface} retrieval omitted evidence")
   if delta["calls"]<3:raise RuntimeError(f"provider observation missed surface calls: {delta}")
   if os.environ.get("AXON_E2E_PERFORMANCE_ONLY") == "1":
    corpus_manifest=json.loads((ROOT/"tests/e2e/corpus/manifest.json").read_text());representative=Path(env["AXON_DATA_DIR"])/"representative-corpus";representative.mkdir()
    for document in corpus_manifest["documents"]:
     if document.get("tier")=="representative" and document.get("expected_parse") is None:
      shutil.copy2(ROOT/"tests/e2e/corpus"/document["path"],representative/Path(document["path"]).name);representative_count+=1
    source_started=time.monotonic_ns();representative_result,_=execute.invoke(binary,["source",str(representative),"--scope","directory","--collection",run_id,"--wait","true","--json"],env,120)
    representative_ms=(time.monotonic_ns()-source_started)//1_000_000
    if representative_result.get("status") not in {"completed","completed_degraded"}:raise RuntimeError("representative corpus source performance workload did not complete")
   descriptor["status"]="verified";descriptor["provider_observation"]=delta
   descriptor_path.write_text(json.dumps(descriptor,indent=2,sort_keys=True)+"\n");os.chmod(descriptor_path,0o600)
   retained.write_text(json.dumps(retained_descriptor(descriptor),indent=2,sort_keys=True)+"\n");os.chmod(retained,0o600)
   if os.environ.get("AXON_E2E_PERFORMANCE_ONLY") != "1":
    for entry in sorted((ROOT/"tests/e2e/scenarios").glob("*/hermetic_entry.py")):
     completed=subprocess.run([sys.executable,str(entry),"--launcher-descriptor",str(descriptor_path)],cwd=ROOT,env=env,
                              capture_output=True,text=True,timeout=180,check=False)
     if completed.returncode:raise RuntimeError(f"domain hermetic entry failed: {entry.parent.name}: {completed.stderr[-2000:]}")
   result={"result":"pass","surfaces":["cli","http","mcp"],"provider_observation":delta,"observability":observability,
           "performance":{"cold_start_ms":cold_ms,
                          "warm_start_ms":warm_ms,"source_to_terminal_ms":representative_ms or observability["source_to_terminal_ms"],
                          "workload_cardinality":representative_count or 1,
                          "retrieval_ms":[cli_ms,http_ms,mcp_ms],
                          "http_first_response_ms":http_ms,"mcp_first_response_ms":mcp_ms,
                          "sqlite_growth_bytes":sqlite_growth,"peak_rss_bytes":resource_peak["rss_bytes"],"peak_process_count":resource_peak["process_count"]}}
   manifest=json.loads((ROOT/"tests/e2e/corpus/manifest.json").read_text())
   result["performance"]["provenance"]={"corpus_version":manifest["corpus_version"],"corpus_digest":manifest["corpus_checksum"],
    "provider_versions":{"qdrant_double":hashlib.sha256((ROOT/"tests/e2e/fixtures/teardown/qdrant_contract.py").read_bytes()).hexdigest(),
                         "provider_double":hashlib.sha256((ROOT/"tests/e2e/scenarios/retrieval/provider_double.py").read_bytes()).hexdigest()},
    "model_versions":{"synthesis":descriptor["environment"].get("AXON_SYNTHESIS_OPENAI_MODEL",{"status":"unsupported"}),
                      "embedding":descriptor["environment"].get("AXON_EMBEDDING_MODEL",{"status":"unsupported","reason":"launcher does not export embedding model"})}}
  finally:
   resource_stop.set();resource_thread.join(timeout=1)
   command=json.loads(descriptor_path.read_text())["teardown_handle"]["command"]
   cleanup_started=time.monotonic_ns();stopped=subprocess.run(command,cwd=ROOT,capture_output=True,text=True,timeout=15,check=False);cleanup_ms=(time.monotonic_ns()-cleanup_started)//1_000_000
   if stopped.returncode:raise RuntimeError(f"launcher teardown failed: {stopped.stderr[:300]}")
   cleanup_audit=json.loads(stopped.stdout)
   if cleanup_audit.get("success") is not True or cleanup_audit.get("residual") or cleanup_audit.get("refused"):raise RuntimeError("authoritative launcher cleanup audit failed")
   teardown_handle.unlink(missing_ok=True)
  if result is not None:
   result["performance"]["cleanup_ms"]=cleanup_ms;result["performance"]["cleanup_audit"]=cleanup_audit;print(json.dumps(result,sort_keys=True))
 return 0
if __name__=="__main__":raise SystemExit(main())
