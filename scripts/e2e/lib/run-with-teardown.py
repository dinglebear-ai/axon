#!/usr/bin/env python3
"""Run one E2E child and unconditionally execute the authoritative teardown."""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
import signal
import subprocess
import sys
import time
from pathlib import Path


def _load():
    path = Path(__file__).with_name("teardown.py")
    spec = importlib.util.spec_from_file_location("axon_e2e_supervised_teardown", path)
    if spec is None or spec.loader is None: raise RuntimeError("teardown module unavailable")
    module = importlib.util.module_from_spec(spec); sys.modules[spec.name] = module; spec.loader.exec_module(module)
    return module


teardown = _load()


def _load_reporting():
    path = Path(__file__).with_name("reporting.py")
    spec = importlib.util.spec_from_file_location("axon_e2e_supervised_reporting", path)
    if spec is None or spec.loader is None: raise RuntimeError("reporting module unavailable")
    module = importlib.util.module_from_spec(spec); sys.modules[spec.name] = module; spec.loader.exec_module(module); return module


reporting = _load_reporting()


def _load_observability():
    path = Path(__file__).with_name("observability-assertions.py")
    spec = importlib.util.spec_from_file_location("axon_e2e_supervised_observability", path)
    if spec is None or spec.loader is None: raise RuntimeError("observability oracle unavailable")
    module = importlib.util.module_from_spec(spec); sys.modules[spec.name] = module; spec.loader.exec_module(module); return module


observability = _load_observability()


def _terminate_and_reap(child: subprocess.Popen[bytes], *, grace: float = 2) -> None:
    """Best-effort process-group shutdown that always reaps and closes pipes."""
    if child.poll() is None:
        try:
            if os.name == "nt": child.terminate()
            else: os.killpg(child.pid, signal.SIGTERM)
        except OSError:
            pass
        try:
            child.communicate(timeout=grace)
        except subprocess.TimeoutExpired:
            try:
                if os.name == "nt": child.kill()
                else: os.killpg(child.pid, signal.SIGKILL)
            except OSError:
                pass
            try: child.communicate(timeout=grace)
            except subprocess.TimeoutExpired: child.wait(timeout=grace)
    else:
        child.wait()
    for pipe in (child.stdout, child.stderr, child.stdin):
        if pipe is not None and not pipe.closed:
            pipe.close()


