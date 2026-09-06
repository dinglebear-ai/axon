"""Upload, artifact, collection, and read-only inventory E2E assertions."""
from __future__ import annotations

import concurrent.futures
import hashlib
import time
from typing import Any, Callable


class ResourceContractError(RuntimeError): pass


def upload_artifact_lifecycle(http: Callable[[str, str, dict[str, Any] | None], dict[str, Any]],
                              namespace: str, register: Callable[[str, str], None]) -> dict[str, Any]:
    content=f"# {namespace}".encode()
    digest=hashlib.sha256(content).hexdigest()
    created = http("POST", "/v1/uploads", {"filename": f"{namespace}.md", "content_type": "text/markdown", "size_bytes":len(content), "purpose":"source_artifact", "sha256":digest})
    upload_id = created.get("upload_id")
    if not isinstance(upload_id, str): raise ResourceContractError("upload create omitted upload_id")
    register("upload", upload_id)
    http("PUT", f"/v1/uploads/{upload_id}/content", content)
    completed = http("POST", f"/v1/uploads/{upload_id}/complete", {})
    artifact_id = completed.get("artifact_id")
    if not isinstance(artifact_id, str): raise ResourceContractError("upload complete omitted artifact_id")
    register("artifact", artifact_id)
    metadata = http("GET", f"/v1/artifacts/{artifact_id}", None)
    upload_status = http("GET", f"/v1/uploads/{upload_id}", None)
    returned_content = http("GET", f"/v1/artifacts/{artifact_id}/content", None)
    if metadata.get("artifact_id") != artifact_id or returned_content != content:
        raise ResourceContractError("artifact metadata/content lost exact upload identity")
    if metadata.get("size_bytes") != len(content) or metadata.get("content_type") != "text/markdown":
        raise ResourceContractError("artifact size/content type did not round-trip")
    if upload_status.get("sha256") != digest or upload_status.get("artifact_id") != artifact_id:
        raise ResourceContractError("upload hash/artifact provenance did not round-trip")
    if completed.get("upload_id") != upload_id or not isinstance(completed.get("source_ref"),str):
        raise ResourceContractError("artifact completion lost upload/source provenance")
    repeated = http("POST", f"/v1/uploads/{upload_id}/complete", {})
    if repeated.get("artifact_id") != artifact_id:
        raise ResourceContractError("repeated completion duplicated artifact identity")
    return {"upload_id": upload_id, "artifact_id": artifact_id, "sha256":digest, "source_ref":completed["source_ref"]}


def upload_complete_abort_race(http: Callable[[str, str, dict[str, Any] | None], dict[str, Any]],
                               namespace: str, register: Callable[[str, str], None]) -> dict[str, Any]:
    content=f"race {namespace}".encode()
    created = http("POST", "/v1/uploads", {"filename": f"{namespace}-race.md", "content_type":"text/plain", "size_bytes":len(content), "purpose":"source_artifact"})
    upload_id = created["upload_id"]; register("upload", upload_id)
    http("PUT",f"/v1/uploads/{upload_id}/content",content)
    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
        futures = [pool.submit(http, "POST", f"/v1/uploads/{upload_id}/complete", {}),
                   pool.submit(http, "DELETE", f"/v1/uploads/{upload_id}", None)]
        outcomes = []
        for future in futures:
            try: outcomes.append(future.result())
            except RuntimeError as error: outcomes.append({"error": str(error)})
    terminal = http("GET", f"/v1/uploads/{upload_id}", None)
    if terminal.get("status") not in {"completed", "aborted"}:
        raise ResourceContractError(f"upload race had illegal terminal state: {terminal}")
    artifact_ids = {value.get("artifact_id") for value in outcomes if isinstance(value.get("artifact_id"), str)}
    if len(artifact_ids) > 1: raise ResourceContractError("upload race produced multiple artifacts")
    for identity in artifact_ids: register("artifact", identity)
    return terminal


def read_only_inventory(cli: Callable[..., dict[str, Any]]) -> dict[str, Any]:
    values = {"collections": cli("collections", "list", "--json"),
              "capabilities": cli("capabilities", "--json"),
              "providers": cli("providers", "list", "--json"),
              "status": cli("status", "--json"), "stats": cli("stats", "--json"),
              "config": cli("config", "list", "--json")}
    if any(not isinstance(value, dict) or value.get("error") for value in values.values()):
        raise ResourceContractError("read-only inventory returned an error/non-object")
    required={"collections":("collections",),"capabilities":("schema_version","minimum_client_schema_version","supported_routes","build","version"),
              "providers":("providers",),"status":("build_identity","cleanup","jobs","sqlite","totals","degraded","warnings","watches"),
              "stats":("collection","status","points_count","payload_fields","freshness","counts")}
    for operation,fields in required.items():
        missing=[field for field in fields if field not in values[operation]]
        if missing: raise ResourceContractError(f"{operation} inventory omitted {missing}")
    if not isinstance(values["collections"]["collections"],list) or not isinstance(values["providers"]["providers"],list):
        raise ResourceContractError("collection/provider inventory list DTO drifted")
    capabilities=values["capabilities"]
    if capabilities["schema_version"] != capabilities["minimum_client_schema_version"] or not isinstance(capabilities["supported_routes"],list) or not capabilities["supported_routes"] or not isinstance(capabilities["build"],dict):
        raise ResourceContractError("capability inventory semantic DTO drifted")
    for provider in values["providers"]["providers"]:
        if not isinstance(provider,dict) or not {"id","ok","detail"} <= provider.keys(): raise ResourceContractError("provider summary DTO drifted")
    if not isinstance(values["config"],dict) or not values["config"]: raise ResourceContractError("config inventory was empty")
    return values


def register_growth(http: Callable[[str, str, dict[str, Any] | None], dict[str, Any]], namespace: str,
                    register: Callable[[str, str], None], count: int = 257,
                    enumerate_resources: Callable[[], Any] | None = None) -> list[str]:
    def create(index):
        for attempt in range(6):
            try:
                value = http("POST", "/v1/uploads", {"filename": f"{namespace}-{index}.txt", "content_type":"text/plain", "size_bytes":0, "purpose":"source_artifact"})
                break
            except RuntimeError as error:
                if "upload.busy" not in str(error) or attempt == 5: raise
                time.sleep(.02*(attempt+1))
        identity = value.get("upload_id")
        if not isinstance(identity, str): raise ResourceContractError("growth create omitted upload_id")
        register("upload", identity); return identity
    with concurrent.futures.ThreadPoolExecutor(max_workers=16) as pool:
        futures=[pool.submit(create,index) for index in range(count)]
        enumerations=[]
        while any(not future.done() for future in futures):
            if enumerate_resources is not None: enumerations.append(enumerate_resources())
        identities=[future.result() for future in futures]
    if len(identities) != len(set(identities)): raise ResourceContractError("growth duplicated upload IDs")
    if enumerate_resources is not None and not enumerations:
        raise ResourceContractError("growth finished before teardown enumeration raced it")
    if enumerate_resources is not None:
        final=enumerate_resources()
        visible=set()
        def collect(value):
            if isinstance(value,dict):
                for key,item in value.items():
                    if key=="upload_id" and isinstance(item,str): visible.add(item)
                    collect(item)
            elif isinstance(value,list):
                for item in value: collect(item)
        collect(final)
        if not set(identities) <= visible:
            raise ResourceContractError("post-growth enumeration omitted exact created upload IDs")
    return identities
