#!/usr/bin/env python3
"""Run the measured, network-denied Axon E2E vertical slice."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import resource
import re
import signal
import shutil
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path
from urllib.parse import urlparse

ROOT = Path(__file__).resolve().parents[2]
REQUIRED_ENV = {
    "AXON_E2E_HERMETIC": "1",
    "AXON_E2E_LIVE": "0",
    "AXON_E2E_PROVIDER_MODE": "double",
    "AXON_E2E_STAGE_GATES": "1",
    "AXON_E2E_NETWORK_POLICY": "loopback-only",
    "AXON_E2E_NATIVE_ISOLATION": "1",
}
# The total is a real wall-clock deadline. It is deliberately greater than the
# current sum of stage caps; individual caps remain useful regression gates.
DEFAULT_BUDGET_SECONDS = 2400
RESOURCE_BUDGETS = {"cpu_seconds": 220, "memory_mib": 4096, "processes": 128, "ports": 64,
                    "shards": 16, "retries": 32, "artifacts": 256}


def validate_environment() -> None:
    wrong = {key: (os.environ.get(key), value) for key, value in REQUIRED_ENV.items()
             if os.environ.get(key) != value}
    if wrong:
        raise RuntimeError(f"mandatory hermetic environment is absent or invalid: {wrong}")
    for key, value in os.environ.items():
        if not (key.endswith("_URL") or key.endswith("_ENDPOINT")) or not value:
            continue
        parsed = urlparse(value)
        if parsed.scheme in {"http", "https"} and parsed.hostname not in {"127.0.0.1", "::1", "localhost"}:
            raise RuntimeError(f"public provider route is forbidden in hermetic mode: {key}")


def verify_native_isolation() -> None:
    """Prove a non-Python process cannot open a public TCP route."""
    try:
        probe=subprocess.run(["bash","-c","exec 3<>/dev/tcp/1.1.1.1/80"],capture_output=True,timeout=2,check=False)
    except subprocess.TimeoutExpired as error:
        raise RuntimeError("native egress probe did not fail closed") from error
    if probe.returncode == 0: raise RuntimeError("native public egress is reachable")


def commands() -> list[tuple[str, list[str], int]]:
    python = sys.executable
    result = [
        ("build-axon", ["cargo", "build", "--locked", "--bin", "axon"], 900),
        ("real-axon", [str(ROOT/"target/debug/axon"), "--version"], 20),
        ("upgrade", [python, "scripts/e2e/run-upgrade.py", "--binary", str(ROOT/"target/debug/axon")], 90),
        ("catalog", [python, "scripts/e2e/validate-catalog.py", "--report"], 20),
        ("mutation-sensitivity", [python, "scripts/e2e/run-mutations.py", "--subset", "representative"], 30),
        ("real-composed-retrieval", [python, "tests/e2e/hermetic/real_composed.py"], 120),
    ]
    # Stable discovery contract: later domain beads register by adding a
    # scenario directory with test_*.py; workflow YAML never needs editing.
    for domain in sorted((ROOT/"tests/e2e/scenarios").iterdir()):
        if domain.is_dir() and any(domain.glob("test_*.py")):
            result.append((f"scenario-{domain.name}", [python, "-m", "unittest", "discover", "-s",
                           str(domain.relative_to(ROOT)), "-p", "test_*.py"], 90))
    result.extend([
        ("adapter-cli", [python, "tests/e2e/cli_adapter_tests.py"], 30),
        ("adapter-http", [python, "-m", "unittest", "discover", "-s", "tests/e2e/http", "-p", "test_*.py"], 30),
        ("adapter-mcp", [python, "-m", "unittest", "discover", "-s", "tests/e2e/mcp", "-p", "test_*.py"], 30),
        ("teardown", [python, "-m", "unittest", "discover", "-s", "tests/e2e/teardown", "-p", "test_*.py"], 90),
        ("isolation", [python, "tests/e2e/run_isolation_tests.py"], 30),
    ])
    return result


def run(report_path: Path, total_budget: int) -> int:
    started = time.monotonic(); stages = []; failed = False
    # Freeze discovery once. Other wave workers may add a domain concurrently;
    # recomputing at report time would make a successful run self-inconsistent.
    planned=commands()
    canceled=False;current: subprocess.Popen[str] | None=None
    measured={"processes":0,"ports":0,"shards":0,"retries":0,"artifacts":0}
    monitor_samples=0;monitor_errors=[];owned_pids:set[int]=set();owned_ports:set[int]=set();monitor_lock=threading.Lock()
    def descendants(root_pid: int) -> set[int]:
        if sys.platform != "darwin":
            rows=subprocess.run(["ps","-axo","pid=,ppid="],capture_output=True,text=True,timeout=1,check=True).stdout.splitlines()
            pairs=[tuple(map(int,row.split())) for row in rows]
            owned={root_pid};changed=True
            while changed:
                changed=False
                for pid,ppid in pairs:
                    if ppid in owned and pid not in owned:owned.add(pid);changed=True
            return owned
        import ctypes
        libproc=ctypes.CDLL("/usr/lib/libproc.dylib",use_errno=True);owned={root_pid};pending=[root_pid]
        while pending:
            parent=pending.pop();capacity=256;values=(ctypes.c_int*capacity)()
            count=libproc.proc_listchildpids(parent,values,ctypes.sizeof(values))
            if count < 0:raise OSError(ctypes.get_errno(),"proc_listchildpids")
            # proc_listchildpids returns the number of PIDs written, unlike
            # proc_pidinfo which returns a byte count.
            for pid in values[:count]:
                if pid > 0 and pid not in owned:owned.add(pid);pending.append(pid)
        return owned
    def sample_processes(root_pid: int, stop_event: threading.Event) -> None:
        nonlocal monitor_samples
        while True:
            try:
                owned=descendants(root_pid)
                with monitor_lock:
                    monitor_samples+=1;owned_pids.update(owned)
                    measured["processes"]=max(measured["processes"],len(owned))
                if shutil.which("lsof"):
                    ports=subprocess.run(["lsof","-nP","-a","-p",",".join(map(str,owned)),"-iTCP","-sTCP:LISTEN"],capture_output=True,text=True,timeout=1).stdout.splitlines()
                    discovered={int(match.group(1)) for line in ports[1:] if (match:=re.search(r":(\d+)\s+\(LISTEN\)$",line))}
                    with monitor_lock:
                        owned_ports.update(discovered);measured["ports"]=max(measured["ports"],len(discovered))
            except (OSError,subprocess.SubprocessError,ValueError) as error:
                monitor_errors.append(type(error).__name__)
            if stop_event.wait(.05):
                return
    previous={}
    def cancel(signum,_frame):
        nonlocal canceled
        canceled=True
        if current is not None and current.poll() is None:
            if os.name == "nt":current.terminate()
            else:os.killpg(current.pid,signal.SIGTERM)
    for sig in (signal.SIGINT,signal.SIGTERM):previous[sig]=signal.signal(sig,cancel)
    guard = tempfile.TemporaryDirectory(prefix="axon-e2e-network-guard-")
    guard_path = Path(guard.name)
    (guard_path/"sitecustomize.py").write_text(
        "import socket\n"
        "_connect=socket.socket.connect\n"
        "def _guard(self,address):\n"
        " if isinstance(address,str): return _connect(self,address)\n"
        " host=address[0]\n"
        " if host not in ('127.0.0.1','::1','localhost'): raise PermissionError('hermetic public network denied')\n"
        " return _connect(self,address)\n"
        "socket.socket.connect=_guard\n"
        "_gai=socket.getaddrinfo\n"
        "def _guard_gai(host,*args,**kwargs):\n"
        " if host not in (None,'127.0.0.1','::1','localhost'): raise PermissionError('hermetic DNS denied')\n"
        " return _gai(host,*args,**kwargs)\n"
        "socket.getaddrinfo=_guard_gai\n")
    child_env = {**os.environ, "PYTHONPATH": str(guard_path) + os.pathsep + os.environ.get("PYTHONPATH", ""),
                 "AXON_E2E_REAL_AXON_BIN":str(ROOT/"target/debug/axon"),"AXON_E2E_REQUIRE_REAL_SOURCE_JOBS":"1"}
    budget_exhausted=False; pending_error: Exception | None = None
    try:
        cleanup_names={"teardown","isolation"}
        cleanup_reserve=sum(budget for name,_argv,budget in planned if name in cleanup_names)
        if total_budget < cleanup_reserve:
            raise ValueError(f"total budget must cover cleanup reserve ({cleanup_reserve}s)")
        try:
            validate_environment()
        except Exception as error:
            failed=True;pending_error=error
            stages.append({"name":"environment","status":"failed","duration_ms":0,
                           "error_type":type(error).__name__,"sanitized":True})
        if pending_error is None:
            try:
                verify_native_isolation()
            except Exception as error:
                failed=True;pending_error=error
                stages.append({"name":"native-isolation","status":"failed","duration_ms":0,
                               "error_type":type(error).__name__,"sanitized":True})
        for name, argv, budget in planned:
            if (failed or canceled) and name not in cleanup_names: continue
            remaining=max(0.0,total_budget-(time.monotonic()-started))
            reserve_after=sum(stage_budget for stage_name,_stage_argv,stage_budget in planned
                              if stage_name in cleanup_names and stage_name != name and
                              not any(done["name"] == stage_name for done in stages))
            allowed=min(float(budget),max(0.0,remaining-reserve_after))
            if allowed <= 0:
                failed=True;budget_exhausted=True
                if name not in cleanup_names:continue
                # A cleanup stage is still attempted with the remaining wall
                # clock. Its failure is represented by that known stage.
                allowed=max(0.001,remaining)
            stage_started = time.monotonic()
            print(f"hermetic stage {name} (budget {budget}s, remaining allowance {allowed:.3f}s)", flush=True)
            try:
                current=subprocess.Popen(argv,cwd=ROOT,text=True,stdout=subprocess.PIPE,stderr=subprocess.PIPE,
                                         shell=False,env={**child_env,"AXON_E2E_ACTIVE_STAGE":name},start_new_session=True)
                stage_monitor_stop=threading.Event()
                monitor=threading.Thread(target=sample_processes,args=(current.pid,stage_monitor_stop),daemon=True);monitor.start()
                try:
                    stdout,stderr=current.communicate(timeout=allowed);returncode=current.returncode
                finally:
                    stage_monitor_stop.set();monitor.join(timeout=2)
                for line in stdout.splitlines():
                    try:value=json.loads(line)
                    except json.JSONDecodeError:continue
                    observation=value.get("provider_observation",{}) if isinstance(value,dict) else {}
                    if isinstance(observation.get("retries"),int):measured["retries"]+=observation["retries"]
                status = "passed" if returncode == 0 else "failed"; failed |= returncode != 0
                stages.append({"name": name, "status": status, "budget_seconds": budget,
                               "duration_ms": int((time.monotonic()-stage_started)*1000),
                               "returncode": returncode,"stdout_sha256":hashlib.sha256(stdout.encode()).hexdigest(),
                               "stderr_sha256":hashlib.sha256(stderr.encode()).hexdigest(),"sanitized":True})
                print(f"hermetic stage {name}: {status}", flush=True)
            except subprocess.TimeoutExpired as error:
                if os.name == "nt":current.terminate()
                else:os.killpg(current.pid,signal.SIGTERM)
                try:stdout,stderr=current.communicate(timeout=3)
                except subprocess.TimeoutExpired:
                    if os.name == "nt":current.kill()
                    else:os.killpg(current.pid,signal.SIGKILL)
                    stdout,stderr=current.communicate()
                failed = True; budget_exhausted |= allowed < budget
                stages.append({"name": name, "status": "timed_out", "budget_seconds": budget,
                                              "duration_ms": int((time.monotonic()-stage_started)*1000),
                                              "returncode": 124,"error_sha256":hashlib.sha256(str(error).encode()).hexdigest(),"sanitized":True})
            if time.monotonic() - started >= total_budget:
                failed=True;budget_exhausted=True
    finally:
        usage=resource.getrusage(resource.RUSAGE_CHILDREN)
        # Each stage record is one sanitized evidence artifact; scenario stages
        # are the actual discovery shards executed by this frozen run plan.
        measured["artifacts"]=len(stages)+1  # plus the enclosing report
        measured["shards"]=sum(stage.get("name","").startswith("scenario-") for stage in stages)
        observed={"cpu_seconds":round(usage.ru_utime+usage.ru_stime,3),
                  "memory_mib":round(usage.ru_maxrss/(1024 if sys.platform!="darwin" else 1024*1024),2),
                  **measured}
        budget_ok=monitor_samples > 0 and not monitor_errors and all(observed[key] <= limit for key,limit in RESOURCE_BUDGETS.items())
        cleanup={name:next((stage for stage in stages if stage["name"]==name),None) for name in ("teardown","isolation")}
        residual=[]
        for pid in sorted(owned_pids):
            try:os.kill(pid,0)
            except ProcessLookupError:continue
            residual.append({"class":"process","opaque_id":hashlib.sha256(f"process\0{pid}".encode()).hexdigest()[:20]})
        for port in sorted(owned_ports):
            import socket
            with socket.socket() as probe:
                if probe.connect_ex(("127.0.0.1",port))==0:
                    residual.append({"class":"port","opaque_id":hashlib.sha256(f"port\0{port}".encode()).hexdigest()[:20]})
        cleanup_ok=all(value and value["status"]=="passed" for value in cleanup.values()) and not residual
        report = {"schema": 1, "mode": "hermetic", "required": True,
                  "network_policy": "loopback-only", "provider_mode": "double",
                  "stage_gates": True, "total_budget_seconds": total_budget,
                  "duration_ms": int((time.monotonic()-started)*1000), "stages": stages,
                  "resource_budgets":RESOURCE_BUDGETS,"resource_observed":observed,
                  "cleanup":cleanup,"evidence":{"sanitized":True,"artifact_count":1},"canceled":canceled,
                  "budget_exhausted":budget_exhausted,
                  "cleanup_contract": "teardown-stages-plus-run-wide-residual-audit",
                  "residual_audit":{"success":not residual,"residual":residual,
                                    "owned_processes_observed":len(owned_pids),"owned_ports_observed":len(owned_ports)},
                  "measurement":{"process_samples":monitor_samples,"errors":monitor_errors},
                  "expected_stages":[name for name,_argv,_budget in planned],
                  "success": not failed and not canceled and budget_ok and cleanup_ok and len(stages) == len(planned)}
        report_path.parent.mkdir(parents=True, exist_ok=True)
        report_path.write_text(json.dumps(report, indent=2, sort_keys=True)+"\n")
        guard.cleanup()
        for sig,handler in previous.items():signal.signal(sig,handler)
    if pending_error is not None:
        raise pending_error
    return 0 if report["success"] else 1


def main() -> int:
    parser=argparse.ArgumentParser();parser.add_argument("--report",type=Path,default=ROOT/"target/e2e/hermetic-report.json")
    parser.add_argument("--total-budget-seconds",type=int,default=DEFAULT_BUDGET_SECONDS);args=parser.parse_args()
    if args.total_budget_seconds < 1: raise SystemExit("total budget must be positive")
    return run(args.report,args.total_budget_seconds)


if __name__ == "__main__": raise SystemExit(main())