def _supervise_once(manifest: Path, command: list[str], *, timeout: float, provider_config: Path | None = None,
              qdrant_url: str | None = None, observability_capture: Path | None = None,
              observability_db: Path | None = None) -> dict:
    if not command: raise ValueError("a child command is required")
    secret_keys = ("TOKEN", "PASSWORD", "SECRET", "API_KEY", "PRIVATE_KEY")
    secrets = tuple(value for key, value in os.environ.items() if value and any(part in key.upper() for part in secret_keys))
    reporting.redaction.validate_command(command, secrets)
    if os.environ.get("GITHUB_ACTIONS") == "true":
        masker = reporting.redaction.CredentialMasker(sys.stdout)
        for secret in secrets: masker.acquire(secret)
    # Ownership is provisioned and read back before the child can issue its
    # first query/upsert. The caller must create the empty isolated collection
    # before entering this supervisor; an absent collection fails setup closed.
    header, resources = teardown.manifest_api.load(manifest); provisioning = []
    if qdrant_url:
        qdrant = teardown.provider_api.QdrantAdapter({"base_url": qdrant_url}).bind(header, teardown.manifest_api)
        qdrant_types = {"collection", "qdrant_alias", "qdrant_snapshot", "point", "payload_index"}
        provisioning = [qdrant.provision_ownership_marker(item) for item in resources if item.resource_type in qdrant_types]
        if not provisioning: raise RuntimeError("no Qdrant collection ownership marker was provisioned")
    started = time.monotonic(); child: subprocess.Popen[bytes] | None = None
    interrupted: list[int] = []
    cleanup_in_progress = False
    previous = {}

    def stop(signum, _frame):
        interrupted.append(signum)
        # Once cleanup begins the supervisor owns termination. A repeated
        # cancellation must not interrupt manifest teardown or report writing.
        if cleanup_in_progress:
            return
        if child is None:
            return
        try:
            if os.name == "nt": child.terminate()
            else: os.killpg(child.pid, signal.SIGTERM)
        # The child can exit between signal delivery and process-group lookup.
        # macOS reports that boundary as EPERM as well as ESRCH; neither may
        # escape a Python signal handler and bypass child reaping.
        except OSError: pass

    for sig in (signal.SIGINT, signal.SIGTERM):
        previous[sig] = signal.signal(sig, stop)
    try:
        child = subprocess.Popen(command, start_new_session=(os.name != "nt"),
                                 stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    except BaseException:
        for sig, handler in previous.items(): signal.signal(sig, handler)
        raise
    timed_out = False
    stdout = stderr = b""
    try:
        try: stdout, stderr = child.communicate(timeout=timeout); returncode = child.returncode
        except subprocess.TimeoutExpired:
            timed_out = True; stop(signal.SIGTERM, None)
            try: stdout, stderr = child.communicate(timeout=2); returncode = child.returncode
            except subprocess.TimeoutExpired:
                if os.name == "nt": child.kill()
                else: os.killpg(child.pid, signal.SIGKILL)
                stdout, stderr = child.communicate(timeout=2); returncode = child.returncode
        cleanup_in_progress = True
        observe_outcomes = None; observe_error = None
        if observability_capture is not None or observability_db is not None:
            try:
                if observability_capture is None or observability_db is None:
                    raise observability.ObservabilityFailure("capture and SQLite path must be provided together")
                capture = json.loads(observability_capture.read_text())
                job_ids = {item.get("job_id") for item in capture.get("executions", [])}
                if capture.get("observation_mode") != "multi_observer" or len(job_ids) != 1 or None in job_ids:
                    raise observability.ObservabilityFailure("supervised runtime capture must observe exactly one job")
                runtime = observability.load_runtime(observability_db, job_ids.pop(), tuple(capture.get("owned_provider_ids", [])))
                observe_outcomes = observability.evaluate(capture, runtime)
            except (OSError, json.JSONDecodeError, observability.ObservabilityFailure) as error:
                observe_error = str(error)
        # Re-open only after the child exits so resources registered at every setup
        # stage are included, including a final append immediately before a crash.
        header, _ = teardown.manifest_api.load(manifest)
        adapters = teardown.provider_api.build(provider_config, header, teardown.manifest_api) if provider_config else None
        cleanup = teardown.Engine(manifest, adapters).run().json()
        log_error = None
        try:
            prepared = []
            for value, sink in ((stdout, sys.stdout), (stderr, sys.stderr)):
                if b"\x00" in value: raise reporting.redaction.RedactionError("binary child log is forbidden")
                try: decoded = value.decode("utf-8")
                except UnicodeDecodeError as error: raise reporting.redaction.RedactionError("non-UTF-8 child log is forbidden") from error
                sanitized, _ = reporting.redaction.sanitize_text(decoded, secrets); prepared.append((sanitized, sink))
            for value, sink in prepared: sink.write(value); sink.flush()
        except reporting.redaction.RedactionError as error:
            log_error = str(error)
        return {"success": cleanup["success"] and returncode == 0 and not interrupted and not timed_out and log_error is None and observe_error is None,
                "child_returncode": returncode, "signal": interrupted[-1] if interrupted else None,
                "timed_out": timed_out, "duration_ms": int((time.monotonic() - started) * 1000),
                "qdrant_ownership": provisioning, "log_redaction_error": log_error,
                "observability_error": observe_error, "observability": observe_outcomes, "cleanup": cleanup}
    finally:
        if child is not None:
            _terminate_and_reap(child)
        for sig, handler in previous.items(): signal.signal(sig, handler)


def supervise(manifest: Path, command: list[str], *, timeout: float, provider_config: Path | None = None,
              qdrant_url: str | None = None, observability_capture: Path | None = None,
              observability_db: Path | None = None) -> dict:
    """Run a child with canonical teardown covering every post-validation exit."""
    if not command:
        raise ValueError("a child command is required")
    secret_keys = ("TOKEN", "PASSWORD", "SECRET", "API_KEY", "PRIVATE_KEY")
    secrets = tuple(value for key, value in os.environ.items()
                    if value and any(part in key.upper() for part in secret_keys))
    # Command-policy failures happen before resource ownership is accepted and
    # remain programmer errors. Every later failure is converted to a cleanup-
    # bearing result so callers can never accidentally omit teardown evidence.
    reporting.redaction.validate_command(command, secrets)
    try:
        return _supervise_once(manifest, command, timeout=timeout, provider_config=provider_config,
                               qdrant_url=qdrant_url, observability_capture=observability_capture,
                               observability_db=observability_db)
    except Exception as primary:
        cleanup_error = None
        try:
            header, _ = teardown.manifest_api.load(manifest)
            adapters = teardown.provider_api.build(provider_config, header, teardown.manifest_api) if provider_config else None
            cleanup = teardown.Engine(manifest, adapters).run().json()
        except Exception as error:
            cleanup_error = f"{type(error).__name__}: {error}"
            cleanup = {"success": False, "refused": [{"class": "cleanup", "reason": cleanup_error}],
                       "residual": [{"class": "cleanup", "identity": "unknown-after-setup-failure"}]}
        return {"success": False, "child_returncode": None, "signal": None, "timed_out": False,
                "duration_ms": 0, "qdrant_ownership": [], "log_redaction_error": None,
                "observability_error": None, "observability": None, "cleanup": cleanup,
                "fatal": f"{type(primary).__name__}: {primary}", "cleanup_error": cleanup_error}


def canonical_scenario(result: dict, *, scenario_id: str, tier: str, capability: str, surface: str,
                       failure_class: str = "product"):
    scenario = reporting.Scenario(scenario_id, tier, capability, surface)
    if result.get("success"): status, classification, summary = "passed", None, None
    elif not result.get("cleanup", {}).get("success"): status, classification, summary = "failed", "cleanup", "authoritative teardown or residual audit failed"
    elif result.get("log_redaction_error"): status, classification, summary = "failed", "harness", "log redaction failed closed"
    elif result.get("observability_error"): status, classification, summary = "failed", "product", "critical lifecycle observability contract failed"
    elif result.get("timed_out"): status, classification, summary = "timed_out", "harness", "scenario exceeded its declared timeout"
    elif result.get("signal"): status, classification, summary = "canceled", "harness", "scenario was canceled"
    else: status, classification, summary = "failed", failure_class, f'child exited {result.get("child_returncode")}'
    scenario.attempt(status, int(result.get("duration_ms", 0)), classification=classification, summary=summary)
    scenario.cleanup = result.get("cleanup", {"success": False, "residual": [{"class": "cleanup", "identity": "missing"}]})
    scenario.invariants.extend(result.get("observability") or [])
    for item in scenario.cleanup.get("retained", []):
        path = Path(str(item.get("path", "sanitized-evidence")))
        if not path.is_file() or path.is_symlink(): raise reporting.ReportingError("retained evidence is missing or not regular")
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        if digest != item.get("sha256"): raise reporting.ReportingError("retained evidence checksum mismatch")
        reference = reporting.evidence_ref(path, path.parent); reference["kind"] = item.get("class", "evidence")
        scenario.evidence.append(reference)
    return scenario


def result_outcome(result: dict, failure_class: str = "product") -> tuple[str, str | None, str | None]:
    """Map one real supervised child result to the canonical attempt taxonomy."""
    if result.get("success"): return "passed", None, None
    if not result.get("cleanup", {}).get("success"): return "failed", "cleanup", "authoritative teardown or residual audit failed"
    if result.get("log_redaction_error"): return "failed", "harness", "log redaction failed closed"
    if result.get("observability_error"): return "failed", "product", "critical lifecycle observability contract failed"
    if result.get("timed_out"): return "timed_out", "harness", "scenario exceeded its declared timeout"
    if result.get("signal"): return "canceled", "harness", "scenario was canceled"
    return "failed", failure_class, f"child exited {result.get('child_returncode')}"


def supervise_suite(entries: list[dict], *, tested_sha: str, provider_versions: dict[str, str], policy: dict) -> dict:
    scenarios = [];declared_budget=policy.get("suite_retry_budget",0);remaining=declared_budget;retry_ordinal=0
    for entry in entries:  # Intentionally continue after independent failures.
        diagnostic=entry.get("diagnostic_retry")
        if entry.get("tier")=="live" and diagnostic is not None:
            governance_path=Path(__file__).with_name("flake-governance.py")
            spec=importlib.util.spec_from_file_location("axon_e2e_supervisor_governance",governance_path)
            if spec is None or spec.loader is None:raise RuntimeError("flake governance unavailable")
            governance=importlib.util.module_from_spec(spec);spec.loader.exec_module(governance)
            scenario=reporting.Scenario(entry["scenario_id"],entry["tier"],entry["capability"],entry["surface"]);results={}
            retry_ordinal+=1
            def invoke(namespace):
                attempt_entry=entry if not results else diagnostic
                header,_=teardown.manifest_api.load(Path(attempt_entry["manifest"]))
                if header.run_id!=namespace:raise RuntimeError("live attempt manifest namespace disagrees with governed namespace")
                result=supervise(Path(attempt_entry["manifest"]),list(attempt_entry["command"]),timeout=float(attempt_entry.get("timeout",900)),
                    provider_config=Path(attempt_entry["provider_config"]) if attempt_entry.get("provider_config") else None,qdrant_url=attempt_entry.get("qdrant_url"))
                results[namespace]=result
                return result_outcome(result,entry.get("failure_class","provider"))
            try:
                retry_policy=governance.run_live_diagnostic(scenario=scenario,lifecycle=entry.get("lifecycle","source"),
                    retry_class=entry.get("retry_class","never"),budget_remaining=remaining,seed=policy.get("retry_seed",tested_sha),
                    invoke=invoke,verify_teardown=lambda namespace:results[namespace].get("cleanup",{}).get("success") is True,
                    suite_budget_declared=declared_budget,retry_ordinal=retry_ordinal)
                if retry_policy is None:retry_ordinal-=1
                else:remaining-=1
                last=next(reversed(results.values()));scenario.cleanup=last.get("cleanup",{"success":False,"residual":[{"class":"cleanup","identity":entry["scenario_id"]}]})
            except Exception as error:
                scenario.attempt("failed",0,classification="harness",summary=f"scenario setup failed: {type(error).__name__}")
                scenario.cleanup={"success":False,"residual":[{"class":"setup","identity":entry["scenario_id"]}]}
            scenarios.append(scenario);continue
        try:
            result = supervise(Path(entry["manifest"]), list(entry["command"]), timeout=float(entry.get("timeout", 900)),
                               provider_config=Path(entry["provider_config"]) if entry.get("provider_config") else None,
                               qdrant_url=entry.get("qdrant_url"),
                               observability_capture=Path(entry["observability_capture"]) if entry.get("observability_capture") else None,
                               observability_db=Path(entry["observability_db"]) if entry.get("observability_db") else None)
            scenario = canonical_scenario(result, scenario_id=entry["scenario_id"], tier=entry["tier"],
                                            capability=entry["capability"], surface=entry["surface"],
                                            failure_class=entry.get("failure_class", "product"))
        except Exception as error:
            scenario = reporting.Scenario(entry["scenario_id"], entry["tier"], entry["capability"], entry["surface"])
            classification = entry.get("setup_failure_class", "harness")
            scenario.attempt("failed", 0, classification=classification, summary=f"scenario setup failed: {type(error).__name__}")
            scenario.cleanup = {"success": False, "residual": [{"class": "setup", "identity": entry["scenario_id"]}]}
        scenarios.append(scenario)
    return reporting.suite_report(scenarios, tested_sha=tested_sha, provider_versions=provider_versions, policy=policy)


def main() -> int:
    parser = argparse.ArgumentParser(); parser.add_argument("manifest", type=Path); parser.add_argument("--report", type=Path, required=True)
    parser.add_argument("--provider-config", type=Path); parser.add_argument("--timeout", type=float, default=900)
    parser.add_argument("--qdrant-url"); parser.add_argument("--scenario-id"); parser.add_argument("--tier", default="hermetic")
    parser.add_argument("--observability-capture", type=Path); parser.add_argument("--observability-db", type=Path)
    parser.add_argument("--capability", default="unknown"); parser.add_argument("--surface", default="harness")
    parser.add_argument("--failure-class", choices=sorted(reporting.FAILURE_CLASSES), default="product")
    parser.add_argument("--tested-sha"); parser.add_argument("--provider-version", action="append", default=[])
    parser.add_argument("--junit", type=Path)
    parser.add_argument("command", nargs=argparse.REMAINDER); args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ["--"] else args.command
    try: report = supervise(args.manifest, command, timeout=args.timeout, provider_config=args.provider_config,
                            qdrant_url=args.qdrant_url, observability_capture=args.observability_capture,
                            observability_db=args.observability_db)
    except Exception as error: report = {"success": False, "fatal": str(error)}
    success = report.get("success") is True
    if args.scenario_id:
        if not args.tested_sha: raise SystemExit("--tested-sha is required for canonical reporting")
        versions = dict(item.split("=", 1) for item in args.provider_version)
        scenario = canonical_scenario(report, scenario_id=args.scenario_id, tier=args.tier, capability=args.capability,
                                       surface=args.surface, failure_class=args.failure_class)
        report = reporting.suite_report([scenario], tested_sha=args.tested_sha, provider_versions=versions,
                                        policy={"supervisor": "ownership-revalidating-teardown"},
                                        upload={"status": "not_attempted", "local_evidence_path": str(args.report)})
        reporting.write_json(report, args.report)
        if args.junit: reporting.write_junit(report, args.junit)
    else:
        args.report.parent.mkdir(parents=True, exist_ok=True); args.report.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    return 0 if success else 2


if __name__ == "__main__": raise SystemExit(main())
