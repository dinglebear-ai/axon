#!/usr/bin/env python3
"""Verify Axon's SEP-2663 task lifecycle over raw stdio or HTTP MCP."""
from __future__ import annotations
import argparse, importlib.util, json, os, queue, secrets, subprocess, sys, threading, time, urllib.error, urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
EXTENSION = "io.modelcontextprotocol/tasks"
TERMINAL = {"completed", "failed", "cancelled"}

class WireError(RuntimeError): pass

def rpc(identifier, method, params=None):
    value = {"jsonrpc": "2.0", "id": identifier, "method": method}
    if params is not None: value["params"] = params
    return value

def result(message, context):
    if "error" in message: raise WireError(f"{context}: {message['error']}")
    value = message.get("result")
    if not isinstance(value, dict): raise WireError(f"{context}: result object missing")
    return value

def structured_error(message, context):
    value = message.get("error")
    if not isinstance(value, dict) or not isinstance(value.get("code"), int) or not isinstance(value.get("message"), str):
        raise WireError(f"{context}: structured JSON-RPC error missing")
    return value

class Stdio:
    def __init__(self, binary, env, stderr_path, manifest=None):
        self.stderr = stderr_path.open("a", encoding="utf-8")
        nonce = secrets.token_hex(32)
        nonce_file = Path(env["AXON_DATA_DIR"]).parent/"process-ownership"/(nonce+".owner")
        if manifest:
            nonce_file.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
            nonce_file.write_text(nonce, encoding="utf-8"); nonce_file.chmod(0o600)
            env = env | {"AXON_E2E_PROCESS_NONCE":nonce}
        self.proc = subprocess.Popen([str(binary), "mcp"], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=self.stderr, text=True, env=env, start_new_session=True)
        if manifest:
            try:
                spec = importlib.util.spec_from_file_location("run_isolation", ROOT/"scripts/e2e/lib/run-isolation.py")
                isolation = importlib.util.module_from_spec(spec); spec.loader.exec_module(isolation)
                isolation.Manifest.open(Path(manifest)).register("process", str(self.proc.pid), {
                    "start_time":isolation._process_start_time(self.proc.pid), "nonce":nonce,
                    "nonce_file":str(nonce_file), "process_group":self.proc.pid, "argv0":Path(binary).name,
                })
            except Exception:
                self.proc.terminate(); self.proc.wait(timeout=5); self.stderr.close(); raise
        self.messages = queue.Queue()
        threading.Thread(target=self._read, daemon=True).start()
    def _read(self):
        for line in self.proc.stdout:
            try: self.messages.put(json.loads(line))
            except json.JSONDecodeError as error: self.messages.put({"_wire_error": str(error)})
    def request(self, payload, timeout=30):
        self.proc.stdin.write(json.dumps(payload, separators=(",", ":")) + "\n"); self.proc.stdin.flush()
        notices, deadline = [], time.monotonic() + timeout
        while time.monotonic() < deadline:
            try: message = self.messages.get(timeout=max(0.01, deadline-time.monotonic()))
            except queue.Empty: break
            if "_wire_error" in message: raise WireError(f"invalid server JSON: {message}")
            if message.get("id") == payload.get("id"): return message, notices
            notices.append(message)
        raise TimeoutError(payload.get("id"))
    def notify(self, payload):
        self.proc.stdin.write(json.dumps(payload, separators=(",", ":")) + "\n"); self.proc.stdin.flush()
    def malformed_probe(self):
        self.proc.stdin.write('{"jsonrpc":"2.0","id":998,"method":\n'); self.proc.stdin.flush()
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            message = self.messages.get(timeout=max(0.01, deadline-time.monotonic()))
            if message.get("id") in (998, None) and "error" in message: return structured_error(message, "malformed JSON")
        raise WireError("malformed JSON did not return a structured error")
    def close(self):
        self.proc.terminate()
        try: self.proc.wait(timeout=5)
        except subprocess.TimeoutExpired: self.proc.kill(); self.proc.wait(timeout=5)
        self.stderr.close()

