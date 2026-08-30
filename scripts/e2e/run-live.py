#!/usr/bin/env python3
"""Lease-scoped live runner; gateways, never raw shared providers, own mutation."""
from __future__ import annotations
import argparse,datetime as dt,importlib.util,json,os,secrets,signal,subprocess,threading,time,urllib.error,urllib.parse,urllib.request
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
def module(name,path):
 spec=importlib.util.spec_from_file_location(name,path);value=importlib.util.module_from_spec(spec);__import__("sys").modules[name]=value;spec.loader.exec_module(value);return value
isolation=module("axon_live_isolation",ROOT/"scripts/e2e/lib/run-isolation.py");teardown=module("axon_live_teardown",ROOT/"scripts/e2e/lib/teardown.py")
def register_for_outer_cleanup(manifest):
 registry=os.environ.get("AXON_E2E_CLEANUP_REGISTRY")
 if not registry:return
 report=Path(manifest).parent/"outer-cleanup-registration.json"
 completed=subprocess.run([__import__("sys").executable,str(ROOT/"scripts/e2e/cleanup-owned-runs.py"),"--registry",registry,
                           "--register-manifest",str(manifest),"--report",str(report)],cwd=ROOT,
                          capture_output=True,text=True,timeout=15,check=False)
 if completed.returncode:raise RuntimeError("durable outer-cleanup manifest registration failed")
 try:value=json.loads(report.read_text())
 except (OSError,json.JSONDecodeError) as error:raise RuntimeError("outer-cleanup registration receipt is invalid") from error
 if value.get("success") is not True:raise RuntimeError("outer-cleanup registration was refused")
class CancellationShield:
 def __init__(self):self.cleanup=False;self.interrupted=False;self.previous={}
 def _handle(self,_signum,_frame):
  if self.cleanup or self.interrupted:return
  self.interrupted=True;raise InterruptedError("workflow cancellation")
 def install(self):
  for sig in (signal.SIGTERM,signal.SIGINT):self.previous[sig]=signal.signal(sig,self._handle)
 def begin_cleanup(self):self.cleanup=True
 def restore(self):
  for sig,handler in self.previous.items():signal.signal(sig,handler)
def call(url,token,method,payload):
 data=json.dumps(payload).encode();request=urllib.request.Request(url,data=data,method=method,headers={"Authorization":f"Bearer {token}","Content-Type":"application/json"})
 with urllib.request.urlopen(request,timeout=20) as response:return json.load(response)
def retire_manifest_authority(manifest,owned_root):
 directory=manifest.path.parent.resolve();expected=(owned_root/"manifests"/manifest.run_id).resolve()
 if directory!=expected:raise RuntimeError("refusing to retire manifest outside the owned authority")
 for name in ("resources.jsonl.provider-ledger","resources.jsonl","manifest.key","outer-cleanup-registration.json"):(directory/name).unlink(missing_ok=True)
 directory.rmdir()
def oracle(kind,stdout):
 if kind in {"exit-zero","plan-only"}:return True
 if kind=="nonempty":return bool(stdout.strip())
 try:value=json.loads(stdout)
 except (json.JSONDecodeError,UnicodeDecodeError):return False
 encoded=json.dumps(value).lower()
 if kind=="grounded-json":
  answer=value.get("answer") if isinstance(value,dict) else None;citations=value.get("citations") if isinstance(value,dict) else None
  return isinstance(answer,str) and bool(answer.strip()) and isinstance(citations,list) and bool(citations) and all(isinstance(item,dict) and (item.get("source_id") or item.get("url")) and item.get("quote") for item in citations)
 if kind=="artifact-json":return isinstance(value,dict) and any(key in value for key in ("artifact_id","relative_path","screenshot"))
 return False
