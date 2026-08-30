#!/usr/bin/env python3
"""Executable HTTP MCP authorization-policy matrix.

Tokens are accepted only through environment variables and never included in
evidence. The target should be a loopback fixture or explicitly trusted live
endpoint prepared by the E2E runner.
"""
from __future__ import annotations
import argparse, json, os, urllib.error, urllib.request

INITIALIZE = {"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25",
    "capabilities":{},"clientInfo":{"name":"axon-e2e-auth","version":"1"}}}
INITIALIZE_TASKS = {"jsonrpc":"2.0","id":10,"method":"initialize","params":{"protocolVersion":"2025-11-25",
    "capabilities":{"extensions":{"io.modelcontextprotocol/tasks":{}}},
    "clientInfo":{"name":"axon-e2e-auth-tasks","version":"1"}}}
CANCEL = {"jsonrpc":"2.0","id":2,"method":"tasks/cancel","params":{"taskId":"extract:00000000-0000-0000-0000-000000000000"}}
TASK_GET = {"jsonrpc":"2.0","id":3,"method":"tasks/get","params":{"taskId":"extract:00000000-0000-0000-0000-000000000000"}}
RESOURCE = {"jsonrpc":"2.0","id":4,"method":"resources/read","params":{"uri":"axon://schema/mcp-tool"}}
TOOL_CANCEL = {"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"axon","arguments":{
    "action":"jobs","subaction":"cancel","job_id":"00000000-0000-0000-0000-000000000000","job_kind":"extract"}}}

def request(url, payload, headers):
    req = urllib.request.Request(url, json.dumps(payload,separators=(",",":")).encode(),
        {"content-type":"application/json","accept":"application/json, text/event-stream",**headers}, method="POST")
    try:
        with urllib.request.urlopen(req, timeout=10) as response:
            return response.status, dict(response.headers), response.read().decode(errors="replace")
    except urllib.error.HTTPError as error:
        try:return error.code, dict(error.headers), error.read().decode(errors="replace")
        finally:error.close()

def rpc_error_shape(body):
    candidate=body.strip()
    if candidate.startswith("event:"):
        candidate="".join(line.removeprefix("data: ") for line in candidate.splitlines() if line.startswith("data:"))
    try:
        decoded=json.loads(candidate)
    except json.JSONDecodeError:
        return None
    error=decoded.get("error") if isinstance(decoded,dict) else None
    if not isinstance(error,dict):return None
    return {"code":error.get("code"),"message":error.get("message")}

