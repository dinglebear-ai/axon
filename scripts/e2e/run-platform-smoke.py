#!/usr/bin/env python3
"""Bounded, no-secret platform smoke selected from canonical catalog tags."""
from __future__ import annotations

import argparse
import ctypes
import importlib.util
import json
import os
import queue
import shutil
import secrets
import signal
import socket
import sqlite3
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def load(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None: raise RuntimeError(f"cannot load {path}")
    module = importlib.util.module_from_spec(spec); sys.modules[name] = module; spec.loader.exec_module(module); return module


isolation = load("axon_platform_isolation", ROOT / "scripts/e2e/lib/run-isolation.py")
teardown = load("axon_platform_teardown", ROOT / "scripts/e2e/lib/teardown.py")
reporting = load("axon_platform_reporting", ROOT / "scripts/e2e/lib/reporting.py")
cli_adapter = load("axon_platform_cli_adapter", ROOT / "scripts/e2e/adapters/cli.py")


class CheckError(RuntimeError): pass


def register_outer_cleanup(manifest) -> None:
    registry = os.environ.get("AXON_E2E_CLEANUP_REGISTRY")
    if not registry: return
    report = manifest.path.parent / "outer-registry-registration.json"
    completed = subprocess.run([sys.executable, str(ROOT / "scripts/e2e/cleanup-owned-runs.py"),
        "--registry", registry, "--register-manifest", str(manifest.path), "--report", str(report)],
        cwd=ROOT, capture_output=True, text=True, timeout=15)
    ensure(completed.returncode == 0, f"outer cleanup registration failed: {completed.stderr[:300]}")


class _JobBasicLimit(ctypes.Structure):
    _fields_ = [("per_process", ctypes.c_int64), ("per_job", ctypes.c_int64),
                ("limit_flags", ctypes.c_uint32), ("minimum_working_set", ctypes.c_size_t),
                ("maximum_working_set", ctypes.c_size_t), ("active_process_limit", ctypes.c_uint32),
                ("affinity", ctypes.c_size_t), ("priority_class", ctypes.c_uint32),
                ("scheduling_class", ctypes.c_uint32)]


class _IoCounters(ctypes.Structure):
    _fields_ = [(name, ctypes.c_uint64) for name in
                ("read_operations", "write_operations", "other_operations",
                 "read_bytes", "write_bytes", "other_bytes")]


class _JobExtendedLimit(ctypes.Structure):
    _fields_ = [("basic", _JobBasicLimit), ("io", _IoCounters),
                ("process_memory_limit", ctypes.c_size_t), ("job_memory_limit", ctypes.c_size_t),
                ("peak_process_memory", ctypes.c_size_t), ("peak_job_memory", ctypes.c_size_t)]


def ensure(value: bool, message: str) -> None:
    if not value: raise CheckError(message)


def selected_catalog() -> list[str]:
    catalog = json.loads((ROOT / "tests/e2e/catalog/catalog.json").read_text(encoding="utf-8"))
    selected = [item["id"] for item in catalog["scenarios"] if "platform_smoke" in item.get("tags", [])]
    ensure(bool(selected), "platform tag selection is empty")
    return selected


def axon_env(data_dir: Path) -> dict[str, str]:
    return {**os.environ, "AXON_DATA_DIR":str(data_dir), "AXON_SQLITE_PATH":str(data_dir / "jobs.db"),
            "AXON_JOBS_AUTO_WORKER":"false",
            "TEI_URL":"http://127.0.0.1:1", "QDRANT_URL":"http://127.0.0.1:1",
            "AXON_LLM_BACKEND":"openai-compat", "AXON_OPENAI_BASE_URL":"http://127.0.0.1:1",
            "AXON_SERVER_URL":"http://127.0.0.1:1"}


def executable_and_paths(binary: Path, run_root: Path, _env: dict[str, str]) -> dict:
    python = shutil.which("python") or shutil.which("python3")
    ensure(python is not None and Path(python).is_file(), "Python executable resolution failed")
    corpus = ROOT / "tests/e2e/corpus/v1/documents/micro/unicode-東京-🧪.txt"
    fixtures=run_root/"path-fixtures";fixtures.mkdir()
    names = ["space name.txt", "unicode-東京-🧪.txt", ".axon-dotfile", "extensionless"]
    axon_results = []
    for index,name in enumerate(names):
        path = fixtures / name; shutil.copyfile(corpus, path)
        ensure(path.read_bytes() == corpus.read_bytes(), f"corpus byte round trip failed for {name}")
        runtime=run_root/f"path-runtime-{index}";runtime.mkdir()
        result = subprocess.run([str(binary), str(path), "--wait", "false", "--json"], env=axon_env(runtime),
                                capture_output=True, text=True, timeout=20)
        ensure(result.returncode == 0, f"Axon rejected native path {name}: {result.stderr[:300]}")
        envelope = json.loads(result.stdout); ensure(bool(envelope.get("job_id")), f"Axon omitted job ID for {name}")
        axon_results.append({"name":name,"job_id":reporting.opaque(envelope["job_id"])})
    mixed = fixtures / "CaseProbe"; shutil.copyfile(corpus, mixed)
    aliases = (fixtures / "caseprobe").exists()
    expected_alias = os.name == "nt" or sys.platform == "darwin"
    # macOS can be configured case-sensitive; record the native result instead
    # of imposing a filesystem setting that the product does not control.
    if os.name == "nt": ensure(aliases, "Windows path lookup unexpectedly became case-sensitive")
    ensure(os.pathsep in os.environ.get("PATH", ""), "native executable search path is malformed")
    return {"python": Path(python).name, "axon_binary":binary.name, "axon_paths":axon_results,
            "argv_round_trip": "real-axon-direct-exec-no-shell",
            "case_alias": aliases, "case_default_expected": expected_alias,
            "separator": os.sep, "path_separator": os.pathsep}


def config_roots(binary: Path, run_root: Path, base_env: dict[str, str]) -> dict:
    homes = []
    for label, value in (("missing", None), ("empty", "")):
        home = run_root / f"home {label} 東京"; home.mkdir()
        candidate = {**base_env, "HOME":str(home), "USERPROFILE":str(home)}
        if value is None: candidate.pop("AXON_DATA_DIR", None)
        else: candidate["AXON_DATA_DIR"] = value
        result = subprocess.run([str(binary), "config", "path", "--json"], env=candidate,
                                capture_output=True, text=True, timeout=10)
        ensure(result.returncode == 0, f"Axon config path failed for {label}: {result.stderr[:300]}")
        resolved = json.loads(result.stdout)
        for key in ("env_path", "toml_path"):
            ensure(home.resolve() in Path(resolved[key]).resolve().parents, f"Axon {key} escaped isolated home")
        homes.append({"case":label,"env":Path(resolved["env_path"]).name,"toml":Path(resolved["toml_path"]).name})
    return {"missing_and_empty_safe": True, "actual_axon_results":homes}


def stdio_mcp(binary: Path, env: dict[str, str], manifest, run_root: Path) -> dict:
    nonce = secrets.token_hex(32); nonce_dir = run_root / "mcp-process-ownership"; nonce_dir.mkdir()
    nonce_file = nonce_dir / f"{nonce}.owner"; isolation._private_write(nonce_file, nonce.encode())
    manifest.register("temp_path", str(nonce_dir)); manifest.register("temp_path", str(nonce_file))
    child_env = {**env,"AXON_E2E_PROCESS_NONCE":nonce,"AXON_MCP_TRANSPORT":"stdio"}
    proc = subprocess.Popen([str(binary), "mcp", "--transport", "stdio"], stdin=subprocess.PIPE,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, env=child_env,
        start_new_session=os.name != "nt", creationflags=getattr(subprocess,"CREATE_NEW_PROCESS_GROUP",0) if os.name == "nt" else 0)
    assert proc.stdin and proc.stdout
    manifest.register("process", str(proc.pid), {"start_time":isolation._process_start_time(proc.pid),"nonce":nonce,
        "nonce_file":str(nonce_file),"process_group":proc.pid,"argv0":binary.name})
    messages: queue.Queue = queue.Queue()
    def reader():
        for line in proc.stdout:
            try: messages.put(json.loads(line))
            except json.JSONDecodeError: messages.put({"invalid":True})
    threading.Thread(target=reader,daemon=True).start()
    def request(identifier: int, method: str, params: dict):
        proc.stdin.write(json.dumps({"jsonrpc":"2.0","id":identifier,"method":method,"params":params})+"\n"); proc.stdin.flush()
        deadline=time.monotonic()+20
        while time.monotonic()<deadline:
            message=messages.get(timeout=max(.01,deadline-time.monotonic()))
            if message.get("id")==identifier:return message
        raise CheckError(f"Axon MCP timed out during {method}")
    initialized=request(1,"initialize",{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"platform-smoke","version":"1"}})
    ensure("result" in initialized,"Axon MCP initialize failed")
    proc.stdin.write(json.dumps({"jsonrpc":"2.0","method":"notifications/initialized"})+"\n");proc.stdin.flush()
    tools=request(2,"tools/list",{}); names=[item.get("name") for item in tools.get("result",{}).get("tools",[])]
    ensure("axon" in names,"Axon MCP tools/list omitted axon")
    # Leave the live owned process for authoritative teardown, proving native
    # CLI subprocess cancellation rather than relying on cooperative shutdown.
    return {"messages":2,"transport":"axon-mcp-stdio","owned_process":reporting.opaque(str(proc.pid))}


