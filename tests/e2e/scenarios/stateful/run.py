#!/usr/bin/env python3
"""Executable graph/memory/watch/resource acceptance over real Axon transports."""
from __future__ import annotations
import argparse, atexit, base64, hashlib, importlib.util, json, os, subprocess, sys, tempfile, urllib.error, urllib.parse, urllib.request
import time
from pathlib import Path
from typing import Any

ROOT=Path(__file__).resolve().parents[4]
def load(name,path):
 spec=importlib.util.spec_from_file_location(name,path); module=importlib.util.module_from_spec(spec); sys.modules[name]=module; spec.loader.exec_module(module); return module
isolation=load("stateful_isolation",ROOT/"scripts/e2e/lib/run-isolation.py")
manifest_api=load("stateful_manifest",ROOT/"scripts/e2e/lib/resource-manifest.py")
provider_api=load("stateful_providers",ROOT/"scripts/e2e/lib/provider-adapters.py")
graph=load("stateful_graph",ROOT/"tests/e2e/scenarios/graph/pack.py")
memory=load("stateful_memory",ROOT/"tests/e2e/scenarios/memory/pack.py")
watches=load("stateful_watches",ROOT/"tests/e2e/scenarios/watches/pack.py")
resources=load("stateful_resources",ROOT/"tests/e2e/scenarios/resources/pack.py")

class RunnerError(RuntimeError): pass
TRANSPORT_CATALOG={
 "graph.source":{"cli":True,"mcp":True,"http":True},
 "collections.list":{"cli":True,"mcp":True,"http":True},
 "providers.list":{"cli":True,"mcp":False,"http":True,"nonapplicability":"not present in generated MCP preferred actions"},
 "capabilities":{"cli":True,"mcp":False,"http":True,"nonapplicability":"not present in generated MCP preferred actions"},
 "uploads.lifecycle":{"cli":True,"mcp":True,"http":True},
 "memory.mutate":{"cli":True,"mcp":True,"http":True},
 "watch.mutate":{"cli":True,"mcp":True,"http":True},
}

def require_fields(value, fields, operation):
 if not isinstance(value,dict): raise RunnerError(f"{operation} response was not an object")
 missing=[field for field in fields if field not in value]
 if missing: raise RunnerError(f"{operation} response omitted {','.join(missing)}")
 return value

def expect_classified(operation, call, accepted):
 try: call()
 except (RunnerError,ValueError) as error:
  encoded=str(error)
  if not any(code in encoded for code in accepted): raise RunnerError(f"{operation} lacked stable classification: {encoded}")
  return encoded
 raise RunnerError(f"{operation} unexpectedly succeeded")

def identity_set(value, keys):
 return {item for key in keys for item in ids(value,key)}
class Cli:
 def __init__(self,binary,env,timeout): self.binary=Path(binary).resolve(); self.env=env; self.timeout=timeout
 def __call__(self,*args,ok=True,stdin=None):
  for attempt in range(4):
   run=subprocess.run([str(self.binary),*map(str,args)],cwd=ROOT,env=self.env,input=json.dumps(stdin) if stdin else None,capture_output=True,text=True,timeout=self.timeout)
   if run.returncode and "database is locked" in run.stderr.lower() and attempt < 3:
    time.sleep(.05*(attempt+1)); continue
   break
  value=None
  for stream in (run.stdout,run.stderr):
   try: candidate=json.loads(stream)
   except json.JSONDecodeError: candidate=None
   if isinstance(candidate,dict): value=candidate; break
   for line in reversed(stream.splitlines()):
    try: candidate=json.loads(line)
    except json.JSONDecodeError: continue
    if isinstance(candidate,dict): value=candidate; break
   if value is not None: break
  if value is None and run.returncode==0 and run.stdout.strip()=="null": value={"result":None,"code":"memory.not_found"}
  if ok and run.returncode: raise RunnerError(f"CLI failed {args}: {run.stderr}")
  if not ok and run.returncode==0: raise RunnerError(f"CLI negative succeeded: {args}")
  if value is None and not ok and run.stderr.strip():
   message=run.stderr.strip(); lowered=message.lower()
   code=("graph.not_found" if "graph" in lowered and "not found" in lowered else
         "memory.not_found" if "memory" in lowered and "not found" in lowered else
         "watch.not_found" if "watch" in lowered and "not found" in lowered else "cli.invalid")
   value={"code":code,"error":message,"returncode":run.returncode}
  if value is None: raise RunnerError(f"CLI lacked structured JSON: {args}; stdout={run.stdout[-1000:]!r}; stderr={run.stderr[-1000:]!r}")
  return value
