"""Three-projection contract checks using the existing real MCP wire transports.

This extends the catalog adapter; no mocked MCP server or replacement transport
is used. Only disposable upload/job state is mutated. Provider-backed sweeps and
full task lifecycle checks remain in their existing harnesses.
"""
from __future__ import annotations
import base64
import importlib.util
import itertools
import json
import os
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
SPEC = importlib.util.spec_from_file_location("task_wire", ROOT / "scripts/test-mcp-tasks-wire.py")
wire = importlib.util.module_from_spec(SPEC)
assert SPEC.loader
SPEC.loader.exec_module(wire)


def envelope(message):
    response = wire.result(message, "tools/call")
    if isinstance(response.get("structuredContent"), dict):
        return response["structuredContent"]
    for block in response.get("content", []):
        if block.get("type") == "text":
            try:
                value = json.loads(block["text"])
            except (ValueError, KeyError):
                continue
            if isinstance(value, dict):
                return value
    raise wire.WireError("tool response omitted a structured Axon envelope")


def data(value):
    payload = value.get("data", {})
    return payload.get("inline", payload.get("data", payload))


class Checks:
    def __init__(self, client):
        self.client, self.ids, self.results = client, itertools.count(100), []

    def rpc(self, method, params=None):
        message, _ = self.client.request(wire.rpc(next(self.ids), method, params or {}))
        return message

    def call(self, arguments, *, error=False, meta=None, name=None):
        params = wire.call_params(self.client, arguments, meta) if name is None else {"name": name, "arguments": arguments}
        if meta is not None: params["_meta"] = meta
        response = self.rpc("tools/call", params)
        if error:
            if "error" in response:
                detail = wire.structured_error(response, params["name"])
                self.results.append({"tool": params["name"], "outcome": "expected_denial", "code": detail["code"]})
                return detail
            value = envelope(response)
            if value.get("ok") is not False:
                raise wire.WireError(f"expected denial from {params['name']}")
            self.results.append({"tool": params["name"], "outcome": "expected_denial"})
            return value
        value = envelope(response)
        if value.get("ok") is not True:
            raise wire.WireError(f"business operation failed: {params['name']}: {wire.sanitize(value)}")
        self.results.append({"tool": params["name"], "outcome": "success"})
        return value


def check_discovery(checks, projection):
    tools = wire.result(checks.rpc("tools/list"), "tools/list")["tools"]
    help_ = data(checks.call({"action": "help", "response_mode": "inline"}))
    if help_.get("projection") != projection:
        raise wire.WireError("observed server projection differs from requested projection")
    inventory = wire.adapter.assert_inventory(tools, help_["operations"], projection)
    resource = wire.result(checks.rpc("resources/read", {"uri": "axon://schema/mcp-operations"}), "operation resource")
    catalog = json.loads(resource["contents"][0]["text"])
    if catalog["projection"] != projection or catalog["active_tools"] != inventory["tools"]:
        raise wire.WireError("operation resource and actual router disagree")
    wire.result(checks.rpc("resources/read", {"uri": "axon://schema/mcp-tool"}), "legacy schema resource")
    dashboard = next(tool for tool in tools if tool["name"] == "axon_status_dashboard")
    if not dashboard.get("_meta", {}).get("ui", {}).get("resourceUri"):
        raise wire.WireError("dashboard UI metadata missing")
    return inventory, dashboard


def check_upload_lifecycle(checks):
    content = b"atomic leaf\n"
    created = data(checks.call({"action": "uploads", "subaction": "create", "filename": "axon-e2e-atomic.txt",
                               "content_type": "text/plain", "size_bytes": len(content), "purpose": "source_artifact", "response_mode": "inline"}))
    upload_id = created.get("upload_id", created.get("id"))
    if not isinstance(upload_id, str): raise wire.WireError("owned upload id missing")
    checks.call({"action":"uploads", "subaction":"put_content", "upload_id":upload_id,
                 "content":base64.b64encode(content).decode(), "response_mode":"inline"})
    checks.call({"action":"uploads", "subaction":"complete", "upload_id":upload_id, "response_mode":"inline"})
    checks.call({"action":"uploads", "subaction":"get", "upload_id":upload_id, "response_mode":"inline"})
    disposable = data(checks.call({"action":"uploads", "subaction":"create", "filename":"axon-e2e-abort.txt",
                                  "content_type":"text/plain", "size_bytes":1, "purpose":"source_artifact", "response_mode":"inline"}))
    aborted_id = disposable.get("upload_id", disposable.get("id"))
    if not isinstance(aborted_id, str): raise wire.WireError("owned abort upload id missing")
    checks.call({"action":"uploads", "subaction":"abort", "upload_id":aborted_id, "response_mode":"inline"})
    return {"completed_upload":upload_id, "aborted_upload":aborted_id}


