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
CANCEL = {"jsonrpc":"2.0","id":2,"method":"tasks/cancel","params":{"taskId":"extract:00000000-0000-0000-0000-000000000000"}}
TASK_GET = {"jsonrpc":"2.0","id":3,"method":"tasks/get","params":{"taskId":"extract:00000000-0000-0000-0000-000000000000"}}
RESOURCE = {"jsonrpc":"2.0","id":4,"method":"resources/read","params":{"uri":"axon://schema/mcp-tool"}}

def request(url, payload, headers):
    req = urllib.request.Request(url, json.dumps(payload,separators=(",",":")).encode(),
        {"content-type":"application/json","accept":"application/json, text/event-stream",**headers}, method="POST")
    try:
        with urllib.request.urlopen(req, timeout=10) as response:
            return response.status, dict(response.headers), response.read().decode(errors="replace")
    except urllib.error.HTTPError as error:
        return error.code, dict(error.headers), error.read().decode(errors="replace")

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
            ("valid_task_stream", TASK_GET, {"authorization":f"Bearer {valid_token}"}, {200}),
            ("valid_task_cancel", CANCEL, {"authorization":f"Bearer {valid_token}"}, {200}),
            ("conflicting_credentials", INITIALIZE, {"authorization":f"Bearer {valid_token}","x-api-key":"invalid-e2e-token"}, {400,401,403}),
            ("denied_origin", INITIALIZE, {"authorization":f"Bearer {valid_token}","origin":denied_origin}, {400,403}),
        ]
        if allowed_origin:
            cases.append(("allowed_origin", INITIALIZE, {"authorization":f"Bearer {valid_token}","origin":allowed_origin}, {200}))
    if read_token:
        cases += [
            ("read_scope_resource", RESOURCE, {"authorization":f"Bearer {read_token}"}, {200}),
            ("read_scope_task_stream", TASK_GET, {"authorization":f"Bearer {read_token}"}, {200}),
            ("read_scope_cancel_denied", CANCEL, {"authorization":f"Bearer {read_token}"}, {401,403}),
        ]
    evidence, failures = [], []
    for name, payload, headers, expected in cases:
        status, response_headers, body = request(url, payload, headers)
        passed = status in expected
        if name == "valid_sse" and status == 200:
            passed = "event-stream" in response_headers.get("Content-Type", "") or body.lstrip().startswith("{")
        if not passed: failures.append(f"{name}: HTTP {status}, expected {sorted(expected)}")
        evidence.append({"case":name,"status":status,"passed":passed,"unauthorized_shape":body.strip() if status in {401,403} else None})
    unauthorized = [item for item in evidence if item["case"].startswith(("missing", "invalid"))]
    shapes = {(item["status"], item["unauthorized_shape"]) for item in unauthorized}
    if len(shapes) != 1:
        failures.append("unauthorized resource/task responses are distinguishable")
    return {"schema_version":1,"surface":"mcp_http_auth","success":not failures,"cases":evidence,"failures":failures}

def main():
    parser=argparse.ArgumentParser(); parser.add_argument("url"); parser.add_argument("--allowed-origin",default=os.getenv("AXON_MCP_ALLOWED_ORIGIN"))
    args=parser.parse_args(); result=matrix(args.url,os.getenv("AXON_MCP_AUTH_TOKEN"),os.getenv("AXON_MCP_READ_TOKEN"),args.allowed_origin)
    print(json.dumps(result,sort_keys=True)); return int(not result["success"])
if __name__ == "__main__": raise SystemExit(main())