def execute_catalog_scenarios(binary: Path, env: dict[str, str], selected: list[str]) -> list:
    catalog=json.loads((ROOT/"tests/e2e/catalog/catalog.json").read_text(encoding="utf-8")); by_id={s["id"]:s for s in catalog["scenarios"]}
    records=[]
    for scenario_id in selected:
        scenario=by_id[scenario_id]; fixture=json.loads((ROOT/scenario["requests"]["cli"]).read_text(encoding="utf-8"))
        argv=[str(binary),*cli_adapter.scenario_argv(scenario,fixture,{})]; started=time.monotonic()
        completed=subprocess.run(argv,env=env,capture_output=True,timeout=20)
        normalized,_=cli_adapter.normalized_failure_envelope(scenario,completed.returncode,completed.stdout,completed.stderr)
        result,failure,assertions=cli_adapter.classify(completed.returncode,normalized,completed.stderr,scenario)
        item=reporting.Scenario(scenario_id,"hermetic",scenario["capability"],"cli")
        if result=="pass":item.attempt("passed",int((time.monotonic()-started)*1000))
        else:item.attempt("failed",int((time.monotonic()-started)*1000),classification="product" if failure=="product" else "harness",summary=f"catalog adapter result {result}")
        item.cleanup={"success":True,"residual":[],"refused":[]}
        item.invariants=[{"catalog_tag":"platform_smoke","adapter_assertions":assertions}]; records.append(item)
    return records