def job_ids(value):
    payload = data(value)
    items = payload.get("items", payload.get("jobs", []))
    if not isinstance(items, list): raise wire.WireError("job listing is not an array")
    return sorted(str(item.get("id", item.get("job_id"))) for item in items)


def check_capability_denial(args, env, checks):
    before = job_ids(checks.call({"action":"jobs", "subaction":"list", "limit":100, "response_mode":"inline"}))
    client = wire.transport(args, env, args.outdir / "incapable-server.stderr")
    try:
        wire.initialize(client, supports_tasks=False)
        denied = Checks(client)
        denied.call({"action":"extract", "subaction":"start", "urls":["https://example.com"], "prompt":"owned capability guard"},
                    error=True, meta={wire.EXTENSION:{}, "progressToken":"must-not-enqueue"})
        checks.results.extend(denied.results)
    finally:
        client.close()
    after = job_ids(checks.call({"action":"jobs", "subaction":"list", "limit":100, "response_mode":"inline"}))
    if before != after: raise wire.WireError("capability-denied request created a durable job")
    return {"before":before, "after":after, "no_enqueue":True}


def run(args):
    args.outdir = args.outdir.resolve()
    local = args.outdir / "data"
    local.mkdir(parents=True, exist_ok=True)
    config = args.outdir / "config.toml"
    config.write_text("")
    env = os.environ.copy()
    env.update({"AXON_HOME":str(local), "AXON_DATA_DIR":str(local), "AXON_CONFIG_PATH":str(config),
                "AXON_SQLITE_PATH":str(local / "jobs.db"), "AXON_MCP_TRANSPORT":"stdio",
                "AXON_MCP_TOOL_PROJECTION":args.projection,
                "AXON_COLLECTION":"axon_e2e_projection_contract"})
    env.setdefault("TEI_URL", "http://127.0.0.1:52000")
    env.setdefault("QDRANT_URL", "http://127.0.0.1:53333")
    client = wire.transport(args, env, args.outdir / "server.stderr")
    checks = Checks(client)
    try:
        wire.initialize(client)
        inventory, dashboard = check_discovery(checks, args.projection)
        for action, subaction in [("jobs","list"), ("uploads","list"), ("watch","list")]:
            checks.call({"action":action, "subaction":subaction, "response_mode":"inline"})
        ownership = check_upload_lifecycle(checks)
        for action in ("prune", "reset"):
            checks.call({"action":action, "subaction":"exec", "response_mode":"inline"}, error=True)
        checks.call({"action":"jobs", "subaction":"clear", "response_mode":"inline"}, error=True)
        checks.call({}, name="axon_query", error=True)
        if args.projection != "legacy":
            for value in ("jobs", None, 42, {}):
                checks.call({"action":value}, name="jobs_get", error=True)
                checks.call({"subaction":value}, name="jobs_get", error=True)
        checks.call({"action":"extract", "subaction":"start"}, name="axon_status_dashboard", error=True, meta={wire.EXTENSION:{}})
        capability = check_capability_denial(args, env, checks)
        evidence = {"schema_version":1, "surface":"mcp_projection_contract", "transport":args.transport,
                    "projection":args.projection, "call_form":args.call_form, "inventory":inventory,
                    "dashboard":dashboard, "owned":ownership, "capability_denial":capability,
                    "results":checks.results, "success":True}
        (args.outdir / "evidence.json").write_text(json.dumps(wire.sanitize(evidence), indent=2) + "\n")
        return evidence
    finally:
        client.close()