class Http:
 def __init__(self,base,token,timeout): self.base=base.rstrip('/'); self.token=token; self.timeout=timeout; self.upload_content_types={}
 def __call__(self,method,path,body=None):
  headers={"Accept":"application/json"}; data=None
  if self.token: headers["Authorization"]=f"Bearer {self.token}"
  if isinstance(body,(bytes,bytearray)):
   upload_id=path.split("/")[3] if path.startswith("/v1/uploads/") and path.endswith("/content") else None
   headers["Content-Type"]=self.upload_content_types.get(upload_id,"application/octet-stream"); data=bytes(body)
  elif body is not None: headers["Content-Type"]="application/json"; data=json.dumps(body).encode()
  for attempt in range(6):
   request=urllib.request.Request(self.base+path,data=data,headers=headers,method=method)
   try:
    with urllib.request.urlopen(request,timeout=self.timeout) as response:
     raw=response.read(); content_type=response.headers.get_content_type()
     if content_type == "application/json" or content_type.endswith("+json") or raw.lstrip().startswith((b"{",b"[")): result=json.loads(raw) if raw else {}
     else: result=raw
     if method=="POST" and path=="/v1/uploads" and isinstance(body,dict) and isinstance(result,dict):
      upload_id=result.get("upload_id"); declared=body.get("content_type")
      if isinstance(upload_id,str) and isinstance(declared,str): self.upload_content_types[upload_id]=declared
     return result
   except urllib.error.HTTPError as error:
    try:
     raw=error.read()
     try: value=json.loads(raw) if raw else {"code":f"http.{error.code}"}
     except json.JSONDecodeError: value={"code":f"http.{error.code}","message":raw.decode(errors="replace")}
    finally:error.close()
    if "upload.busy" in json.dumps(value) and attempt < 5:
     time.sleep(.02*(attempt+1)); continue
    raise RunnerError(json.dumps(value,sort_keys=True))
class Mcp:
 def __init__(self,binary,selector,timeout,env): self.binary=Path(binary).resolve(); self.selector=selector; self.timeout=timeout; self.env=env
 def call(self,args):
  insecure=["--allow-http"] if self.selector.startswith("http://") else []
  run=subprocess.run([str(self.binary),"call",self.selector,*insecure,"--args",json.dumps(args,separators=(',',':')),"--output","json"],cwd=ROOT,env=self.env,capture_output=True,text=True,timeout=self.timeout)
  if run.returncode: raise RunnerError(f"MCP failed: {(run.stderr or run.stdout).strip()}")
  value=json.loads(run.stdout)
  for _ in range(8):
   if isinstance(value,dict) and isinstance(value.get("content"),list): value=json.loads(value["content"][0]["text"]); continue
   for key in ("result","data","inline"):
    if isinstance(value,dict) and isinstance(value.get(key),dict): value=value[key]; break
   else: break
  if not isinstance(value,dict): raise RunnerError("MCP payload was not an object")
  return value

class MemoryDispatch:
 CLI_SUBACTIONS={"remember","list","search","show","link","supersede","context"}
 def __init__(self,cli,mcp): self.cli,self.mcp=cli,mcp
 def __call__(self,*args,**kwargs):
  operation=args[1]
  if operation in self.CLI_SUBACTIONS: return self.cli(*args,**kwargs)
  request={"action":"memory","subaction":operation,"response_mode":"inline"}
  values=list(args[2:]); flag=lambda name: values[values.index(name)+1] if name in values else None
  if operation in {"reinforce","pin","archive","forget"}: request["id"]=values[0]
  if operation=="reinforce": request["amount"]=float(flag("--amount")); request["reason"]=flag("--reason")
  if operation=="pin": request["pinned"]=True; request["reason"]=flag("--reason")
  if operation in {"archive","forget"}: request["reason"]=flag("--reason")
  if operation=="contradict": request.update(source_id=values[0],target_id=values[1],reason=flag("--reason"))
  if operation=="review": request["limit"]=int(flag("--limit"))
  if operation=="compact": request.update(memory_ids=values[:2],strategy="concatenate",archive_sources="--archive-sources" in values)
  if operation=="export": request.update(include_archived=True,include_working=True)
  if operation=="import": request.update(records=json.loads(Path(values[0]).read_text()),import_mode=flag("--mode"),dry_run=False)
  try: result=self.mcp.call(request)
  except RunnerError:
   if kwargs.get("ok") is False: return {"code":"memory.invalid","error":"MCP memory request rejected"}
   raise
  if operation=="export":
   output=flag("--output"); records=result.get("records")
   if not isinstance(records,list): raise RunnerError("MCP memory export omitted records")
   Path(output).write_text(json.dumps(records)); return result
  return result

def registerer(manifest,run_id,collection,ownership_generation):
 serial=[0];seen=set()
 def register(kind,identity):
  if (kind,identity) in seen:return
  seen.add((kind,identity))
  serial[0]+=1; operation=f"{run_id}_stateful_{serial[0]}"; manifest.register("operation",operation,{"run_id":run_id,"scenario_id":"stateful.pack"})
  metadata={"run_id":run_id,"attempt":1,"scenario_id":"stateful.pack",
      "request_id":f"stateful-{serial[0]}","origin":"server_response",
      "parent_resource_type":"operation","parent_identity":operation,
      "ownership_generation":ownership_generation}
  if kind=="point": metadata.update(collection=collection,ownership_generation=ownership_generation)
  manifest.register(kind,identity,metadata)
 return register