def sqlite_lock(path: Path) -> dict:
    owner = sqlite3.connect(path, timeout=0.1); contender = sqlite3.connect(path, timeout=0.1)
    try:
        owner.execute("CREATE TABLE smoke(value TEXT)"); owner.commit(); owner.execute("BEGIN EXCLUSIVE")
        try: contender.execute("INSERT INTO smoke VALUES ('blocked')"); contender.commit()
        except sqlite3.OperationalError as error: ensure("locked" in str(error).lower(), "unexpected SQLite lock failure")
        else: raise CheckError("locked SQLite file accepted a competing writer")
    finally: contender.close(); owner.rollback(); owner.close()
    return {"exclusive_lock": "enforced"}


def port_lease(lease_root: Path, run_id: str, manifest) -> dict:
    reservation = isolation.allocate_port(lease_root, run_id, manifest)
    probe = socket.socket()
    try:
        try: probe.bind(("127.0.0.1", reservation.port))
        except OSError: pass
        else: raise CheckError("held port lease was concurrently bindable")
    finally: probe.close(); reservation.close()
    return {"held_then_released": True}


def _pid_alive(pid: int) -> bool:
    if os.name == "nt":
        kernel=ctypes.windll.kernel32;handle=kernel.OpenProcess(0x1000,False,pid)
        if not handle:return False
        code=ctypes.c_uint32()
        try:return bool(kernel.GetExitCodeProcess(handle,ctypes.byref(code))) and code.value==259
        finally:kernel.CloseHandle(handle)
    try: os.kill(pid, 0); return True
    except ProcessLookupError: return False


