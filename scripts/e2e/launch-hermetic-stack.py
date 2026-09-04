#!/usr/bin/env python3
"""Launch one allocation-bound hermetic Axon stack and emit its live descriptor."""
from __future__ import annotations
import hashlib,importlib.util,json,os,secrets,socket,sqlite3,subprocess,sys,time,urllib.request,uuid
from contextlib import closing
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
ISOLATION_SPEC=importlib.util.spec_from_file_location("axon_e2e_launcher_isolation",ROOT/"scripts/e2e/lib/run-isolation.py")
assert ISOLATION_SPEC and ISOLATION_SPEC.loader
isolation=importlib.util.module_from_spec(ISOLATION_SPEC);sys.modules[ISOLATION_SPEC.name]=isolation;ISOLATION_SPEC.loader.exec_module(isolation)
def register_for_outer_cleanup(manifest: Path) -> None:
 registry=os.environ.get("AXON_E2E_CLEANUP_REGISTRY")
 if not registry:return
 report=manifest.parent/"outer-cleanup-registration.json"
 completed=subprocess.run([sys.executable,str(ROOT/"scripts/e2e/cleanup-owned-runs.py"),"--registry",registry,
                           "--register-manifest",str(manifest),"--report",str(report)],cwd=ROOT,
                          capture_output=True,text=True,timeout=15,check=False)
 if completed.returncode:raise RuntimeError("durable outer-cleanup manifest registration failed")
 try:value=json.loads(report.read_text())
 except (OSError,json.JSONDecodeError) as error:raise RuntimeError("outer-cleanup registration receipt is invalid") from error
 if value.get("success") is not True:raise RuntimeError("outer-cleanup registration was refused")
def wait(url,process,token=None):
 for _ in range(200):
  if process.poll() is not None:raise RuntimeError(f"owned process exited before ready: {process.returncode}")
  try:
   request=urllib.request.Request(url,headers={"Authorization":f"Bearer {token}"} if token else {})
   urllib.request.urlopen(request,timeout=.1).close();return
  except OSError:time.sleep(.025)
 raise RuntimeError(f"owned endpoint did not become ready: {url}")
def wait_port(port,process):
 for _ in range(1200):
  if process.poll() is not None:raise RuntimeError(f"owned process exited before ready: {process.returncode}")
  try:
   with socket.create_connection(("127.0.0.1",port),timeout=.1):return
  except OSError:time.sleep(.025)
 raise RuntimeError(f"owned port did not become ready: {port}")
def require(value,key):
 item=value.get(key)
 if not isinstance(item,str) or not item.strip():raise RuntimeError(f"allocation omitted {key}")
 return item
def fixture_identities(run_id: str):
 suffix=hashlib.sha256(run_id.encode()).hexdigest()[:16]
 return {"source_id":f"source-{suffix}","node_ids":[f"node-atlas-{suffix}",f"node-amber-{suffix}"],
  "edge_id":f"edge-atlas-amber-{suffix}","evidence_id":f"evidence-atlas-{suffix}",
  "conflict_id":f"conflict-atlas-{suffix}","item_key":f"atlas-{suffix}",
  "document_id":f"doc-atlas-{suffix}","chunk_id":f"chunk-atlas-{suffix}"}
