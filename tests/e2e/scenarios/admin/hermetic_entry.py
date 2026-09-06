#!/usr/bin/env python3
"""Real plan-first destructive scenarios against disposable owned state."""
from __future__ import annotations
import argparse,datetime,hashlib,importlib.util,json,os,subprocess,sys,time,urllib.error,urllib.request
from pathlib import Path
ROOT=Path(__file__).resolve().parents[4]
def load(name,path):
 spec=importlib.util.spec_from_file_location(name,path);module=importlib.util.module_from_spec(spec)
 assert spec and spec.loader;sys.modules[name]=module;spec.loader.exec_module(module);return module
isolation=load("admin_isolation",ROOT/"scripts/e2e/lib/run-isolation.py")
http=load("admin_http",ROOT/"scripts/e2e/adapters/http_adapter.py")
guard=load("admin_destructive_guard",Path(__file__).with_name("destructive_guard.py"))
manifest_api=load("admin_manifest",ROOT/"scripts/e2e/lib/resource-manifest.py")
providers=load("admin_providers",ROOT/"scripts/e2e/lib/provider-adapters.py")
def wait(url,process):
 for _ in range(200):
  if process.process.poll() is not None:raise RuntimeError("owned admin Axon exited")
  try:urllib.request.urlopen(url,timeout=.1);return
  except urllib.error.HTTPError as error:
   try:
    if error.code in {401,403}:return
   finally:error.close()
  except OSError:pass
  time.sleep(.025)
 raise RuntimeError("owned admin Axon did not become ready")
def call(base,token,path,body,expected):
 response=http.request(base,token,http.json_request("POST",path,body),10)
 try:value=json.loads(response.body)
 except json.JSONDecodeError:raise RuntimeError(f"{path} returned non-JSON")
 if response.status not in expected:raise RuntimeError(f"{path}: HTTP {response.status}: {value}")
 return response.status,value
def get(base,token,path,expected):
 response=http.request(base,token,http.HttpRequest("GET",path,None),10)
 try:value=json.loads(response.body)
 except json.JSONDecodeError:raise RuntimeError(f"{path} returned non-JSON")
 if response.status not in expected:raise RuntimeError(f"{path}: HTTP {response.status}: {value}")
 return response.status,value
def cli_json(binary,args,env,expected=(0,)):
 completed=subprocess.run([binary,*args],env=env,capture_output=True,text=True,timeout=15)
 if completed.returncode not in expected:raise RuntimeError(f"CLI {' '.join(args)} failed: {completed.stderr}")
 try:return completed.returncode,json.loads(completed.stdout)
 except json.JSONDecodeError:raise RuntimeError(f"CLI {' '.join(args)} returned non-JSON: {completed.stdout}")
def qdrant(base,method,path,body=None,expected=(200,)):
 raw=None if body is None else json.dumps(body).encode();request=urllib.request.Request(base+path,data=raw,method=method,
   headers={"content-type":"application/json"} if raw is not None else {})
 try:
  with urllib.request.urlopen(request,timeout=8) as response:value=json.loads(response.read())
 except urllib.error.HTTPError as error:
  try:
   if error.code not in expected:raise
   value=json.loads(error.read())
  finally:error.close()
 else:
  if response.status not in expected:raise RuntimeError(f"Qdrant {method} {path}: HTTP {response.status}")
 return value.get("result",value)