def terminate_tree(manifest, run_root: Path) -> dict:
    fixture = ROOT / "tests/e2e/platform/tree_child.py"
    flags = getattr(subprocess, "CREATE_NEW_PROCESS_GROUP", 0) if os.name == "nt" else 0
    nonce = secrets.token_hex(32); nonce_dir = run_root / "process-ownership"; nonce_dir.mkdir()
    nonce_file = nonce_dir / f"{nonce}.owner"; isolation._private_write(nonce_file, nonce.encode())
    manifest.register("temp_path", str(nonce_dir)); manifest.register("temp_path", str(nonce_file))
    child_env = os.environ.copy(); child_env["AXON_E2E_PROCESS_NONCE"] = nonce
    proc = subprocess.Popen([sys.executable, str(fixture)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True,
                            start_new_session=os.name != "nt", creationflags=flags, env=child_env)
    assert proc.stdin and proc.stdout
    ensure(json.loads(proc.stdout.readline()) == {"ready": True}, "tree fixture did not reach ready state")
    start_time = isolation._process_start_time(proc.pid)
    manifest.register("process", str(proc.pid), {"start_time":start_time,"nonce":nonce,
        "nonce_file":str(nonce_file),"process_group":proc.pid,"argv0":Path(sys.executable).name})
    method = "posix-process-group"
    job = None
    if os.name == "nt":
        kernel = ctypes.windll.kernel32
        kernel.CreateJobObjectW.argtypes = [ctypes.c_void_p, ctypes.c_wchar_p]
        kernel.CreateJobObjectW.restype = ctypes.c_void_p
        kernel.SetInformationJobObject.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_void_p, ctypes.c_uint32]
        kernel.SetInformationJobObject.restype = ctypes.c_int
        kernel.AssignProcessToJobObject.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
        kernel.AssignProcessToJobObject.restype = ctypes.c_int
        kernel.CloseHandle.argtypes = [ctypes.c_void_p]
        kernel.CloseHandle.restype = ctypes.c_int
        job = kernel.CreateJobObjectW(None, None); ensure(bool(job), "CreateJobObjectW failed")
        info = _JobExtendedLimit(); info.basic.limit_flags = 0x00002000  # JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        ensure(bool(kernel.SetInformationJobObject(job, 9, ctypes.byref(info), ctypes.sizeof(info))), "job limit failed")
        ensure(bool(kernel.AssignProcessToJobObject(job, ctypes.c_void_p(int(proc._handle)))), "AssignProcessToJobObject failed")
        method = "windows-job-object"
    proc.stdin.write("spawn\n"); proc.stdin.flush()
    descendant = int(json.loads(proc.stdout.readline())["child_pid"])
    if os.name == "nt": kernel.CloseHandle(job)
    else: os.killpg(proc.pid, signal.SIGTERM)
    proc.wait(timeout=5)
    deadline = time.monotonic() + 5
    while _pid_alive(descendant) and time.monotonic() < deadline: time.sleep(0.05)
    ensure(not _pid_alive(descendant), "owned descendant survived tree teardown")
    return {"method": method, "equivalent_child_termination": True}


