"""Memory lifecycle and import-contract E2E pack."""
import json
from pathlib import Path

class MemoryContractError(RuntimeError): pass

def _obj(value, op):
    if not isinstance(value, dict): raise MemoryContractError(f"{op} did not return an object")
    for key in ("memory","edge","result"):
        if isinstance(value.get(key),dict): return value[key]
    return value

def _id(value, op):
    value=_obj(value,op); identity=value.get("memory_id",value.get("id"))
    if not isinstance(identity,str) or not identity: raise MemoryContractError(f"{op} omitted memory_id/id: {value}")
    return identity

def _item(value,op,identity=None,status=None):
    value=_obj(value,op); actual=_id(value,op)
    if identity is not None and actual!=identity: raise MemoryContractError(f"{op} returned wrong identity")
    for key,kind in (("memory_type",str),("status",str),("confidence",(int,float))):
        if not isinstance(value.get(key),kind) or isinstance(value.get(key),bool): raise MemoryContractError(f"{op} omitted typed {key}")
    if status is not None and value["status"]!=status: raise MemoryContractError(f"{op} expected {status}, got {value['status']}")
    return value

def _edge(value,op,source,target,edge_type):
    value=_obj(value,op)
    if (value.get("source_id"),value.get("target_id"),value.get("edge_type"))!=(source,target,edge_type):
        raise MemoryContractError(f"{op} returned wrong relationship")
    return value

def _walk(value):
    if isinstance(value,dict):
        yield value
        for child in value.values(): yield from _walk(child)
    elif isinstance(value,list):
        for child in value: yield from _walk(child)

def _register(value,register):
    seen=set()
    for item in _walk(value):
        pairs=[("memory_record",item.get("memory_id",item.get("id"))),("graph_node",item.get("graph_node_id"))]
        pairs += [("point",x) for x in (item.get("vector_point_ids",item.get("embedding_refs",[])) or [])]
        pairs += [("memory_record",x) for x in (item.get("created_ids",[]) or [])]
        for pair in pairs:
            if isinstance(pair[1],str) and pair not in seen: register(*pair); seen.add(pair)

def _search(value):
    if isinstance(value,list): return value
    if isinstance(value,dict) and isinstance(value.get("memories"),list): return value["memories"]
    if isinstance(value,dict) and isinstance(value.get("results"),list):
        return [x.get("record",x) for x in value["results"]]
    raise MemoryContractError(f"search did not return results: {value}")

