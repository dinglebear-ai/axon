#!/usr/bin/env python3
"""Drive Axon's SEP-2663 task lifecycle over raw MCP stdio JSON-RPC."""

import json
import os
import subprocess
import sys
import time


def send(proc, payload):
    proc.stdin.write(json.dumps(payload, separators=(",", ":")) + "\n")
    proc.stdin.flush()


def receive(proc, response_id, deadline=30):
    end = time.time() + deadline
    while time.time() < end:
        line = proc.stdout.readline()
        if not line:
            raise RuntimeError("MCP server closed stdout")
        message = json.loads(line)
        if message.get("id") == response_id:
            return message
    raise TimeoutError(response_id)


root = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
outdir = os.path.abspath(os.environ.get("AXON_MCP_TASK_OUTDIR", os.path.join(root, ".cache/mcp-tasks-wire")))
data_dir = os.path.join(outdir, "data")
os.makedirs(data_dir, exist_ok=True)
env = os.environ.copy()
env.update({
    "AXON_HOME": data_dir,
    "AXON_DATA_DIR": data_dir,
    "AXON_SQLITE_PATH": os.path.join(data_dir, "jobs.db"),
    "AXON_MCP_TRANSPORT": "stdio",
})
binary = os.path.abspath(os.environ.get("AXON_MCP_BINARY", os.path.join(root, "target/debug/axon")))
url = os.environ.get("REAL_PAGE_URL", "https://example.com")
stderr = open(os.path.join(outdir, "server.stderr"), "w")
proc = subprocess.Popen(
    [binary, "mcp"], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
    stderr=stderr, text=True, env=env,
)
try:
    extension = {"io.modelcontextprotocol/tasks": {}}
    send(proc, {
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {
            "protocolVersion": "2025-11-25",
            "capabilities": {"extensions": extension},
            "clientInfo": {"name": "axon-live-task-harness", "version": "1"},
        },
    })
    initialized = receive(proc, 1)
    send(proc, {"jsonrpc": "2.0", "method": "notifications/initialized"})

    def create(request_id, prompt, progress=False):
        meta = {"io.modelcontextprotocol/tasks": {}}
        if progress:
            meta["progressToken"] = "axon-live-task"
        send(proc, {
            "jsonrpc": "2.0", "id": request_id, "method": "tools/call",
            "params": {
                "name": "axon",
                "arguments": {
                    "action": "extract", "subaction": "start", "urls": [url],
                    "prompt": prompt, "max_pages": 1,
                },
                "_meta": meta,
            },
        })
        return receive(proc, request_id, 60)

    created = create(2, "Extract the page title.", progress=True)
    task_id = created.get("result", {}).get("taskId")
    if not task_id:
        raise RuntimeError(f"task creation did not return taskId: {created}")
    states = []
    detailed = None
    for request_id in range(3, 27):
        send(proc, {"jsonrpc": "2.0", "id": request_id, "method": "tasks/get", "params": {"taskId": task_id}})
        detailed = receive(proc, request_id)
        status = detailed.get("result", {}).get("status")
        states.append(status)
        if status in {"completed", "failed", "cancelled"}:
            break
        time.sleep(5)

    send(proc, {"jsonrpc": "2.0", "id": 30, "method": "tasks/result", "params": {"taskId": task_id}})
    removed_result = receive(proc, 30)
    send(proc, {"jsonrpc": "2.0", "id": 31, "method": "tasks/list", "params": {}})
    removed_list = receive(proc, 31)

    cancel_created = create(40, "Extract every visible sentence and classify each one.")
    cancel_task_id = cancel_created["result"]["taskId"]
    send(proc, {"jsonrpc": "2.0", "id": 41, "method": "tasks/cancel", "params": {"taskId": cancel_task_id}})
    cancelled = receive(proc, 41)
    cancel_states = []
    cancelled_detail = None
    for request_id in range(42, 48):
        send(proc, {"jsonrpc": "2.0", "id": request_id, "method": "tasks/get", "params": {"taskId": cancel_task_id}})
        cancelled_detail = receive(proc, request_id)
        status = cancelled_detail.get("result", {}).get("status")
        cancel_states.append(status)
        if status in {"cancelled", "completed", "failed"}:
            break
        time.sleep(1)

    json.dump({
        "initialize": initialized, "created": created, "task_id": task_id,
        "states": states, "detailed": detailed,
        "tasks_result_probe": removed_result, "tasks_list_probe": removed_list,
        "cancel_created": cancel_created, "cancelled": cancelled,
        "cancelled_detail": cancelled_detail, "cancel_states": cancel_states,
    }, sys.stdout, indent=2)
    print()
finally:
    proc.terminate()
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()
    stderr.close()