class Http:
    def __init__(self, url, token=None, origin=None): self.url, self.token, self.origin, self.session = url, token, origin, None
    def headers(self):
        value = {"content-type":"application/json", "accept":"application/json, text/event-stream"}
        if self.token: value["authorization"] = f"Bearer {self.token}"
        if self.origin: value["origin"] = self.origin
        if self.session: value["mcp-session-id"] = self.session
        return value
    def request(self, payload, timeout=30):
        request = urllib.request.Request(self.url, json.dumps(payload, separators=(",", ":")).encode(), self.headers(), method="POST")
        try:
            with urllib.request.urlopen(request, timeout=timeout) as response:
                self.session = response.headers.get("mcp-session-id", self.session)
                body, kind = response.read().decode(), response.headers.get_content_type()
        except urllib.error.HTTPError as error:
            raise WireError(f"HTTP {error.code}: {error.read().decode(errors='replace')[:512]}") from error
        messages = ([json.loads(line[5:].strip()) for line in body.splitlines() if line.startswith("data:")]
                    if kind == "text/event-stream" else [json.loads(body)])
        matched = next((item for item in messages if item.get("id") == payload.get("id")), None)
        if matched is None: raise WireError(f"response id missing: {payload.get('id')}")
        return matched, [item for item in messages if item is not matched]
    def notify(self, payload):
        request = urllib.request.Request(self.url, json.dumps(payload, separators=(",", ":")).encode(), self.headers(), method="POST")
        try:
            with urllib.request.urlopen(request, timeout=30) as response:
                self.session = response.headers.get("mcp-session-id", self.session)
                response.read()
        except urllib.error.HTTPError as error:
            raise WireError(f"HTTP notification {error.code}: {error.read().decode(errors='replace')[:512]}") from error
    def malformed_probe(self):
        request = urllib.request.Request(self.url, b'{"jsonrpc":"2.0","id":998,"method":', self.headers(), method="POST")
        try:
            with urllib.request.urlopen(request, timeout=10) as response: body = response.read().decode()
        except urllib.error.HTTPError as error: body = error.read().decode(errors="replace")
        try: message = json.loads(body)
        except json.JSONDecodeError as error: raise WireError("malformed JSON response was not JSON") from error
        return structured_error(message, "malformed JSON")
    def close(self): pass

def initialize(transport, identifier=1):
    message, _ = transport.request(rpc(identifier, "initialize", {"protocolVersion":"2025-11-25",
        "capabilities":{"extensions":{EXTENSION:{}}}, "clientInfo":{"name":"axon-e2e-task-wire","version":"1"}}))
    value = result(message, "initialize")
    if EXTENSION not in value.get("capabilities", {}).get("extensions", {}): raise WireError("tasks extension not negotiated")
    transport.notify({"jsonrpc":"2.0", "method":"notifications/initialized"})
    return value

def create(transport, identifier, url, prompt, token):
    message, notices = transport.request(rpc(identifier, "tools/call", {"name":"axon", "arguments":{
        "action":"extract", "subaction":"start", "urls":[url], "prompt":prompt, "max_pages":1},
        "_meta":{EXTENSION:{}, "progressToken":token}}), timeout=60)
    value = result(message, "create"); task_id = value.get("taskId")
    if not isinstance(task_id, str) or not task_id: raise WireError("taskId missing")
    if not progress_values(notices): raise WireError("task create omitted the required initial progress notification")
    return task_id, notices, value

def poll(transport, task_id, first_id, attempts=24, delay=1):
    states, notices, detail = [], [], {}
    for offset in range(attempts):
        message, seen = transport.request(rpc(first_id+offset, "tasks/get", {"taskId":task_id}))
        notices += seen; detail = result(message, "tasks/get"); state = detail.get("status")
        if state not in {"working","input_required",*TERMINAL}: raise WireError(f"invalid task state: {state}")
        states.append(state)
        if state in TERMINAL: break
        time.sleep(delay)
    if not states or states[-1] not in TERMINAL: raise WireError(f"task did not reach a terminal state: {states}")
    return states, detail, notices

def progress_values(notices):
    values = [float(item["params"]["progress"]) for item in notices if item.get("method") == "notifications/progress"
              and isinstance(item.get("params", {}).get("progress"), (int,float))]
    if values != sorted(values): raise WireError(f"progress regressed: {values}")
    return values

def transport(args, env, stderr):
    return Http(args.base_url, args.token, args.origin) if args.transport == "http" else Stdio(args.binary, env, stderr, args.manifest)