def matrix(url, valid_token=None, read_token=None, allowed_origin=None, denied_origin="https://denied.invalid"):
    cases = [
        ("missing", INITIALIZE, {}, {401}),
        ("invalid", INITIALIZE, {"authorization":"Bearer invalid-e2e-token"}, {401,403}),
        ("missing_resource", RESOURCE, {}, {401}),
        ("invalid_resource", RESOURCE, {"authorization":"Bearer invalid-e2e-token"}, {401,403}),
        ("missing_task_stream", TASK_GET, {}, {401}),
        ("invalid_task_stream", TASK_GET, {"authorization":"Bearer invalid-e2e-token"}, {401,403}),
        ("missing_task_cancel", CANCEL, {}, {401}),
        ("invalid_task_cancel", CANCEL, {"authorization":"Bearer invalid-e2e-token"}, {401,403}),
    ]
    if valid_token:
        cases += [
            ("valid_sse", INITIALIZE, {"authorization":f"Bearer {valid_token}"}, {200}),
            ("valid_resource", RESOURCE, {"authorization":f"Bearer {valid_token}"}, {200}),
            ("valid_task_cancel", TOOL_CANCEL, {"authorization":f"Bearer {valid_token}"}, {200}),
            ("conflicting_credentials", INITIALIZE, {"authorization":f"Bearer {valid_token}","x-api-key":"invalid-e2e-token"}, {400,401,403}),
            ("denied_origin", INITIALIZE, {"authorization":f"Bearer {valid_token}","origin":denied_origin}, {400,403}),
        ]
        if allowed_origin:
            cases.append(("allowed_origin", INITIALIZE, {"authorization":f"Bearer {valid_token}","origin":allowed_origin}, {200}))
    if read_token:
        cases += [
            ("read_scope_resource", RESOURCE, {"authorization":f"Bearer {read_token}"}, {200}),
            # Exercise cancellation through the real Axon MCP tool rather
            # than protocol-level tasks/cancel: Axon's HTTP MCP transport is
            # intentionally stateless, so the SDK rejects lifecycle methods
            # before server authorization when no negotiated session exists.
            ("read_scope_cancel_denied", TOOL_CANCEL, {"authorization":f"Bearer {read_token}"}, {200,401,403}),
        ]
    evidence, failures = [], []
    for name, payload, headers, expected in cases:
        if payload in (TASK_GET,CANCEL) and (name.startswith("valid_") or name.startswith("read_scope_")):
            init_status,init_headers,_=request(url,INITIALIZE_TASKS,headers)
            session=next((value for key,value in init_headers.items() if key.casefold()=="mcp-session-id"),None)
            if init_status!=200 or not session:
                failures.append(f"{name}: task-capable MCP initialization failed: HTTP {init_status}")
            else:
                headers={**headers,"mcp-session-id":session}
        status, response_headers, body = request(url, payload, headers)
        rpc_error=rpc_error_shape(body)
        passed = status in expected
        if name == "valid_sse" and status == 200:
            passed = "event-stream" in response_headers.get("Content-Type", "") or body.lstrip().startswith("{")
        if name == "read_scope_cancel_denied" and status == 200:
            folded = json.dumps(rpc_error or {},sort_keys=True).casefold()
            passed = "forbidden" in folded and "axon:write" in folded
        if name == "valid_task_cancel" and status == 200:
            folded = json.dumps(rpc_error or {},sort_keys=True).casefold()
            passed = "forbidden" not in folded and "axon:write" not in folded
        if not passed: failures.append(f"{name}: HTTP {status}, expected {sorted(expected)}, rpc_error={rpc_error}")
        unauthorized_shape = None
        if status in {401,403}:
            try:
                decoded = json.loads(body)
                error = decoded.get("error", decoded) if isinstance(decoded, dict) else decoded
                if isinstance(error, dict):
                    unauthorized_shape = json.dumps({"code": error.get("code"), "message": error.get("message")},
                                                    sort_keys=True, separators=(",", ":"))
                else:
                    unauthorized_shape = json.dumps(error, sort_keys=True, separators=(",", ":"))
            except json.JSONDecodeError:
                unauthorized_shape = body.strip()
        evidence.append({"case":name,"status":status,"passed":passed,"unauthorized_shape":unauthorized_shape,
                         "rpc_error":rpc_error})
    for credential_class in ("missing", "invalid"):
        unauthorized = [item for item in evidence if item["case"].startswith(credential_class)]
        shapes = {(item["status"], item["unauthorized_shape"]) for item in unauthorized}
        if len(shapes) != 1:
            failures.append(f"{credential_class} resource/task responses are distinguishable")
    return {"schema_version":1,"surface":"mcp_http_auth","success":not failures,"cases":evidence,"failures":failures}

def main():
    parser=argparse.ArgumentParser(); parser.add_argument("url"); parser.add_argument("--allowed-origin",default=os.getenv("AXON_MCP_ALLOWED_ORIGIN"))
    args=parser.parse_args(); result=matrix(args.url,os.getenv("AXON_MCP_AUTH_TOKEN"),os.getenv("AXON_MCP_READ_TOKEN"),args.allowed_origin)
    print(json.dumps(result,sort_keys=True)); return int(not result["success"])
if __name__ == "__main__": raise SystemExit(main())