def ids(value,key):
 found=[]
 if isinstance(value,dict):
  for name,item in value.items():
   if name==key and isinstance(item,str): found.append(item)
   found.extend(ids(item,key))
 elif isinstance(value,list):
  for item in value: found.extend(ids(item,key))
 return found
def validate_transport_catalog():
 cli_names={item["name"] for item in json.loads((ROOT/"docs/reference/cli/commands.json").read_text())["commands"]}
 routes=json.loads((ROOT/"docs/reference/rest/openapi.json").read_text())["routes"]
 rest_paths={(item.get("method","GET").upper(),item["path"]) for item in routes}
 mcp_handlers=(ROOT/"crates/axon-mcp/src/server/handlers_system.rs").read_text()
 cli_required={"graph source","collections list","providers list","capabilities","memory remember","memory link","watch create","watch exec","uploads create","uploads complete","uploads abort"}
 rest_required={("GET","/v1/collections"),("GET","/v1/providers"),("GET","/v1/capabilities"),("POST","/v1/uploads"),("POST","/v1/watches"),("POST","/v1/watches/{watch_id}/exec"),("POST","/v1/memories/{memory_id}/link")}
 if not cli_required <= cli_names: raise RunnerError("generated CLI applicability drift")
 if not rest_required <= rest_paths: raise RunnerError("generated REST applicability drift")
 for family in ("graph","collections","uploads","watch"):
  if f'"{family}": [' not in mcp_handlers: raise RunnerError(f"generated MCP handler catalog omitted {family}")
 mcp_text=(ROOT/"docs/reference/mcp/tool-schema.md").read_text()
 if "- `memory`:" not in mcp_text:
  raise RunnerError("generated MCP applicability drift")
 return {"cli":sorted(cli_required),"rest":sorted(f"{method} {path}" for method,path in rest_required)}

def mcp_mutation_lifecycles(mcp,source,namespace,register):
 content=f"mcp {namespace}".encode(); digest=__import__('hashlib').sha256(content).hexdigest()
 created=require_fields(mcp.call({"action":"uploads","subaction":"create","filename":f"{namespace}-mcp.txt","content_type":"text/plain","size_bytes":len(content),"purpose":"source_artifact","sha256":digest,"response_mode":"inline"}),("upload_id",),"MCP upload create")
 upload_id=created["upload_id"]; register("upload",upload_id)
 received=mcp.call({"action":"uploads","subaction":"put_content","upload_id":upload_id,"content":base64.b64encode(content).decode(),"sha256":digest,"response_mode":"inline"})
 if received.get("status") != "received" or received.get("sha256") != digest: raise RunnerError("MCP upload put semantic mismatch")
 completed=require_fields(mcp.call({"action":"uploads","subaction":"complete","upload_id":upload_id,"sha256":digest,"response_mode":"inline"}),("upload_id","artifact_id","source_ref"),"MCP upload complete")
 if completed["upload_id"] != upload_id: raise RunnerError("MCP upload completion identity mismatch")
 register("artifact",completed["artifact_id"])
 watch=require_fields(mcp.call({"action":"watch","subaction":"create","source":source,"every_seconds":3600,"response_mode":"inline"}),("watch_id","source_id","canonical_uri","enabled"),"MCP watch create")
 register("watch",watch["watch_id"])
 if watch["enabled"] is not True: raise RunnerError("MCP watch create state mismatch")
 paused=mcp.call({"action":"watch","subaction":"pause","id":watch["watch_id"],"response_mode":"inline"})
 resumed=mcp.call({"action":"watch","subaction":"resume","id":watch["watch_id"],"response_mode":"inline"})
 if paused.get("enabled") is not False or resumed.get("enabled") is not True: raise RunnerError("MCP watch transition mismatch")
 deleted=mcp.call({"action":"watch","subaction":"delete","id":watch["watch_id"],"response_mode":"inline"})
 if deleted.get("deleted") is not True: raise RunnerError("MCP watch delete semantic mismatch")
 return {"upload_id":upload_id,"artifact_id":completed["artifact_id"],"watch_id":watch["watch_id"],"terminal":"completed"}