def run(cli,namespace,register,work_dir=None):
    created=[]
    for suffix in ("alpha","beta","gamma","delta"):
        value=_item(cli("memory","remember",f"{namespace} {suffix}","--json"),"remember",status="active")
        _register(value,register); created.append(value)
    ids=[_id(x,"remember") for x in created]
    if len(set(ids))!=4: raise MemoryContractError("duplicate remember IDs")
    evidence=[]
    evidence.append(_edge(cli("memory","link",ids[0],ids[1],"--json"),"link",ids[0],ids[1],"relates_to"))
    evidence.append(_edge(cli("memory","supersede",ids[1],ids[0],"--json"),"supersede",ids[1],ids[0],"supersedes"))
    _item(cli("memory","show",ids[0],"--json"),"show superseded",ids[0],"superseded")
    before=_item(cli("memory","show",ids[1],"--json"),"before reinforce",ids[1])
    reinforced=_item(cli("memory","reinforce",ids[1],"--amount","0.25","--reason",namespace,"--json"),"reinforce",ids[1])
    if reinforced.get("access_count",0)<=before.get("access_count",0): raise MemoryContractError("reinforce did not increment access_count")
    evidence.append(reinforced)
    evidence.append(_item(cli("memory","pin",ids[1],"--reason",namespace,"--json"),"pin",ids[1],"active"))
    evidence.append(_edge(cli("memory","contradict",ids[2],ids[1],"--reason",namespace,"--json"),"contradict",ids[2],ids[1],"contradicts"))
    _item(cli("memory","show",ids[2],"--json"),"show contradicted",ids[2],"contradicted")
    evidence.append(_item(cli("memory","archive",ids[3],"--reason",namespace,"--json"),"archive",ids[3],"archived"))
    export_path=Path(work_dir or ".")/f"{namespace}-memory-export.json"
    exported=cli("memory","export","--output",str(export_path),"--json")
    try: records=json.loads(export_path.read_text())
    except Exception as error: raise MemoryContractError("export was not valid JSON") from error
    if not isinstance(records,list) or not records: raise MemoryContractError("export lacked MemoryRecords")
    for record in records: _register(record,register)
    seed=next((x for x in records if x.get("memory_id")==ids[1]),records[0]); clone=dict(seed)
    requested_import_id=f"{namespace}_imported"; clone.update(memory_id=requested_import_id,body=f"{namespace} imported unique body")
    import_path=Path(work_dir or ".")/f"{namespace}-memory-import.json"; import_path.write_text(json.dumps([clone]))
    imported=_obj(cli("memory","import",str(import_path),"--mode","merge","--json"),"import")
    created_ids=imported.get("created_ids"); imported_id=created_ids[0] if isinstance(created_ids,list) and len(created_ids)==1 else None
    if (imported.get("created"),imported.get("updated"),imported.get("dry_run"))!=(1,0,False) or not isinstance(imported_id,str):
        raise MemoryContractError(f"first import violated MemoryImportResult: {imported}")
    _register(imported,register)
    repeated=_obj(cli("memory","import",str(import_path),"--mode","merge","--json"),"repeat import")
    if repeated.get("created")!=0 or repeated.get("updated")!=0 or repeated.get("skipped",0)<1 or repeated.get("created_ids",[])!=[]:
        raise MemoryContractError("merge import was not idempotent")
    searched=_search(cli("memory","search",namespace,"--limit","50","--json")); search_ids={_id(x,"search") for x in searched}
    owned=set(ids)|{imported_id}
    if not {ids[1],imported_id}.issubset(search_ids) or not search_ids.issubset(owned): raise MemoryContractError("search identity isolation failed")
    if any(namespace not in str(x) for x in searched): raise MemoryContractError("search returned foreign content")
    reviewed=cli("memory","review","--limit","50","--json")
    if not isinstance(reviewed,dict) or not isinstance(reviewed.get("memories"),list) or not isinstance(reviewed.get("warnings",[]),list): raise MemoryContractError(f"review DTO invalid: {reviewed}")
    compacted=_item(cli("memory","compact",ids[1],imported_id,"--archive-sources","--json"),"compact",status="active")
    compact_id=_id(compacted,"compact"); _register(compacted,register); evidence.append(compacted)
    if compact_id in owned: raise MemoryContractError("compact reused source identity")
    _item(cli("memory","show",ids[1],"--json"),"archived source",ids[1],"archived")
    _item(cli("memory","show",imported_id,"--json"),"archived imported",imported_id,"archived")
    forgotten=_item(cli("memory","forget",compact_id,"--reason",namespace,"--json"),"forget",compact_id,"forgotten")
    shown=_item(cli("memory","show",compact_id,"--json"),"show forgotten",compact_id,"forgotten")
    if forgotten.get("status") != "forgotten" or shown.get("status") != "forgotten": raise MemoryContractError("forget terminal status did not persist")
    evidence.append(forgotten)
    point_ids={point for item in [*created,*records,compacted] for point in (item.get("vector_point_ids",item.get("embedding_refs",[])) or []) if isinstance(point,str)}
    return {"ids":ids,"search_ids":sorted(search_ids),"point_ids":sorted(point_ids),"compact_id":compact_id,"imported_id":imported_id,"evidence":evidence,"exported":exported,"imported":imported}

def negatives(cli,namespace):
    cases={"missing":cli("memory","show",f"{namespace}_missing","--json"),"malformed":cli("memory","remember","","--json",ok=False),"conflict":cli("memory","link",f"{namespace}_missing_a",f"{namespace}_missing_b","--json",ok=False),"self_supersede":cli("memory","supersede",f"{namespace}_same",f"{namespace}_same","--json",ok=False)}
    for name,value in cases.items():
        semantic_missing=name=="missing" and isinstance(value,dict) and (("memory" in value and value["memory"] is None) or ("result" in value and value["result"] is None))
        if not semantic_missing and (not isinstance(value,dict) or not (value.get("code") or value.get("error"))): raise MemoryContractError(f"{name} lacked classification")
    return cases