def seed_stateful_database(path: Path, canonical_uri: str, identities):
 now="2026-01-01T00:00:00Z";source=identities["source_id"];first,second=identities["node_ids"];edge=identities["edge_id"]
 with closing(sqlite3.connect(path)) as db, db:
  db.execute("PRAGMA foreign_keys=ON")
  db.execute("INSERT OR REPLACE INTO sources VALUES(?,?,?,?,?)",(source,"1",json.dumps({"canonical_uri":canonical_uri,"source_id":source}),now,now))
  db.execute("INSERT OR REPLACE INTO source_generations(source_id,generation,sequence,status,publish_state,generation_json,created_at,published_at) VALUES(?,?,?,?,?,?,?,?)",
             (source,"1",1,"published","committed",json.dumps({"source_id":source,"generation":"1"}),now,now))
  nodes=((first,"source","atlas",canonical_uri,"Atlas fixture"),(second,"concept","amber","urn:axon:e2e:amber","Amber signal"))
  for node_id,kind,stable,uri,name in nodes:
   db.execute("INSERT OR REPLACE INTO graph_nodes VALUES(?,?,?,?,?,?,?,?,?,?,?)",
              (node_id,kind,stable,uri,name,"e2e_fixture",1.0,"{}",json.dumps([source]),now,now))
  edge_metadata=json.dumps({"conflict_ids":[identities["conflict_id"]],"fixture_source_id":source})
  db.execute("INSERT OR REPLACE INTO graph_edges VALUES(?,?,?,?,?,?,?,?,?)",(edge,"emits",first,second,"e2e_fixture",1.0,edge_metadata,now,now))
  db.execute("INSERT OR REPLACE INTO graph_evidence VALUES(?,?,?,?,?,?,?,?,?,?,?)",
             (identities["evidence_id"],edge,"source",source,identities["item_key"],identities["document_id"],identities["chunk_id"],json.dumps({"line_start":1,"line_end":2}),"Atlas emits amber",1.0,"{}"))
  db.execute("INSERT OR REPLACE INTO graph_conflicts VALUES(?,?,?,?,?,?,?,?,?)",
             (identities["conflict_id"],"edge",edge,"authority","e2e_fixture","e2e_fixture_competing","e2e_fixture","e2e_fixture",now))
  for kind,value,node in (("canonical_uri",canonical_uri,first),("stable_key","atlas",first),("stable_key","amber",second)):
   db.execute("INSERT OR REPLACE INTO graph_aliases VALUES(?,?,?)",(kind,value,node))
  db.execute("INSERT OR REPLACE INTO graph_publication_state(source_id,committed_epoch,updated_at) VALUES(?,?,?)",(source,1,now));db.commit()