def http_watch_lifecycle(http,source,namespace,register):
 request={"source":source,"schedule":{"every_seconds":3600},"embed":True,"options":{"values":{}},"limits":{},"metadata":{"e2e_namespace":namespace},"scope":"page","collection":namespace,"enabled":True}
 created=http("POST","/v1/watches",request)
 summary=created.get("summary",created)
 watch_id=summary.get("watch_id")
 if not isinstance(watch_id,str) or summary.get("enabled") is not True: raise RunnerError("HTTP watch create DTO/state mismatch")
 register("watch",watch_id)
 paused=http("POST",f"/v1/watches/{watch_id}/pause",None); resumed=http("POST",f"/v1/watches/{watch_id}/resume",None)
 def enabled(value): return value.get("summary",value).get("enabled")
 if enabled(paused) is not False or enabled(resumed) is not True: raise RunnerError("HTTP watch transition mismatch")
 detail=http("GET",f"/v1/watches/{watch_id}",None)
 persisted=detail.get("summary",detail)
 if persisted.get("watch_id") != watch_id or persisted.get("canonical_uri") != source or persisted.get("scope") != "page" or persisted.get("schedule",{}).get("every_seconds") != 3600 or persisted.get("enabled") is not True or not isinstance(persisted.get("source_id"),str) or not isinstance(persisted.get("adapter"),dict):
  raise RunnerError("HTTP watch persisted-state oracle failed")
 deleted=http("DELETE",f"/v1/watches/{watch_id}",None)
 if deleted != {"watch_id":watch_id,"deleted":True}: raise RunnerError("HTTP watch delete semantic mismatch")
 return {"watch_id":watch_id,"terminal":"deleted"}

def http_memory_lifecycle(http,namespace,register):
 def remember(suffix):
  value=http("POST","/v1/memories",{"body":f"{namespace} http {suffix}","memory_type":"fact","confidence":0.9})
  memory=value.get("memory",value); memory_id=memory.get("id")
  if not isinstance(memory_id,str) or memory.get("status") != "active": raise RunnerError(f"HTTP memory remember DTO/state mismatch: {json.dumps(value,sort_keys=True)}")
  register("memory_record",memory_id); return memory
 first,second=remember("alpha"),remember("beta")
 linked=http("POST",f"/v1/memories/{first['id']}/link",{"target_id":second["id"],"edge_type":"relates_to"})
 edge=linked.get("edge",linked)
 if (edge.get("source_id"),edge.get("target_id"),edge.get("edge_type")) != (first["id"],second["id"],"relates_to"):
  raise RunnerError("HTTP memory link semantic mismatch")
 archived=http("POST",f"/v1/memories/{first['id']}/archive",{"reason":namespace})
 archived=archived.get("memory",archived)
 if archived.get("id") != first["id"] or archived.get("status") != "archived": raise RunnerError("HTTP memory archive semantic mismatch")
 shown=http("GET",f"/v1/memories/{first['id']}",None)
 shown=shown.get("memory",shown)
 if shown.get("id") != first["id"] or shown.get("status") != "archived": raise RunnerError("HTTP memory terminal oracle failed")
 return {"ids":[first["id"],second["id"]],"terminal":"archived"}

def classified_negative_matrix(http,mcp):
 missing="e2e_missing_opaque_identity"
 missing_upload="upl_00000000000000000000000000000000"
 missing_artifact="art_00000000000000000000000000000000"
 missing_watch="watch_00000000-0000-4000-8000-000000000000"
 results={
  "http_upload_missing":expect_classified("HTTP upload missing",lambda:http("GET",f"/v1/uploads/{missing_upload}",None),("not_found","upload.not_found","404")),
  "http_artifact_missing":expect_classified("HTTP artifact missing",lambda:http("GET",f"/v1/artifacts/{missing_artifact}",None),("not_found","404")),
  "http_watch_missing":expect_classified("HTTP watch missing",lambda:http("GET",f"/v1/watches/{missing_watch}",None),("not_found","404")),
  "http_upload_malformed":expect_classified("HTTP upload malformed",lambda:http("POST","/v1/uploads",{"filename":"x"}),("missing field","400","invalid")),
  "http_watch_malformed":expect_classified("HTTP watch malformed",lambda:http("POST","/v1/watches",{"source":""}),("missing field","400","invalid")),
  "mcp_upload_missing":expect_classified("MCP upload missing",lambda:mcp.call({"action":"uploads","subaction":"get","upload_id":missing_upload,"response_mode":"inline"}),("not found","not_found","invalid_params")),
  "mcp_watch_missing":expect_classified("MCP watch missing",lambda:mcp.call({"action":"watch","subaction":"get","id":missing_watch,"response_mode":"inline"}),("not found","not_found","invalid_params")),
  "mcp_graph_malformed":expect_classified("MCP graph malformed",lambda:mcp.call({"action":"graph","subaction":"node","node_id":"","response_mode":"inline"}),("required","requires","invalid","not found")),
 }
 return results

