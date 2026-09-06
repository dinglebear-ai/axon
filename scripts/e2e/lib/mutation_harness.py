#!/usr/bin/env python3
"""Drive repository-owned E2E oracles with isolated fixture mutations."""
import copy,importlib.util,json,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
class OracleFailure(AssertionError):pass
def load(name,path):
 spec=importlib.util.spec_from_file_location(name,path);module=importlib.util.module_from_spec(spec);sys.modules[name]=module;spec.loader.exec_module(module);return module
wire=load("axon_mutation_task_wire",ROOT/"scripts/test-mcp-tasks-wire.py")
source=load("axon_mutation_source_oracle",ROOT/"tests/e2e/scenarios/source/orchestrator.py")
retrieval=load("axon_mutation_retrieval_oracle",ROOT/"tests/e2e/scenarios/retrieval/execute.py")
security=load("axon_mutation_security_oracle",ROOT/"tests/e2e/scenarios/security/security_pack.py")
parity=load("axon_mutation_parity_oracle",ROOT/"scripts/e2e/reconcile-surfaces.py")
manifest=load("axon_mutation_manifest_oracle",ROOT/"scripts/e2e/lib/resource-manifest.py")
class FakeWire:
 def __init__(self,responses):self.responses=iter(copy.deepcopy(responses))
 def request(self,payload,timeout=30):response,notices=next(self.responses);response["id"]=payload["id"];return response,notices
def fixture(item):
 value=json.loads((ROOT/item["fixture"]).read_text())
 def expand(x):
  if isinstance(x,str):return x.replace("$CORPUS",str((ROOT/"tests/e2e/corpus/v1/documents").resolve()))
  if isinstance(x,list):return [expand(v) for v in x]
  if isinstance(x,dict):return {k:expand(v) for k,v in x.items()}
  return x
 return expand(value)
def mutate(name,value):
 result=copy.deepcopy(value)
 if name=="remove_initial_progress":result["create"][0][1].clear()
 elif name=="remove_vectors":result["snapshot"]["points"]=[]
 elif name=="invalid_transition":result["states"]=["queued","completed","running"]
 elif name=="wrong_envelope":result["execution"]["envelope"]["assertions"][0]["id"]="cli.exit"
 elif name=="duplicate_publication":result["snapshot"]["points"].append(copy.deepcopy(result["snapshot"]["points"][0]))
 elif name=="remove_citations":result["actual"]={"answer":"green"}
 elif name=="leak_canary":result["evidence"]=result["secret"]
 elif name=="remove_cleanup":result["resources"]=[]
 elif name=="wrong_peer":result["provider"]["observed_peer"]="unowned-peer"
 elif name=="stale_ownership":result["resource"]["metadata"]["ownership_generation"]=3
 elif name=="swallow_failure":result["failure"].update(terminal_state="completed",error_code=None)
 elif name!="ineffective":raise ValueError(f"unknown fixed mutation: {name}")
 return result
def require(value,message):
 if not value:raise OracleFailure(message)
def oracle(name,value):
 if name=="mcp.create.initial_progress":wire.create(FakeWire(value["create"]),1,value["url"],value["prompt"],value["token"])
 elif name=="source.qdrant.snapshot":
  class Client:
   def request(self,*_args):return {"result":copy.deepcopy(value["snapshot"])}
  observed=source.QdrantEvidenceClient.snapshot(Client(),"fixture",value["source_id"]);require(observed["count"] and observed["generations"]==[str(value["generation"])],"published vector generation differs")
 elif name=="mcp.poll.transitions":
  allowed={("queued","running"),("running","completed"),("running","failed"),("running","canceled")};require(all(x in allowed for x in zip(value["states"],value["states"][1:])),"invalid lifecycle transition")
 elif name=="parity.reconcile":
  evidence=ROOT/"tests/e2e/mutations/fixtures/parity-evidence.json";import hashlib;execution=copy.deepcopy(value["execution"]);execution["evidence_sha256"]=hashlib.sha256(evidence.read_bytes()).hexdigest();errors=[];parity.reconcile_parity({"executions":[execution]},ROOT/"tests/e2e/mutations/fixtures/parity-bundle.json",errors);require(not errors,f"parity errors: {errors}")
 elif name=="retrieval.runtime_semantics":
  scenarios={x["id"]:x for x in retrieval.scenarios()};retrieval.runtime_semantics(scenarios[value["scenario_id"]],value["actual"])
 elif name=="security.scan_artifact":require(not security.scan_artifact(value["evidence"].encode(),[value["secret"]]),"canary leaked")
 elif name=="manifest.registration":
  require(bool(value["resources"]),"cleanup registration absent")
  for resource in value["resources"]:_ownership(value["header"],resource)
 elif name=="provider.peer":require(value["provider"]["observed_peer"]==value["provider"]["expected_peer"],"provider peer differs")
 elif name=="manifest.ownership":
  require(value["resource"]["metadata"]["ownership_generation"]==value["header"]["ownership_generation"],"stale ownership generation");_ownership(value["header"],value["resource"])
 elif name=="source.provider_failure":require(value["failure"]["provider_failed"] and value["failure"]["terminal_state"]=="failed" and value["failure"]["error_code"],"provider failure swallowed")
 else:raise ValueError(f"unknown oracle: {name}")
def _ownership(header,resource):
 run=manifest.RunHeader(header["run_id"],Path(header["data_dir"]),header["created_unix_ms"],header["digest"],Path(header["manifest_path"]));item=manifest.Resource(resource["resource_type"],resource["identity"],resource["metadata"],resource["sequence"],resource["checkpoint_digest"]);return manifest.qdrant_ownership_point(run,item)
def load_mutants(path):return json.loads(path.read_text())["mutants"]
def executable_ids():
 ids=set()
 for path in (ROOT/"tests/e2e").rglob("scenarios.json"):ids.update(x["id"] for x in json.loads(path.read_text()).get("scenarios",[]))
 ids.update(x["id"] for x in json.loads((ROOT/"tests/e2e/catalog/catalog.json").read_text())["scenarios"])
 return ids
def validate_registry(mutants):
 known=executable_ids();required={"id","codepath","scenario","invariant","mutation","fixture","oracle"}
 for item in mutants:
  if missing:=required-item.keys():raise ValueError(f"{item.get('id')} missing {sorted(missing)}")
  if item["scenario"] not in known:raise ValueError(f"{item['id']} maps to non-executable scenario {item['scenario']}")
  if not (ROOT/item["codepath"]).is_file():raise ValueError(f"{item['id']} codepath does not exist")
  if not (ROOT/item["fixture"]).is_file():raise ValueError(f"{item['id']} fixture does not exist")