class Heartbeats:
 def __init__(self,leases,namespace,run_id,attempt,interval):self.leases,self.namespace,self.run_id,self.attempt,self.interval=leases,namespace,run_id,attempt,interval;self.stop=threading.Event();self.error=None;self.thread=threading.Thread(target=self._run,daemon=True)
 def _beat(self):
  for item,lease in self.leases:
   beat=call(os.environ[item["url_env"]].rstrip("/")+f"/v1/e2e/leases/{lease['lease_id']}/heartbeat",os.environ[item["auth_env"]],"PATCH",{"namespace":self.namespace,"owner":"dinglebear-ai/axon","run_id":self.run_id,"run_attempt":self.attempt})
   required={"status","heartbeat_at","expires_at","namespace","owner","run_id","run_attempt"}
   if set(beat)!=required or (beat["status"],beat["namespace"],beat["owner"],beat["run_id"],beat["run_attempt"])!=("renewed",self.namespace,"dinglebear-ai/axon",self.run_id,self.attempt):raise RuntimeError("provider lease heartbeat ownership mismatch")
   if dt.datetime.fromisoformat(beat["expires_at"].replace("Z","+00:00"))<=dt.datetime.now(dt.timezone.utc):raise RuntimeError("provider lease heartbeat did not renew expiry")
 def _run(self):
  try:
   while not self.stop.is_set():self._beat();self.stop.wait(self.interval)
  except Exception as error:self.error=error;self.stop.set()
 def __enter__(self):self.thread.start();return self
 def __exit__(self,*_args):self.stop.set();self.thread.join(timeout=max(2,self.interval+1))
def run_owned(manifest,run_root,scenario,env,heartbeats):
 capture=run_root/f"scenario-{scenario['id']}";managed=isolation.spawn_owned_process(manifest,run_root,scenario["argv"],env=env,capture_prefix=capture)
 stdout_path=capture.with_suffix(".stdout");stderr_path=capture.with_suffix(".stderr");manifest.register("output",str(stdout_path));manifest.register("output",str(stderr_path))
 deadline=time.monotonic()+scenario["timeout"]
 while managed.process.poll() is None:
  if heartbeats.error is not None:
   terminate_owned(managed.process);raise RuntimeError("provider heartbeat circuit breaker opened")
  if time.monotonic()>=deadline:
   terminate_owned(managed.process);raise subprocess.TimeoutExpired(scenario["argv"],scenario["timeout"])
  time.sleep(.1)
 return type("OwnedResult",(),{"returncode":managed.process.returncode,"stdout":stdout_path.read_bytes(),"stderr":stderr_path.read_bytes()})()
def terminate_owned(process,grace=5):
 if process.poll() is not None:return
 try:
  if os.name=="nt":process.terminate()
  else:os.killpg(process.pid,signal.SIGTERM)
 except ProcessLookupError:return
 try:process.wait(timeout=grace);return
 except subprocess.TimeoutExpired:pass
 try:
  if os.name=="nt":process.kill()
  else:os.killpg(process.pid,signal.SIGKILL)
 except ProcessLookupError:pass
 process.wait(timeout=grace)