def run(args):
    outdir = args.outdir.resolve(); data = outdir/"data"; data.mkdir(parents=True, exist_ok=True)
    env = os.environ.copy(); env.update({"AXON_HOME":str(data), "AXON_DATA_DIR":str(data),
        "AXON_SQLITE_PATH":str(data/"jobs.db"), "AXON_MCP_TRANSPORT":"stdio"})
    client = transport(args, env, outdir/"server.stderr")
    evidence = {"schema_version":1, "surface":"mcp_task_wire", "transport":args.transport}
    try:
        evidence["initialize"] = initialize(client)
        evidence["malformed_json"] = client.malformed_probe()
        invalid, _ = client.request(rpc(8, "tools/call", {"name":"axon", "arguments":{"action":17}}))
        evidence["invalid_arguments"] = structured_error(invalid, "invalid tool arguments")
        task_id, creation_notices, created = create(client, 10, args.url, "Extract the page title.", "axon-e2e-progress")
        before_terminal, _ = client.request(rpc(11, "tasks/result", {"taskId":task_id}))
        evidence["tasks_result_before_terminal"] = structured_error(before_terminal, "tasks/result before terminal")
        states, detail, poll_notices = poll(client, task_id, 20, delay=args.poll_interval)
        evidence.update({"task_id":task_id, "created":created, "states":states, "detailed":detail,
                         "progress":progress_values(creation_notices+poll_notices)})
        # rmcp 3.x inlines terminal content in tasks/get. Legacy result/list are
        # deliberately probed as structured unsupported-method responses.
        for identifier, method, params in ((60,"tasks/result",{"taskId":task_id}), (61,"tasks/list",{}),
                (62,"tasks/get",{"taskId":"extract:not-a-uuid"}), (63,"unknown/method",{})):
            response, _ = client.request(rpc(identifier, method, params))
            evidence[method.replace("/","_")] = structured_error(response, method)
        cancel_id, _, cancel_created = create(client, 70, args.url, "Extract every visible sentence.", "axon-e2e-cancel")
        cancelled, _ = client.request(rpc(71, "tasks/cancel", {"taskId":cancel_id}))
        if "error" in cancelled: structured_error(cancelled, "cancel/complete race")
        elif cancelled.get("result") is not None: raise WireError("tasks/cancel returned a non-empty acknowledgement")
        cancel_states, cancel_detail, cancel_notices = poll(client, cancel_id, 72, attempts=8, delay=args.poll_interval)
        evidence.update({"cancel_created":cancel_created, "cancel_states":cancel_states,
                         "cancel_detail":cancel_detail, "cancel_progress":progress_values(cancel_notices)})
        abandoned_id, _, _ = create(client, 90, args.url, "Extract headings for abandonment recovery.", "axon-e2e-abandon")
        evidence["abandoned_task_id"] = abandoned_id
    finally: client.close()
    reopened = transport(args, env, outdir/"server.stderr")
    try:
        initialize(reopened, 100)
        message, _ = reopened.request(rpc(101, "tasks/get", {"taskId":evidence["task_id"]}))
        evidence["reconnected"] = result(message, "reconnected tasks/get")
        abandoned, _ = reopened.request(rpc(102, "tasks/get", {"taskId":evidence["abandoned_task_id"]}))
        evidence["abandoned_after_reconnect"] = result(abandoned, "abandoned tasks/get")
        abandoned_cancel, _ = reopened.request(rpc(103, "tasks/cancel", {"taskId":evidence["abandoned_task_id"]}))
        if "error" in abandoned_cancel: structured_error(abandoned_cancel, "abandoned cancel/complete race")
        elif abandoned_cancel.get("result") is not None: raise WireError("abandoned cancel returned non-empty acknowledgement")
        final_states, final_detail, _ = poll(reopened, evidence["abandoned_task_id"], 104, attempts=8, delay=args.poll_interval)
        evidence["abandoned_terminal_states"] = final_states
        evidence["abandoned_terminal"] = final_detail
    finally: reopened.close()
    evidence["success"] = True
    return evidence

def arguments():
    parser = argparse.ArgumentParser()
    parser.add_argument("--transport", choices=("stdio","http"), default=os.getenv("AXON_MCP_TASK_TRANSPORT","stdio"))
    parser.add_argument("--binary", type=Path, default=Path(os.getenv("AXON_MCP_BINARY", ROOT/"target/debug/axon")))
    parser.add_argument("--base-url", default=os.getenv("AXON_MCP_URL","http://127.0.0.1:8080/mcp"))
    parser.add_argument("--token", default=os.getenv("AXON_HTTP_TOKEN")); parser.add_argument("--origin", default=os.getenv("AXON_MCP_ORIGIN"))
    parser.add_argument("--url", default=os.getenv("REAL_PAGE_URL","https://example.com"))
    parser.add_argument("--outdir", type=Path, default=Path(os.getenv("AXON_MCP_TASK_OUTDIR", ROOT/".cache/mcp-tasks-wire")))
    parser.add_argument("--manifest", default=os.getenv("AXON_E2E_MANIFEST")); parser.add_argument("--poll-interval", type=float, default=1)
    return parser.parse_args()

if __name__ == "__main__":
    try: json.dump(run(arguments()), sys.stdout, indent=2, sort_keys=True); print()
    except (WireError, TimeoutError, OSError, json.JSONDecodeError) as error:
        json.dump({"schema_version":1,"surface":"mcp_task_wire","success":False,"error":str(error)}, sys.stdout); print(); raise SystemExit(1)
