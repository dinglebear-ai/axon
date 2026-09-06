"""Graph lifecycle pack using exact public graph identities."""
class GraphContractError(RuntimeError): pass

def run(cli, source_id, canonical_uri, expected, register=lambda _kind, _identity: None):
    kinds=cli("graph","kinds","--json")
    resolved=cli("graph","resolve",canonical_uri,"--json")
    queried=cli("graph","query",canonical_uri,"--limit","50","--json")
    source=cli("graph","source",source_id,"--json")
    encoded=str({"resolved":resolved,"queried":queried,"source":source})
    if source_id not in encoded or canonical_uri not in encoded: raise GraphContractError("graph lost fixture identity")
    nodes=queried.get("nodes",[]); edges=queried.get("edges",[])
    if not nodes or not edges or not kinds: raise GraphContractError("graph omitted kinds/nodes/relationship edge")
    node_ids={str(node["node_id"]) for node in nodes}; edge_ids={str(edge["edge_id"]) for edge in edges}
    if node_ids != set(expected["node_ids"]) or edge_ids != {expected["edge_id"]}: raise GraphContractError("graph identities did not match allocation-derived fixture")
    for node_id in node_ids:
        register("graph_node",node_id); detail=cli("graph","node",node_id,"--json")
        if detail.get("node",{}).get("node_id") != node_id or not isinstance(detail.get("edges"),list): raise GraphContractError("graph node detail DTO/identity drifted")
    for edge in edges:
        edge_id=str(edge["edge_id"]); register("graph_edge",edge_id); detail=cli("graph","edge",edge_id,"--json")
        if detail.get("edge_id") != edge_id: raise GraphContractError("graph edge detail identity drifted")
        endpoints={str(edge.get("from_node_id") or edge.get("source_node_id")),str(edge.get("to_node_id") or edge.get("target_node_id"))}
        if not endpoints <= node_ids: raise GraphContractError("graph edge referenced an unreturned node")
        attached=edge.get("evidence")
        if not isinstance(attached,list) or not attached: raise GraphContractError("graph edge omitted attached evidence")
        for evidence in attached:
            if evidence.get("source_id") != source_id or not isinstance(evidence.get("evidence_id"),str):
                raise GraphContractError("edge evidence lost source provenance")
        if edge.get("metadata",{}).get("conflict_ids") != [expected["conflict_id"]]: raise GraphContractError("edge omitted exact lifecycle conflict identity")
        register("graph_conflict",expected["conflict_id"])
    for kind,key in (("graph_evidence","evidence"),):
        items=queried.get(key,[])
        if not items: raise GraphContractError(f"fixture omitted mandatory {key}")
        for item in items:
            identity=item.get("evidence_id") if isinstance(item,dict) else None
            if not isinstance(identity,str): raise GraphContractError(f"{key} omitted exact ID")
            if item.get("source_id") != source_id: raise GraphContractError("graph evidence lost exact source linkage")
            if identity != expected["evidence_id"]: raise GraphContractError("graph evidence identity was not allocation-derived")
            register(kind,identity)
    if not isinstance(resolved.get("resolved"),list) or not resolved["resolved"]: raise GraphContractError("graph resolve omitted canonical alias match")
    if resolved["resolved"][0].get("node",{}).get("node_id") not in node_ids: raise GraphContractError("graph resolve returned a foreign node")
    return {"kinds":kinds,"resolved":resolved,"nodes":nodes,"edges":edges,"source":source}

def negative(cli, missing="e2e_missing_graph_identity"):
    values=[cli("graph","node",missing,"--json",ok=False),cli("graph","edge",missing,"--json",ok=False)]
    if any(not value.get("code") and not value.get("error") for value in values): raise GraphContractError("graph missing-ID lacked classification")
    return values