def launch_runtime(launcher,allocation,timeout):
 completed=subprocess.run([str(Path(launcher).resolve())],input=json.dumps(allocation),capture_output=True,text=True,timeout=timeout)
 if completed.returncode: raise RunnerError(f"per-run launcher failed: {completed.stderr}")
 try: value=json.loads(completed.stdout)
 except json.JSONDecodeError as error: raise RunnerError("per-run launcher response was not JSON") from error
 required=("schema","run_id","status","http_base_url","mcp_selector","qdrant_url","environment","collection_marker","binary","binary_sha256","process_ids","descriptor_path","teardown_handle")
 require_fields(value,required,"per-run launcher")
 if value["schema"] != 1 or value["status"] != "running" or value["run_id"] != allocation["run_id"]: raise RunnerError("launcher descriptor schema/run/status mismatch")
 binary=Path(value["binary"])
 if not binary.is_file() or hashlib.sha256(binary.read_bytes()).hexdigest() != value["binary_sha256"]: raise RunnerError("launcher binary provenance mismatch")
 if not all(isinstance(pid,int) and pid > 1 for pid in value["process_ids"].values()): raise RunnerError("launcher process identities invalid")
 environment=value["environment"]; marker=value["collection_marker"]; collection=allocation["collection"]
 if environment.get("AXON_COLLECTION") != collection or environment.get("AXON_MEMORY_COLLECTION") != collection:
  raise RunnerError("launcher did not bind Axon source+memory collections to run collection")
 if marker.get("collection") != collection or marker.get("run_id") != allocation["run_id"] or marker.get("ownership_generation") != allocation["ownership_generation"] or not isinstance(marker.get("marker_id"),str):
  raise RunnerError("launcher collection marker was not provider-bound to this run")
 if not value["http_base_url"].startswith(("http://127.0.0.1:","http://[::1]:")): raise RunnerError("launcher HTTP endpoint was not isolated loopback")
 command=value["teardown_handle"].get("command")
 if not isinstance(command,list) or not command: raise RunnerError("launcher teardown handle invalid")
 atexit.register(lambda: subprocess.run(command,cwd=ROOT,capture_output=True,text=True))
 return value

def discover_memory_points(base_url,token,timeout,collection,memory_ids):
 qdrant=Http(base_url,token,timeout); deadline=time.monotonic()+30; expected=set(memory_ids)
 while time.monotonic()<deadline:
  found={}; offset=None
  while True:
   body={"limit":256,"with_payload":True,"with_vector":False}
   if offset is not None: body["offset"]=offset
   page=qdrant("POST",f"/collections/{urllib.parse.quote(collection,safe='')}/points/scroll",body).get("result",{})
   for point in page.get("points",[]):
    payload=point.get("payload",{})
    if payload.get("memory_id") in expected: found[str(point.get("id"))]=payload
   offset=page.get("next_page_offset")
   if offset is None: break
  if found: return found
  time.sleep(.25)
 raise RunnerError(f"Qdrant omitted provider-native points for owned memories {sorted(expected)}")

def prove_qdrant_point_ownership(base_url,token,timeout,collection,marker,point_ids,memory_ids):
 if not point_ids: raise RunnerError("memory lifecycle returned no provider-native point IDs")
 qdrant=Http(base_url,token,timeout)
 all_ids=[marker["marker_id"],*point_ids]
 result=qdrant("POST",f"/collections/{urllib.parse.quote(collection,safe='')}/points",{"ids":all_ids,"with_payload":True,"with_vector":False})
 points=result.get("result")
 if not isinstance(points,list): raise RunnerError("Qdrant point retrieval DTO drifted")
 observed={str(point.get("id")):point for point in points if isinstance(point,dict)}
 if set(all_ids) != set(observed): raise RunnerError("Qdrant omitted or added owned marker/memory point identities")
 for point_id,point in observed.items():
  if not isinstance(point.get("payload"),dict): raise RunnerError(f"Qdrant point {point_id} omitted production payload")
 marker_payload=observed[marker["marker_id"]].get("payload",{})
 if marker_payload.get("_axon_e2e_owner") != marker["run_id"] or marker_payload.get("ownership_generation") != marker["ownership_generation"] or marker_payload.get("resource_type") != "collection": raise RunnerError("provider-native collection marker payload mismatch")
 memory_payloads=[observed[point_id]["payload"] for point_id in point_ids]
 if not {payload.get("memory_id") for payload in memory_payloads} <= set(memory_ids): raise RunnerError("memory point payload included a foreign memory identity")
 if any(payload.get("source_generation") is None or payload.get("source_generation") != payload.get("committed_generation") for payload in memory_payloads): raise RunnerError("memory point generation payload was absent or uncommitted")