def main():
 parser=argparse.ArgumentParser();parser.add_argument("--launcher-descriptor",type=Path,required=True);args=parser.parse_args()
 descriptor=json.loads(args.launcher_descriptor.read_text());run_id=descriptor["run_id"];run_root=Path(descriptor["run_root"])
 admin_run_id=f"{run_id}_admin"
 owned=run_root/"admin";data=owned/"data";data.mkdir(parents=True,exist_ok=True)
 manifest=isolation.Manifest.create(owned/"manifests",admin_run_id,data);manifest.register("data_dir",str(data))
 evidence=owned/"evidence";evidence.mkdir();manifest.register("output",str(evidence))
 reservation=isolation.allocate_port(owned/"leases",admin_run_id,manifest);port=reservation.port;reservation.close()
 token="axon-e2e-admin-token";prune_collection=f"{admin_run_id}_prune";migrate_source=f"{admin_run_id}_migrate_source";migrate_destination=f"{admin_run_id}_migrate_destination"
 env={**os.environ,**descriptor["environment"],"AXON_DATA_DIR":str(data),"AXON_COLLECTION":prune_collection,
  "AXON_MEMORY_COLLECTION":prune_collection,
  "AXON_SQLITE_PATH":str(data/"jobs.db"),"AXON_HTTP_HOST":"127.0.0.1","AXON_HTTP_PORT":str(port),"AXON_HTTP_TOKEN":token,
  "AXON_E2E_RUN_ID":admin_run_id}
 base=f"http://127.0.0.1:{port}";qstate=run_root/"launcher/qdrant.json"
 qbase=descriptor["qdrant_url"];generation=descriptor["collection_marker"]["ownership_generation"]
 provider_config=owned/"provider-config.json";provider_config.write_text(json.dumps({"providers":{"qdrant":{"kind":"qdrant","base_url":qbase,
   "tenant_enforced":True,"owned_prefix":"axon_e2e_","timeout_seconds":5,
   "resource_types":["collection","qdrant_alias","qdrant_snapshot","point","payload_index"]}}})+"\n")
 manifest.register("temp_path",str(provider_config))
 header,_=manifest_api.load(manifest.path);adapter=providers.build(provider_config,header,manifest_api)["collection"]
 for collection,named in ((prune_collection,True),(migrate_source,False)):
  vectors={"dense":{"size":8,"distance":"Cosine"}} if named else {"size":8,"distance":"Cosine"}
  create_payload={"vectors":vectors}
  if named:create_payload["sparse_vectors"]={"bm42":{"modifier":"idf"}}
  manifest.register("collection",collection,{"ownership_generation":generation,"provider":"qdrant","create_payload":create_payload})
  header,resources=manifest_api.load(manifest.path);adapter=providers.build(provider_config,header,manifest_api)["collection"]
  resource=next(item for item in resources if item.resource_type=="collection" and item.identity==collection)
  adapter.create_and_provision(resource)
  if collection==migrate_source:
   # The source collection's durable teardown marker is also returned by a
   # real Qdrant scroll. Give that synthetic point valid migration text so the
   # production migrator can process the complete owned collection.
   marker_id=manifest_api.qdrant_ownership_point(header,resource)["id"]
   qdrant(qbase,"POST",f"/collections/{collection}/points/payload",{"points":[marker_id],
     "payload":{"chunk_text":"owned migration ownership marker"}})
  if named:
   indexes=json.loads(qstate.read_text())["collections"][run_id]["indexes"]
   for field,schema in indexes.items():qdrant(qbase,"PUT",f"/collections/{collection}/index/{field}",schema)
  manifest_api.write_provider_ledger(header,resource,adapter._provider_state(resource))
  qdrant(qbase,"PUT",f"/collections/{collection}/points",{"points":[{"id":"00000000-0000-4000-8000-000000000001",
    "vector":{"dense":[0.0]*8} if named else [0.0]*8,"payload":{"_axon_e2e_owner":admin_run_id,"run_id":admin_run_id,
    "ownership_generation":generation,"resource_type":"collection","payload_contract_version":"2026-07-01","chunk_text":"owned migration fixture"}}]})
 manifest.register("collection",migrate_destination,{"ownership_generation":generation,"provider":"qdrant",
   "create_payload":{"vectors":{"dense":{"size":8,"distance":"Cosine"}}}})
 header,resources=manifest_api.load(manifest.path);destination_resource=next(item for item in resources if item.resource_type=="collection" and item.identity==migrate_destination)
 manifest_api.write_setup_intent(header,destination_resource)
 process=isolation.spawn_owned_process(manifest,owned,[descriptor["binary"],"mcp","--transport","http"],env=env)
 wait(base+"/v1/status",process)
 result=None;teardown_report=run_root/"launcher/admin-teardown-report.json"
 try:
  qbefore=hashlib.sha256(qstate.read_bytes()).hexdigest()
  _,prune=call(base,token,"/v1/prune/plan",{"target":f"collection:{prune_collection}","generation":None},{200})
  if prune.get("destructive") is not True or prune.get("requires_admin") is not True or not isinstance(prune.get("job_id"),str):
   raise RuntimeError("production prune plan omitted destructive/admin/job identity")
  _,stored_prune=get(base,token,f"/v1/prune/plans/{prune['job_id']}",{200})
  if stored_prune.get("plan")!=prune or not stored_prune.get("inventory_checksum") or not stored_prune.get("expires_at_utc"):
   raise RuntimeError("persisted prune plan did not preserve exact executable contract")
  # Mismatched confirmation must fail without touching the provider.
  _,mismatch=call(base,token,"/v1/prune/exec",{"prune_plan_id":"axon_e2e_mismatched_plan","confirm":True,
    "reason":"owned E2E mismatch probe"},{400,403,404,409})
  if hashlib.sha256(qstate.read_bytes()).hexdigest()!=qbefore:raise RuntimeError("mismatched prune mutated provider")
  _,prune_receipt=call(base,token,"/v1/prune/exec",{"prune_plan_id":prune["job_id"],"confirm":True,
    "reason":"owned E2E prune execution"},{200})
  if prune_receipt.get("status") not in {"completed","succeeded"} or prune_receipt.get("deleted_counts",{}).get("vector_points",0)<1:
   raise RuntimeError(f"owned prune did not produce a real deletion receipt: {prune_receipt}")
  pruned_state=json.loads(qstate.read_text())["collections"][prune_collection]["points"]
  if pruned_state:raise RuntimeError("successful owned prune left vector points behind")
  header,resources=manifest_api.load(manifest.path);adapter=providers.build(provider_config,header,manifest_api)["collection"]
  prune_resource=next(item for item in resources if item.resource_type=="collection" and item.identity==prune_collection)
  adapter.provision_ownership_marker(prune_resource);manifest_api.write_provider_ledger(header,prune_resource,adapter._provider_state(prune_resource))
  # Exercise the real "reset" plan/get/execute contract over HTTP.
  _,reset=call(base,token,"/v1/reset/plan",{"stores":["artifacts"],"dry_run":True,"reason":"owned E2E reset"},{200})
  plan_id=reset.get("plan_id");expiry=reset.get("expires_at_utc")
  if not isinstance(plan_id,str) or not isinstance(expiry,str) or not isinstance(reset.get("inventory_checksum"),str):
   raise RuntimeError("production reset plan omitted bound identity/checksum/expiry")
  expires=int(datetime.datetime.fromisoformat(expiry.replace("Z","+00:00")).timestamp()*1000)
  marker=f"{admin_run_id}:{descriptor['collection_marker']['ownership_generation']}"
  payload=guard.plan_payload(admin_run_id,1,[{"type":"reset_plan","identity":f"{admin_run_id}_artifacts",
    "ownership_marker":marker}],expires);key=os.urandom(32);confirmation=guard.Confirmation(guard.digest(payload,key),admin_run_id,1)
  def refetch():
   _,stored=get(base,token,f"/v1/reset/plans/{plan_id}",{200})
   actual=stored.get("reset_plan")
   if actual!=reset:raise guard.GuardError("production reset plan changed before execution")
   state=json.loads(qstate.read_text());collection=state.get("collections",{}).get(run_id,{})
   marker_point=collection.get("points",{}).get(descriptor["collection_marker"]["marker_id"],{})
   owner=marker_point.get("payload",{}).get("run_id");generation=marker_point.get("payload",{}).get("ownership_generation")
   if (owner,generation)!=(run_id,descriptor["collection_marker"]["ownership_generation"]):
    raise guard.GuardError("provider ownership marker changed")
   return payload
  receipts=[]
  def delete_one(_target):
   process.process.terminate();process.process.wait(timeout=8)
   _,receipt=cli_json(descriptor["binary"],["reset","--stores","artifacts","--plan-id",plan_id,"--yes","--json"],env)
   receipts.append(receipt)
  guard.execute(refetch,confirmation,key,delete_one)
  if not receipts or receipts[0].get("dry_run") is True:raise RuntimeError("owned reset did not execute")
  # A fresh second plan/execute proves repeated owned cleanup is idempotent.
  _,again_result=cli_json(descriptor["binary"],["reset","--stores","artifacts","--json"],env)
  again=again_result.get("reset_plan",again_result)
  _,again_receipt=cli_json(descriptor["binary"],["reset","--stores","artifacts","--plan-id",again["plan_id"],"--yes","--json"],env)
  # Migrate the owned source, then prove a foreign source is rejected.
  migrate=subprocess.run([descriptor["binary"],"migrate","--from",migrate_source,"--to",migrate_destination,"--json"],
    env=env,capture_output=True,text=True,timeout=10)
  if migrate.returncode!=0:raise RuntimeError(f"owned migrate failed: {migrate.stderr}")
  migrate_receipt=json.loads(migrate.stdout)
  header,resources=manifest_api.load(manifest.path);adapter=providers.build(provider_config,header,manifest_api)["collection"]
  destination_resource=next(item for item in resources if item.resource_type=="collection" and item.identity==migrate_destination)
  adapter.provision_ownership_marker(destination_resource);manifest_api.write_provider_ledger(header,destination_resource,adapter._provider_state(destination_resource))
  migrated=json.loads(qstate.read_text())["collections"].get(migrate_destination,{})
  source_count=len(json.loads(qstate.read_text())["collections"][migrate_source]["points"])
  migrated_payloads=[point.get("payload",{}) for point in migrated.get("points",{}).values()]
  if migrate_receipt.get("points_migrated")!=source_count or len(migrated.get("points",{}))<source_count or not migrated.get("named") \
     or not any(item.get("chunk_text")=="owned migration fixture" for item in migrated_payloads):
   raise RuntimeError(f"owned migrate did not create a named destination: {migrate_receipt}")
  foreign=subprocess.run([descriptor["binary"],"migrate","--from","operator-production","--to",admin_run_id,"--json"],
    env=env,capture_output=True,text=True,timeout=10)
  if foreign.returncode==0:raise RuntimeError("migrate accepted non-owned source")
  report={"schema_version":1,"passed":True,"prune_plan":prune,"stored_prune_plan":stored_prune,"prune_mismatch":mismatch,"reset_receipt":receipts[0],
    "prune_receipt":prune_receipt,"reset_repeat":again_receipt,"migrate_receipt":migrate_receipt,"foreign_migrate_exit":foreign.returncode,"plan_digest":confirmation.digest,
    "manifest":str(manifest.path)}
  (evidence/"admin-report.json").write_text(json.dumps(report,indent=2,sort_keys=True)+"\n")
  handoff=owned/"teardown-handoff.json";handoff.write_text(json.dumps({"manifest":str(manifest.path),"report":str(teardown_report)})+"\n")
  manifest.register("temp_path",str(handoff));result={"result":"pass","manifest":str(manifest.path),"handoff":str(handoff)}
 finally:
  cleaned=subprocess.run([sys.executable,str(ROOT/"scripts/e2e/lib/teardown.py"),str(manifest.path),"--report",str(teardown_report),"--provider-config",str(provider_config)],
    cwd=ROOT,capture_output=True,text=True,timeout=30)
  if cleaned.returncode:raise RuntimeError(f"admin .15 teardown failed: {teardown_report.read_text()}")
 if result is None:raise RuntimeError("admin entry produced no result")
 result["teardown_report"]=str(teardown_report);print(json.dumps(result));return 0
if __name__=="__main__":raise SystemExit(main())