def _launch(allocation):
 run_id=require(allocation,"run_id");collection=require(allocation,"collection")
 run_root=Path(require(allocation,"run_root")).resolve();generation=require(allocation,"ownership_generation")
 if not run_id.startswith("axon_e2e_") or not collection.startswith("axon_e2e_"):raise RuntimeError("allocation is not E2E namespaced")
 run_root.mkdir(parents=True,exist_ok=True);data=run_root/"data";data.mkdir(exist_ok=True);logs=run_root/"launcher";logs.mkdir(exist_ok=True)
 manifest=isolation.Manifest.create(run_root.parent/"ownership-manifests",run_id,data)
 register_for_outer_cleanup(manifest.path)
 manifest.register("temp_path",str(run_root));manifest.register("data_dir",str(data))
 binary=Path(os.environ.get("AXON_E2E_REAL_AXON_BIN",ROOT/"target/debug/axon")).resolve()
 if not binary.is_file():raise RuntimeError("built Axon binary is unavailable")
 leases_root=run_root/"leases"
 reservations=[isolation.allocate_port(leases_root,run_id,manifest) for _ in range(3)]
 qport,pport,hport=(reservation.port for reservation in reservations)
 marker_id=str(uuid.uuid5(uuid.NAMESPACE_URL,f"axon-e2e-marker:{run_id}"));http_token=secrets.token_urlsafe(48)
 identities=fixture_identities(run_id)
 points={marker_id:{"id":marker_id,"vector":{"dense":[0.0]*8},"payload":{"_axon_e2e_owner":run_id,
   "run_id":run_id,"ownership_generation":generation,"resource_type":"collection","payload_contract_version":"2026-07-01"}}}
 if allocation.get("seed_retrieval"):
  corpus=(ROOT/"tests/e2e/corpus/v1/documents/micro/atlas-v1.md").resolve();pid="11111111-1111-4111-8111-111111111111"
  points[pid]={"id":pid,"vector":{"dense":[.125]*8},"payload":{"chunk_id":"chunk-atlas","document_id":"doc-atlas",
   "source_id":identities["source_id"],"source_item_key":identities["item_key"],"chunk_text":"The Atlas beacon emits an amber signal.","payload_contract_version":"2026-07-01",
   "source_generation":1,"committed_generation":1,"job_id":"22222222-2222-4222-8222-222222222222",
   "redaction_status":"clean","visibility":"public","redaction_version":"e2e-v1","redacted_field_count":0,
   "dropped_field_count":0,"detector_count":0,"detector_names":[],"embedding_refs":[],
   "chunk_locator":{"canonical_uri":corpus.as_uri(),"range":{"line_start":1,"line_end":2}},
   "source_range":{"line_start":1,"line_end":2},"canonical_uri":corpus.as_uri()}}
 state=logs/"qdrant.json";state.write_text(json.dumps({"collections":{collection:{"size":8,"named":True,"sparse":True,
  "indexes":{},"snapshots":{},"points":points}},"aliases":{}}))
 env={**os.environ,"AXON_DATA_DIR":str(data),"AXON_SQLITE_PATH":str(data/"jobs.db"),"QDRANT_URL":f"http://127.0.0.1:{qport}",
  "TEI_URL":f"http://127.0.0.1:{pport}","AXON_LLM_BACKEND":"openai-compat","AXON_OPENAI_BASE_URL":f"http://127.0.0.1:{pport}/v1",
  "AXON_SYNTHESIS_OPENAI_MODEL":"e2e-owned","AXON_OPENAI_API_KEY":"","AXON_SEARXNG_URL":f"http://127.0.0.1:{pport}",
  "AXON_CHROME_REMOTE_URL":f"http://127.0.0.1:{pport}","AXON_HTTP_HOST":"127.0.0.1",
  "AXON_HTTP_PORT":str(hport),"AXON_HTTP_TOKEN":http_token,"AXON_COLLECTION":collection,"AXON_MEMORY_COLLECTION":collection,"AXON_E2E_RUN_ID":run_id}
 mcporter_config=logs/"mcporter.json";mcp_name=f"axon-owned-{hashlib.sha256(run_id.encode()).hexdigest()[:12]}"
 mcporter_config.write_text(json.dumps({"mcpServers":{mcp_name:{"baseUrl":f"http://127.0.0.1:{hport}/mcp",
   "headers":{"Authorization":f"Bearer {http_token}"}}}},sort_keys=True)+"\n");os.chmod(mcporter_config,0o600)
 env["MCPORTER_CONFIG"]=str(mcporter_config)
 fixture_source=str(allocation.get("fixture_source") or f"http://127.0.0.1:{pport}/corpus/atlas")
 if allocation.get("stateful") or allocation.get("seed_stateful"):
  # Let the real runtime migrate a new database, stop it, then seed before the
  # tested server opens SQLite. Seeding after startup can leave an eager Linux
  # connection pinned to the pre-seed inode.
  reservations[2].close()
  bootstrap=isolation.spawn_owned_process(manifest,run_root,[str(binary),"mcp","--transport","http"],env=env)
  wait_port(hport,bootstrap.process)
  bootstrap.process.terminate();bootstrap.process.wait(timeout=10)
  seed_stateful_database(data/"jobs.db",fixture_source,identities)
 processes=[];descriptor_path=logs/"descriptor.json"
 descriptor={"schema":1,"run_id":run_id,"run_root":str(run_root),"status":"launching",
  "process_ids":{},"ports":[qport,pport,hport],"ownership_manifest":str(manifest.path),
  "cleanup_report":str(manifest.path.parent/"cleanup-report.json"),"descriptor_path":str(descriptor_path)}
 descriptor["teardown_handle"]={"command":[sys.executable,str(ROOT/"scripts/e2e/teardown-hermetic-stack.py"),str(descriptor_path)]}
 descriptor_path.write_text(json.dumps(descriptor,indent=2,sort_keys=True)+"\n");os.chmod(descriptor_path,0o600)
 try:
  specs=(("qdrant",[sys.executable,str(ROOT/"tests/e2e/fixtures/teardown/qdrant_contract.py"),"--port",str(qport),"--state",str(state)]),
         ("providers",[sys.executable,str(ROOT/"tests/e2e/scenarios/retrieval/provider_double.py"),"--port",str(pport),"--mode","discovery"]),
         ("axon_http_mcp",[str(binary),"mcp","--transport","http"]))
  for index,(name,argv) in enumerate(specs):
   # Keep every allocation socket bound until the exact child that owns it is
   # ready to spawn. The signed lease remains registered through teardown.
   reservations[index].close()
   owned=isolation.spawn_owned_process(manifest,run_root,argv,env=env,capture_prefix=logs/name);processes.append((name,owned.process))
   descriptor["process_ids"][name]=owned.process.pid
   descriptor_path.write_text(json.dumps(descriptor,indent=2,sort_keys=True)+"\n")
  wait(f"http://127.0.0.1:{qport}/readyz",processes[0][1]);wait(f"http://127.0.0.1:{pport}/health",processes[1][1]);wait(f"http://127.0.0.1:{hport}/v1/status",processes[2][1],http_token)
  manifest.register("sqlite",str(data/"jobs.db"))
  bound={key:env[key] for key in ("AXON_COLLECTION","AXON_MEMORY_COLLECTION","QDRANT_URL","TEI_URL","AXON_OPENAI_BASE_URL",
    "AXON_OPENAI_API_KEY","AXON_LLM_BACKEND","AXON_SYNTHESIS_OPENAI_MODEL","AXON_SEARXNG_URL","AXON_CHROME_REMOTE_URL",
    "AXON_HTTP_HOST","AXON_HTTP_PORT","AXON_HTTP_TOKEN","AXON_DATA_DIR","AXON_SQLITE_PATH","MCPORTER_CONFIG")}
  descriptor.update({"schema":1,"run_id":run_id,"run_root":str(run_root),"status":"running",
   "http_base_url":f"http://127.0.0.1:{hport}","http_endpoint":f"http://127.0.0.1:{hport}","mcp_endpoint":f"http://127.0.0.1:{hport}/mcp",
   "mcp_selector":f"{mcp_name}.axon","qdrant_url":env["QDRANT_URL"],"environment":bound,"bindings":bound,
   "environment_sha256":hashlib.sha256(json.dumps(bound,sort_keys=True,separators=(",",":")).encode()).hexdigest(),
   "http_token_sha256":hashlib.sha256(http_token.encode()).hexdigest(),
   "binary":str(binary),"binary_sha256":hashlib.sha256(binary.read_bytes()).hexdigest(),
   "process_ids":{name:process.pid for name,process in processes},"ports":[qport,pport,hport],
   "fixture_source":fixture_source,"fixture_source_id":identities["source_id"],"fixture_graph":identities,
   "ownership_manifest":str(manifest.path),"cleanup_report":str(manifest.path.parent/"cleanup-report.json"),
   "collection_marker":{"collection":collection,"marker_id":marker_id,"point_id":marker_id,"run_id":run_id,
    "ownership_generation":generation,"provider":"qdrant"},"descriptor_path":str(descriptor_path)})
  descriptor_path.write_text(json.dumps(descriptor,indent=2,sort_keys=True)+"\n");os.chmod(descriptor_path,0o600)
  print(json.dumps(descriptor,sort_keys=True));return 0
 except Exception as primary:
  for reservation in reservations:
   try:reservation.close()
   except OSError:pass
  completed=subprocess.run(descriptor["teardown_handle"]["command"],cwd=ROOT,capture_output=True,text=True,timeout=90,check=False)
  if completed.returncode:
   raise RuntimeError(f"launcher failed ({type(primary).__name__}) and canonical teardown failed") from primary
  raise
def main():
 allocation=json.load(sys.stdin);run_id=require(allocation,"run_id");run_root=Path(require(allocation,"run_root")).resolve()
 try:return _launch(allocation)
 except Exception as primary:
  manifest_path=run_root.parent/"ownership-manifests"/run_id/"resources.jsonl"
  if run_root.exists() and manifest_path.is_file():
   report_path=manifest_path.parent/"cleanup-report.json"
   completed=subprocess.run([sys.executable,str(ROOT/"scripts/e2e/lib/teardown.py"),str(manifest_path),"--report",str(report_path)],cwd=ROOT,capture_output=True,text=True,timeout=90,check=False)
   if completed.returncode:raise RuntimeError(f"launcher setup failed ({type(primary).__name__}) and canonical teardown failed") from primary
  raise
if __name__=="__main__":raise SystemExit(main())