def parity(cli,mcp,http,source_id,namespace):
 probes={
  "collections":(cli("collections","list","--json"),http("GET","/v1/collections")),
  "providers":(cli("providers","list","--json"),http("GET","/v1/providers")),
  "capabilities":(cli("capabilities","--json"),http("GET","/v1/capabilities")),
 }
 def collection_names(value):
  entries=value.get("collections",[]) if isinstance(value,dict) else []
  return {item if isinstance(item,str) else item.get("name",item.get("collection")) for item in entries if isinstance(item,(str,dict))}
 if collection_names(probes["collections"][0]) != collection_names(probes["collections"][1]): raise RunnerError("collections CLI/HTTP exact identities differ")
 provider_maps=[]
 for value in probes["providers"]:
  provider_maps.append({item.get("id",item.get("provider_id")):item.get("ok") for item in value.get("providers",[]) if isinstance(item,dict)})
 if provider_maps[0] != provider_maps[1] or not provider_maps[0]: raise RunnerError("providers CLI/HTTP semantic inventory differs")
 cli_caps,http_caps=probes["capabilities"]
 for key in ("schema_version","minimum_client_schema_version","version"):
  if cli_caps.get(key) != http_caps.get(key): raise RunnerError(f"capabilities CLI/HTTP {key} differs")
 if set(cli_caps.get("supported_routes",[])) != set(http_caps.get("supported_routes",[])) or not cli_caps.get("supported_routes"): raise RunnerError("capabilities CLI/HTTP route inventory differs")
 return probes

def verify_public_isolation(outputs):
 for current in outputs:
  uploads=list_all_uploads(current["http"]); visible=identity_set(uploads,("upload_id",))
  own=set(current["upload_ids"]); foreign=set().union(*(set(other["upload_ids"]) for other in outputs if other is not current))
  if not own <= visible: raise RunnerError(f"public upload inventory omitted owned IDs: {own-visible}")
 if own & foreign: raise RunnerError("cross-run upload identity collision")
 for current in outputs:
  listed=current["cli"]("memory","list","--json")
  found=identity_set(listed,("memory_id","id")); foreign={other["http_memory"]["ids"][1] for other in outputs if other is not current}
  expected={current["http_memory"]["ids"][1]}
  if not expected <= found: raise RunnerError(f"memory public list omitted owned identities: {expected-found}")
  if found & foreign: raise RunnerError("memory public query returned a foreign-run identity")

def list_all_uploads(http):
 items=[]; cursor=None; seen=set()
 while True:
  path="/v1/uploads?limit=200" + (f"&cursor={urllib.parse.quote(cursor,safe='')}" if cursor else "")
  page=require_fields(http("GET",path,None),("items",),"upload list page")
  if not isinstance(page["items"],list): raise RunnerError("upload list items was not an array")
  items.extend(page["items"]); cursor=page.get("next_cursor")
  if cursor is None: break
  if not isinstance(cursor,str) or cursor in seen: raise RunnerError("upload pagination cursor was invalid/repeated")
  seen.add(cursor)
 return {"items":items,"next_cursor":None}

def execute_residual_handoff(handoff, manifest):
 specification=json.loads(handoff.read_text()); command=specification.get("command")
 if not isinstance(command,list) or command[:2] != [str(ROOT/"scripts/e2e/lib/residual-audit.py"),str(manifest)]:
  raise RunnerError("residual-audit handoff command drifted")
 completed=subprocess.run(command,cwd=ROOT,capture_output=True,text=True)
 if completed.returncode != 2: raise RunnerError(f"pre-teardown residual audit returned {completed.returncode}, expected 2")
 report=json.loads(Path(specification["report"]).read_text())
 created={item["opaque_id"] for item in report.get("created",[])}; residual={item["opaque_id"] for item in report.get("residual",[])}
 if report.get("success") is not False or report.get("fatal") or created != residual:
  raise RunnerError("residual audit did not account for every registered pre-teardown identity")
 return report