def run(args) -> int:
    started = time.monotonic(); selected = selected_catalog()
    owned_base = (args.root_base or (ROOT / "target/e2e")).resolve()
    run_root = None; binary=args.binary.resolve(); manifest = None; cleanup = None; error = None
    results = {}; catalog_records = []; corpus = {"corpus_version": "unknown", "corpus_checksum": "unknown"}
    previous_handlers = {}

    def interrupted(signum, _frame):
        raise InterruptedError(f"received signal {signum}")

    for signum in (signal.SIGINT, signal.SIGTERM):
        previous_handlers[signum] = signal.signal(signum, interrupted)
    try:
        if os.name == "nt" and not binary.is_file() and binary.suffix.lower() != ".exe":
            binary=binary.with_suffix(".exe")
        ensure(binary.is_file(),f"Axon binary not found: {binary}")
        run_id = isolation.new_run_id(); run_root = owned_base / "runs" / run_id
        data = run_root / "data"; manifests = owned_base / "manifests"
        if args.run_root_record: args.run_root_record.write_text(str(run_root), encoding="utf-8")
        data.mkdir(parents=True,mode=0o700)
        if os.name == "nt":isolation._windows_acl(data,apply=True)
        manifest = isolation.Manifest.create(manifests, run_id, data)
        register_outer_cleanup(manifest)
        manifest.register("temp_path",str(run_root)); manifest.register("data_dir", str(data)); db = data / "jobs.db"; manifest.register("sqlite", str(db))
        corpus = json.loads((ROOT / "tests/e2e/corpus/manifest.json").read_text(encoding="utf-8"))
        ensure(corpus.get("schema_version") == 1 and corpus.get("corpus_version") == "1.0.0", "canonical corpus changed")
        if os.name == "nt":
            isolation.Manifest.open(manifest.path)._key(); permissions = "owner-dacl-verified"
        else:
            ensure((manifest.key_path.stat().st_mode & 0o077) == 0, "manifest key is not private"); permissions = "mode-0600-verified"
        results["permissions"] = permissions
        if args.failure_at == "after_setup": raise CheckError("injected failure after setup")
        if args.test_hold_seconds: time.sleep(args.test_hold_seconds)
        env=axon_env(data)
        catalog_records=execute_catalog_scenarios(binary,env,selected)
        mcp_data=data/"mcp";mcp_data.mkdir()
        results["stdio_mcp"] = stdio_mcp(binary,axon_env(mcp_data),manifest,run_root)
        results["paths"] = executable_and_paths(binary,data,env)
        results["config"] = config_roots(binary,data,env)
        results["sqlite"] = sqlite_lock(db)
        results["port"] = port_lease(owned_base / "leases", run_id, manifest)
        results["process_tree"] = terminate_tree(manifest, run_root)
    except BaseException as caught:
        error = f"{type(caught).__name__}: {caught}"
    finally:
        for signum in previous_handlers: signal.signal(signum, signal.SIG_IGN)
        try:
            if manifest is not None:
                cleanup = teardown.Engine(manifest.path).run().json()
            else:
                if run_root is not None: shutil.rmtree(run_root, ignore_errors=True)
                cleanup = {"success": True, "residual": [], "refused": [], "created": [], "removed": []}
        except BaseException as caught:
            cleanup = {"success": False, "residual": [{"class":"teardown","reason":str(caught)}],
                       "refused": [{"class":"teardown","reason":str(caught)}]}
            error = error or f"teardown failed: {caught}"
        finally:
            # Signed manifests remain as durable completion/discovery records;
            # canonical teardown removes only the run-owned state tree.
            if run_root is not None and run_root.exists():
                cleanup.setdefault("residual", []).append({"class":"run_root","reason":"bootstrap root remains"})
                cleanup["success"] = False
            for signum, handler in previous_handlers.items(): signal.signal(signum, handler)
    scenario = reporting.Scenario("platform.portable.contract", "hermetic", "platform", "native")
    clean = cleanup.get("success") is True and not cleanup.get("residual") and not cleanup.get("refused")
    if error is None and clean: scenario.attempt("passed", int((time.monotonic()-started)*1000))
    else: scenario.attempt("failed", int((time.monotonic()-started)*1000), classification="cleanup" if not clean else "harness", summary=error or "residual audit failed")
    scenario.cleanup = cleanup
    for item in catalog_records:item.cleanup=cleanup
    scenario.invariants = [{"catalog_tag": "platform_smoke", "selected": selected}, {"checks": results}]
    unsupported = ([{"capability":"posix-modes-signals","rationale":"Windows uses owner DACLs and a kill-on-close Job Object"}]
                   if os.name == "nt" else [{"capability":"windows-job-object","rationale":"POSIX uses a fresh process group and TERM"}])
    tested_sha = args.tested_sha or os.environ.get("GITHUB_SHA") or "0" * 40
    report = reporting.suite_report([*catalog_records,scenario], tested_sha=tested_sha, provider_versions={"python":sys.version.split()[0],"axon":"7.2.23"},
        policy={"platform":sys.platform,"catalog_tag":"platform_smoke","budget_seconds":120,"secrets":"forbidden",
                "corpus_manifest":"tests/e2e/corpus/manifest.json","corpus_version":corpus["corpus_version"],
                "corpus_checksum":corpus["corpus_checksum"],"unsupported":unsupported},
        upload={"status":"not_attempted","local_evidence_path":str(args.report)})
    reporting.write_json(report, args.report)
    print(json.dumps({"status":report["summary"]["status"],"platform":sys.platform,"report":str(args.report)}))
    return 0 if report["summary"]["status"] == "passed" else 2


def main() -> int:
    parser=argparse.ArgumentParser(); parser.add_argument("--report",type=Path,required=True); parser.add_argument("--tested-sha")
    parser.add_argument("--binary",type=Path,default=ROOT/"target/debug/axon")
    parser.add_argument("--failure-at", choices=("after_setup",), help=argparse.SUPPRESS)
    parser.add_argument("--test-hold-seconds", type=float, default=0, help=argparse.SUPPRESS)
    parser.add_argument("--root-base", type=Path, help=argparse.SUPPRESS)
    parser.add_argument("--run-root-record", type=Path, help=argparse.SUPPRESS)
    return run(parser.parse_args())


if __name__ == "__main__": raise SystemExit(main())