def main():
 p=argparse.ArgumentParser();p.add_argument("--preflight",type=Path,required=True);p.add_argument("--report",type=Path,required=True);p.add_argument("--scenario-plan",type=Path,default=ROOT/"tests/e2e/live/scenarios.json");a=p.parse_args()
 config=json.loads((ROOT/"config/e2e/live-services.json").read_text());preflight=json.loads(a.preflight.read_text())
 run_id=os.environ.get("GITHUB_RUN_ID","");attempt=os.environ.get("GITHUB_RUN_ATTEMPT","");sha=os.environ.get("GITHUB_SHA","")
 if not run_id.isdigit() or not attempt.isdigit() or len(sha)!=40:raise SystemExit("trusted GitHub run identity is required")
 if preflight.get("status")!="passed":
  a.report.parent.mkdir(parents=True,exist_ok=True);a.report.write_text(json.dumps({"schema":1,"tested_sha":sha,"run_id":run_id,"run_attempt":attempt,"namespace":None,"duration_ms":0,"classification":preflight.get("classification","provider"),"success":False,"scenarios":[],"cleanup":[],"preflight":preflight,"sanitized":True},indent=2,sort_keys=True)+"\n");return 2
 owned_root=Path(os.environ.get("AXON_E2E_OWNED_ROOT",ROOT/"target/e2e")).resolve();owned_root.mkdir(parents=True,exist_ok=True)
 namespace=f"axon_e2e_{run_id}_{attempt}_{secrets.token_hex(8)}";leases=[];started=time.time();failure=None;failure_detail=None;outcomes=[]
 run_root=owned_root/"runs"/namespace;data_dir=run_root/"data";data_dir.mkdir(parents=True,mode=0o700)
 manifest=isolation.Manifest.create(owned_root/"manifests",namespace,data_dir);register_for_outer_cleanup(manifest.path);manifest.register("data_dir",str(data_dir));manifest.register("sqlite",str(data_dir/"jobs.db"))
 cancellation=CancellationShield();cancellation.install()
 try:
  for item in config["providers"]:
   url=os.environ[item["url_env"]].rstrip("/")+"/v1/e2e/leases";token=os.environ[item["auth_env"]]
   janitor=call(url+"/reap",token,"POST",{"owner":"dinglebear-ai/axon","expired_only":True,"residual_audit":True})
   if janitor!={"status":"passed","residuals":[]}:raise RuntimeError("provider stale-lease janitor failed")
   lease_id=f"{namespace}_{item['name']}_{secrets.token_hex(8)}";now=dt.datetime.now(dt.timezone.utc);expires_at=(now+dt.timedelta(seconds=config["lease_ttl_seconds"])).isoformat().replace("+00:00","Z");heartbeat_at=now.isoformat().replace("+00:00","Z")
   manifest.register("provider_reservation",f"{namespace}_{item['name']}",{"provider":item["name"],"lease_id":lease_id,"namespace":namespace,"owner":"axon-e2e","gateway_owner":"dinglebear-ai/axon","run_id":namespace,"github_run_id":run_id,"run_attempt":attempt,"attempt":1,"base_url_env":item["url_env"],"token_env":item["auth_env"],"heartbeat_unix_ms":int(now.timestamp()*1000),"expires_unix_ms":int((now+dt.timedelta(seconds=config["lease_ttl_seconds"])).timestamp()*1000),"ownership_generation":secrets.token_hex(32),"workflow":"e2e-live.yml"})
   header,resources=teardown.manifest_api.load(manifest.path);resource=next(value for value in resources if value.resource_type=="provider_reservation" and value.identity==f"{namespace}_{item['name']}");teardown.manifest_api.write_setup_intent(header,resource)
   lease=call(url,token,"POST",{"lease_id":lease_id,"namespace":namespace,"owner":"dinglebear-ai/axon","run_id":run_id,"run_attempt":attempt,"tested_sha":sha,"expires_at":expires_at,"heartbeat_at":heartbeat_at,"ttl_seconds":config["lease_ttl_seconds"],"heartbeat_seconds":config["heartbeat_seconds"],"max_concurrency":item["max_concurrency"],"qps":item["qps"]})
   if set(lease)!={"lease_id","namespace","expires_at","provider","owner","run_id","run_attempt","heartbeat_at"} or (lease["namespace"],lease["provider"],lease["owner"],lease["run_id"],lease["run_attempt"])!=(namespace,item["name"],"dinglebear-ai/axon",run_id,attempt):raise RuntimeError("provider lease ownership contract mismatch")
   if (lease["lease_id"],lease["expires_at"],lease["heartbeat_at"])!=(lease_id,expires_at,heartbeat_at):raise RuntimeError("provider did not honor signed lease identity/times")
   if dt.datetime.fromisoformat(lease["expires_at"].replace("Z","+00:00"))<=dt.datetime.now(dt.timezone.utc):raise RuntimeError("provider lease is already expired")
   state={key:lease[key] for key in ("lease_id","namespace","provider","owner","run_id","run_attempt")};teardown.manifest_api.write_provider_ledger(header,resource,state)
   leases.append((item,lease))
  env=dict(os.environ);env.update(AXON_E2E_LIVE="1",AXON_E2E_NAMESPACE=namespace,AXON_E2E_TESTED_SHA=sha,AXON_DATA_DIR=str(data_dir),
   QDRANT_URL=os.environ["AXON_E2E_QDRANT_GATEWAY_URL"],QDRANT_API_KEY=os.environ["AXON_E2E_QDRANT_TOKEN"],TEI_URL=os.environ["AXON_E2E_TEI_GATEWAY_URL"],AXON_TEI_BEARER_TOKEN=os.environ["AXON_E2E_TEI_TOKEN"],AXON_CHROME_REMOTE_URL=os.environ["AXON_E2E_CHROME_GATEWAY_URL"],AXON_CHROME_BEARER_TOKEN=os.environ["AXON_E2E_CHROME_TOKEN"],
   AXON_LLM_BACKEND="openai-compat",AXON_OPENAI_BASE_URL=os.environ["AXON_E2E_LLM_GATEWAY_URL"]+"/v1",AXON_OPENAI_API_KEY=os.environ["AXON_E2E_LLM_TOKEN"])
  plan=json.loads(a.scenario_plan.read_text())
  checked=subprocess.run(["git","rev-parse","HEAD"],cwd=ROOT,capture_output=True,text=True,check=True).stdout.strip()
  if checked!=sha or any(Path(row["argv"][0]).resolve()!= (ROOT/"target/debug/axon").resolve() for row in plan["commands"]):raise RuntimeError("scenario binary is not the locally built tested commit")
  for key in ("AXON_SERVER_URL","AXON_REMOTE_URL","AXON_API_URL"):env.pop(key,None)
  with Heartbeats(leases,namespace,run_id,attempt,min(30,config["heartbeat_seconds"])) as heartbeats:
   for scenario in plan["commands"]:
    if failure is not None or heartbeats.error is not None:break
    result=run_owned(manifest,run_root,scenario,env,heartbeats)
    passed=result.returncode==0 and oracle(scenario["oracle"],result.stdout)
    outcomes.append({"id":scenario["id"],"oracle":scenario["oracle"],"passed":passed,"returncode":result.returncode})
    if not passed:failure="product"
   if heartbeats.error is not None:raise RuntimeError("provider heartbeat circuit breaker opened")
 except subprocess.TimeoutExpired as error:failure="timeout";failure_detail=type(error).__name__
 except InterruptedError as error:failure="cancellation";failure_detail=type(error).__name__
 except urllib.error.HTTPError as error:
  try:failure="auth" if error.code in (401,403) else "provider";failure_detail="provider-http-status"
  finally:error.close()
 except urllib.error.URLError as error:failure="network";failure_detail=type(error.reason).__name__
 except (RuntimeError,OSError,ValueError) as error:failure="provider";failure_detail=type(error).__name__
 finally:
  cancellation.begin_cleanup()
  provider_config=a.report.with_name("live-provider-adapters.json");provider_config.write_text(json.dumps({"providers":{"live-gateways":{"kind":"gateway-lease","resource_types":["provider_reservation"]}}}))
  header,_=teardown.manifest_api.load(manifest.path);adapters=teardown.provider_api.build(provider_config,header,teardown.manifest_api);receipt=teardown.Engine(manifest.path,adapters).run().json();provider_config.unlink(missing_ok=True)
  cleanup=[{"provider":"canonical-teardown","passed":receipt["success"] and not receipt["residual"] and not receipt["refused"]}]
  report={"schema":1,"tested_sha":sha,"run_id":run_id,"run_attempt":attempt,"namespace":namespace,"manifest_digest":header.digest,"duration_ms":int((time.time()-started)*1000),"classification":failure,"failure_detail":failure_detail,"success":failure is None and cleanup[0]["passed"],"scenarios":outcomes,"cleanup":cleanup,"teardown":receipt,"preflight":preflight,"sanitized":True}
  a.report.parent.mkdir(parents=True,exist_ok=True);a.report.write_text(json.dumps(report,indent=2,sort_keys=True)+"\n")
  if receipt["success"] and not receipt["residual"] and not receipt["refused"]:
   retire_manifest_authority(manifest,owned_root)
   for path in (run_root,owned_root/"runs",owned_root/"manifests"):
    try:path.rmdir()
    except OSError:pass
  cancellation.restore()
 return 0 if report["success"] else 2
if __name__=="__main__":raise SystemExit(main())