def authoritative_teardown(output):
 allocation,runtime,manifest=output["allocation"],output["runtime"],output["manifest"]
 root=Path(allocation["run_root"]); config=root/"teardown-providers.json"
 artifact_root=Path(allocation["data_dir"])/"output/artifacts"
 _,existing_resources=manifest_api.load(Path(allocation["manifest"]));registered_artifacts={item.identity for item in existing_resources if item.resource_type=="artifact"}
 for path in artifact_root.glob("art_*.json"):
  if path.stem not in registered_artifacts:
   manifest.register("artifact",path.stem,{"run_id":allocation["run_id"],"attempt":1,"scenario_id":"stateful.teardown-discovery","request_id":"artifact-store-discovery","origin":"server_response","parent_resource_type":"evidence","parent_identity":f'{allocation["run_id"]}_residual_handoff',"ownership_generation":allocation["ownership_generation"]})
 durable=["job","job_attempt","job_stage","job_event","job_heartbeat","job_artifact","config_snapshot","watch_run","watch","provider_reservation","source","source_generation","source_manifest","source_item","document_status","cleanup_debt","source_lease","graph_node","graph_edge","graph_evidence","graph_alias","graph_conflict","memory_record","memory_link","memory_reinforcement","memory_review","memory_node","memory_edge","observe_event","observe_heartbeat","observe_provider_health"]
 config.write_text(json.dumps({"providers":{"uploads":{"kind":"upload-store","root":str(Path(allocation["data_dir"])/"output/artifacts/uploads"),"resource_types":["upload"]},"artifacts":{"kind":"artifact-store","root":str(Path(allocation["data_dir"])/"output/artifacts"),"resource_types":["artifact"]},"durable":{"kind":"durable-state","resource_types":durable},"relationships":{"kind":"manifest-only","resource_types":["operation","evidence","compose_project","network"]},"qdrant":{"kind":"qdrant","base_url":runtime["qdrant_url"],"tenant_enforced":True,"owned_prefix":"axon_e2e_","resource_types":["collection","point","payload_index","qdrant_alias","qdrant_snapshot"]}}},sort_keys=True)+"\n")
 manifest.register("temp_path",str(config)); header,resources=manifest_api.load(Path(allocation["manifest"])); adapters=provider_api.build(config,header,manifest_api)
 for resource in resources:
  adapter=adapters.get(resource.resource_type)
  if adapter is None: continue
  try:
   if adapter.exists(resource):
    provision=getattr(adapter,"provision_ownership",None) or getattr(adapter,"provision_ownership_marker",None)
    if provision: provision(resource)
  except Exception as error: raise RunnerError(f"ownership provisioning failed for {resource.resource_type}: {error}")
 # Provider-backed application cleanup requires the owned Axon and Qdrant
 # endpoints to remain live. The authoritative engine quiesces/deletes their
 # exact resources first; the launcher process groups are stopped afterward.
 report=root/"authoritative-teardown.json"; cleaned=subprocess.run([sys.executable,str(ROOT/"scripts/e2e/lib/teardown.py"),allocation["manifest"],"--report",str(report),"--provider-config",str(config),"--global-timeout","600"],cwd=ROOT,capture_output=True,text=True,timeout=620)
 value=json.loads(report.read_text()); created={x["opaque_id"] for x in value.get("created",[])}; removed={x["opaque_id"] for x in value.get("removed",[])}
 stopped=subprocess.run(runtime["teardown_handle"]["command"],cwd=ROOT,capture_output=True,text=True)
 if stopped.returncode: raise RunnerError(f"owned stack stop failed: {stopped.stderr}")
 if cleaned.returncode or value.get("success") is not True or value.get("residual") or value.get("refused") or created != removed: raise RunnerError(f"authoritative teardown failed exact accounting: {value}")
 return value
def main():
 parser=argparse.ArgumentParser(); parser.add_argument("--axon-bin",required=True,type=Path); parser.add_argument("--mcporter",required=True,type=Path)
 parser.add_argument("--launcher",required=True,type=Path); parser.add_argument("--http-token")
 parser.add_argument("--qdrant-token")
 parser.add_argument("--fixture-source",required=True); parser.add_argument("--fixture-source-id",required=True); parser.add_argument("--work-root",type=Path,default=Path(tempfile.gettempdir())/"axon-e2e-stateful"); parser.add_argument("--timeout",type=int,default=60); args=parser.parse_args()
 if not args.axon_bin.is_file() or not args.mcporter.is_file() or not args.launcher.is_file(): raise RunnerError("required executable missing")
 allocations=[isolation.allocate(args.work_root/"runs",args.work_root/"manifests") for _ in range(2)]
 for allocation in allocations: allocation.update(seed_stateful=True,fixture_source=args.fixture_source)
 outputs=[]
 for allocation in allocations:
  if not allocation.get("collection") or not allocation.get("ownership_generation"): raise RunnerError("allocation omitted owned vector binding")
  manifest=isolation.Manifest.open(Path(allocation["manifest"])); reg=registerer(manifest,allocation["run_id"],allocation["collection"],allocation["ownership_generation"])
  runtime=launch_runtime(args.launcher,allocation,args.timeout); fixture_source=runtime.get("fixture_source"); fixture_source_id=runtime.get("fixture_source_id")
  if fixture_source != args.fixture_source or not isinstance(fixture_source_id,str): raise RunnerError("launcher stateful fixture identity mismatch")
  runtime_env={**os.environ,**runtime["environment"],"AXON_DATA_DIR":allocation["data_dir"]}
  http_token=args.http_token or runtime["environment"].get("AXON_HTTP_TOKEN")
  if not isinstance(http_token,str) or not http_token: raise RunnerError("launcher omitted authenticated REST token")
  cli=Cli(args.axon_bin,runtime_env,args.timeout); http=Http(runtime["http_base_url"],http_token,args.timeout); mcp=Mcp(args.mcporter,runtime["mcp_selector"],args.timeout,runtime_env)
  doctor=require_fields(cli("doctor","--json"),("all_ok",),"doctor")
  if doctor["all_ok"] is not True:
   failed={name:value for name,value in doctor.get("services",{}).items() if value.get("ok") is not True}
   raise RunnerError(f"CLI provider preflight failed: {json.dumps(failed,sort_keys=True)}")
  require_fields(http("GET","/v1/status"),("build_identity","payload","text","totals","degraded","warnings"),"HTTP status"); require_fields(mcp.call({"action":"capabilities","response_mode":"inline"}),("server","contract_version","actions","providers"),"MCP capabilities")
  fixture_graph=runtime.get("fixture_graph")
  if not isinstance(fixture_graph,dict) or fixture_graph.get("source_id") != fixture_source_id: raise RunnerError("launcher omitted allocation-derived graph identity contract")
  graph_result=graph.run(cli,fixture_source_id,fixture_source,fixture_graph,reg)
  graph.negative(cli); memory_client=MemoryDispatch(cli,mcp); memory_result=memory.run(memory_client,allocation["namespace"],reg,allocation["run_root"]); memory.negatives(memory_client,allocation["namespace"])
  watch_source=runtime["http_base_url"]+"/v1/status"; watch_result=watches.run(cli,watch_source,allocation["namespace"],reg)
  resource_result=resources.upload_artifact_lifecycle(http,allocation["namespace"],reg)
  race_result=resources.upload_complete_abort_race(http,allocation["namespace"],reg); inventory=resources.read_only_inventory(cli)
  for artifact_id in identity_set(inventory.get("artifacts",{}),("artifact_id",)): reg("artifact",artifact_id)
  mcp_mutations=mcp_mutation_lifecycles(mcp,watch_source,allocation["namespace"],reg)
  http_watch=http_watch_lifecycle(http,watch_source,allocation["namespace"],reg)
  http_memory=http_memory_lifecycle(http,allocation["namespace"],reg)
  mcp_memory=mcp.call({"action":"memory","subaction":"list","response_mode":"inline"})
  if http_memory["ids"][1] not in identity_set(mcp_memory,("memory_id","id")): raise RunnerError("MCP memory list omitted active HTTP-owned identity")
  negatives=classified_negative_matrix(http,mcp)
  owned_memory_ids=[*memory_result["ids"],memory_result["compact_id"],memory_result["imported_id"],*http_memory["ids"]]
  provider_points=discover_memory_points(runtime["qdrant_url"],args.qdrant_token,args.timeout,allocation["collection"],owned_memory_ids)
  memory_result["point_ids"]=sorted(provider_points)
  for point_id in memory_result["point_ids"]: reg("point",point_id)
  prove_qdrant_point_ownership(runtime["qdrant_url"],args.qdrant_token,args.timeout,allocation["collection"],runtime["collection_marker"],memory_result["point_ids"],owned_memory_ids)
  growth_ids=resources.register_growth(http,allocation["namespace"],reg,enumerate_resources=lambda: list_all_uploads(http))
  parity(cli,mcp,http,fixture_source_id,allocation["namespace"])
  handoff=Path(allocation["run_root"])/"residual-audit-handoff.json"; handoff.write_text(json.dumps({"manifest":allocation["manifest"],"report":str(Path(allocation["run_root"])/"residual-audit.json"),"command":[str(ROOT/"scripts/e2e/lib/residual-audit.py"),allocation["manifest"],"--report",str(Path(allocation["run_root"])/"residual-audit.json")]},sort_keys=True)+"\n")
  manifest.register("evidence",f'{allocation["run_id"]}_residual_handoff',{"run_id":allocation["run_id"],"path":str(handoff)})
  race_upload_id=race_result.get("upload_id") or race_result.get("id")
  if not isinstance(race_upload_id,str): raise RunnerError("upload race terminal omitted upload identity")
  upload_ids=[resource_result["upload_id"],race_upload_id,mcp_mutations["upload_id"],*growth_ids]
  outputs.append({"allocation":allocation,"runtime":runtime,"cli":cli,"http":http,"graph":graph_result,"memory":memory_result,"http_memory":http_memory,"watch":watch_result,"resource":resource_result,"inventory":inventory,"upload_ids":upload_ids,"handoff":handoff,"manifest":manifest})
 registry_evidence=validate_transport_catalog(); verify_public_isolation(outputs)
 for value in outputs:
  execute_residual_handoff(value["handoff"],value["allocation"]["manifest"]); authoritative_teardown(value)
 print(json.dumps({"status":"passed","runs":[value["allocation"]["run_id"] for value in outputs],"manifests":[value["allocation"]["manifest"] for value in outputs],"transport_catalog":TRANSPORT_CATALOG,"registry_evidence":registry_evidence})); return 0
if __name__=="__main__":
 try: raise SystemExit(main())
 except (RunnerError,isolation.IsolationError,subprocess.TimeoutExpired) as error: print(f"stateful E2E failed: {error}",file=sys.stderr); raise SystemExit(2)
