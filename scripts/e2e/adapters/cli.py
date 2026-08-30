#!/usr/bin/env python3
"""Data-only catalog projection for the Axon CLI E2E surface.

The catalog deliberately contains no executable instructions. This adapter owns
the small, auditable mapping from scenario IDs to argv arrays and invokes Axon
without a shell.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import time
from typing import Any
from urllib.parse import urlparse


ROOT = Path(__file__).resolve().parents[3]
DEFAULT_CATALOG = ROOT / "tests/e2e/catalog/catalog.json"
ISOLATION = ROOT / "scripts/e2e/lib/run-isolation.py"
TERMINAL_SUCCESS = {"completed", "completed_degraded", "succeeded", "success"}
KNOWN_JOB_STATES = TERMINAL_SUCCESS | {"accepted", "queued", "pending", "running", "canceled", "cancelled", "failed"}


def scenario_argv(scenario: dict[str, Any], fixture: dict[str, Any], prerequisites: dict[str, str] | None = None) -> list[str]:
    prerequisites = prerequisites or {}
    scenario_id = scenario["id"]
    if scenario_id == "source.inline.happy":
        return [str(fixture["source"]), "--scope", str(fixture["scope"]), "--wait", "true", "--json"]
    if scenario_id == "source.detached.negative":
        return ["", "--scope", str(fixture["scope"]), "--wait", "false", "--json"]
    if scenario_id == "jobs.stream.happy":
        return ["jobs", "list", "--limit", "10", "--json"]
    if scenario_id == "jobs.cancel.negative":
        return ["jobs", "cancel", "00000000-0000-0000-0000-000000000000", "--json"]
    if scenario_id == "prune.plan.happy":
        collection = prerequisites.get("fixture.owned_collection", os.environ.get("E2E_OWNED_COLLECTION", str(fixture["collection"])))
        return ["prune", "plan", "--collection", collection, "--json"]
    if scenario_id == "prune.execute.negative":
        collection = prerequisites.get("fixture.foreign_collection", os.environ.get("E2E_FOREIGN_COLLECTION", "production"))
        return ["prune", "exec", "--collection", collection, "--confirm", "--json"]
    raise ValueError(f"CLI adapter has no projection for catalog scenario {scenario_id!r}")


def apply_hermetic_projection(scenario: dict[str, Any], fixture: dict[str, Any], prerequisites: dict[str, str] | None = None) -> dict[str, Any]:
    projected = dict(fixture)
    if "fixture.http" not in scenario["setup_dependencies"]:
        return projected
    base_url = (prerequisites or {}).get("fixture.http", os.environ.get("AXON_E2E_FIXTURE_BASE_URL", ""))
    if not base_url:
        raise ValueError(f"{scenario['id']} requires AXON_E2E_FIXTURE_BASE_URL")
    parsed = urlparse(base_url)
    if parsed.scheme not in {"http", "https"} or parsed.hostname not in {"127.0.0.1", "::1", "localhost"}:
        raise ValueError("hermetic CLI fixture endpoint must be loopback")
    projected["source"] = base_url.rstrip("/") + "/page.html"
    return projected


def error_code(envelope: Any) -> str:
    if not isinstance(envelope, dict):
        return ""
    error = envelope.get("error")
    if isinstance(error, dict):
        return str(error.get("code", ""))
    return str(envelope.get("code", ""))


def normalized_failure_envelope(
    scenario: dict[str, Any], returncode: int, stdout: bytes, stderr: bytes,
) -> tuple[bytes, dict[str, Any] | None]:
    """Project known CLI text rejections into the adapter's typed envelope.

    Raw streams remain untouched on disk. This only handles expected, narrowly
    recognized rejection shapes; crashes and unrelated stderr stay untyped and
    fail the JSON-envelope oracle.
    """
    if returncode == 0 or stdout.strip():
        return stdout, None
    message = stderr.decode("utf-8", errors="replace").strip()
    folded = message.casefold()
    code = ""
    if scenario["id"] == "source.detached.negative" and "error:" in folded \
            and ("source" in folded or "invalid value" in folded) \
            and ("empty" in folded or "requires a local path" in folded or "''" in message or '""' in message):
        code = "validation.source_invalid"
    elif scenario["id"] == "jobs.cancel.negative" and "not found" in folded and "job" in folded:
        code = "jobs.not_found"
    elif scenario["id"] == "prune.execute.negative" and "ownership" in folded \
            and ("foreign" in folded or "not owned" in folded):
        code = "ownership.foreign_resource"
    if not code:
        return stdout, None
    envelope = {"error": {"code": code, "message": "recognized CLI rejection"},
                "normalized_from": "stderr"}
    return json.dumps(envelope, ensure_ascii=False).encode(), envelope


def oracle_passes(oracle: str, envelope: Any, scenario: dict[str, Any], fixture_job_id: str | None) -> bool:
    if not isinstance(envelope, dict):
        return False
    status = str(envelope.get("status", "")).lower()
    job_id = envelope.get("job_id") or envelope.get("id")
    if oracle == "source.accepted":
        return isinstance(job_id, str) and bool(job_id) and status in KNOWN_JOB_STATES
    if oracle == "job.terminal_success":
        return status in TERMINAL_SUCCESS
    if oracle == "job.visible":
        items = envelope.get("items", envelope.get("jobs"))
        return isinstance(items, list) and any(
            isinstance(item, dict) and (not fixture_job_id or item.get("job_id", item.get("id")) == fixture_job_id)
            for item in items
        )
    if oracle == "job.transition_valid":
        items = envelope.get("items", envelope.get("jobs", []))
        if isinstance(items, list) and items:
            return all(isinstance(item, dict) and str(item.get("status", "")).lower() in KNOWN_JOB_STATES for item in items)
        return status in KNOWN_JOB_STATES
    if oracle == "prune.plan_digest_bound":
        plan = envelope.get("plan")
        return isinstance(plan, dict) and isinstance(plan.get("digest", plan.get("plan_digest")), str) and bool(plan.get("digest", plan.get("plan_digest")))
    if oracle == "resource.ownership_checked":
        plan = envelope.get("plan")
        if isinstance(plan, dict):
            return plan.get("ownership_checked") is True or isinstance(plan.get("owned_resources"), list)
        return error_code(envelope).startswith(("ownership.", "safety.", "prune."))
    code = error_code(envelope)
    if oracle == "failure.taxonomy":
        return bool(code) and code.startswith(("jobs.", "job.", "source.", "ownership.", "safety.", "prune.", "validation."))
    if oracle == "rejection.source_invalid":
        return code.startswith(("source.", "validation."))
    if oracle == "rejection.job_missing":
        return code.startswith(("jobs.", "job.")) and ("missing" in code or "not_found" in code)
    if oracle == "rejection.ownership_guard":
        return code.startswith(("ownership.", "safety.", "prune."))
    return False


def classify(returncode: int, stdout: bytes, stderr: bytes, scenario: dict[str, Any],
             fixture_job_id: str | None = None) -> tuple[str, str | None, list[dict[str, Any]]]:
    assertions: list[dict[str, Any]] = []
    try:
        envelope = json.loads(stdout)
        assertions.append({"id": "cli.json_object", "passed": isinstance(envelope, dict)})
    except (UnicodeDecodeError, json.JSONDecodeError):
        envelope = None
        assertions.append({"id": "cli.json_object", "passed": False})
    code = error_code(envelope)
    negative = scenario["polarity"] == "negative"
    exit_ok = returncode != 0 if negative else returncode == 0
    assertions.append({"id": "cli.exit_semantics", "passed": exit_ok})
    provider_error = code.startswith("provider.") or (isinstance(envelope, dict) and envelope.get("provider_error") is True)
    assertions.append({"id": "cli.no_provider_error_envelope", "passed": not provider_error})
    for oracle in scenario["semantic_oracles"]:
        assertions.append({"id": oracle, "passed": oracle_passes(oracle, envelope, scenario, fixture_job_id)})
    result = "pass" if all(item["passed"] for item in assertions) else "fail"
    if returncode == 124:
        result = "timeout"
    elif returncode < 0:
        result = "signal"
    failure_class = None if result == "pass" else ("provider" if provider_error else "product")
    return result, failure_class, assertions


def loopback_url(name: str) -> str:
    value = os.environ.get(name, "")
    parsed = urlparse(value)
    if parsed.scheme not in {"http", "https"} or parsed.hostname not in {"127.0.0.1", "::1", "localhost"}:
        raise ValueError(f"{name} must name an owned loopback endpoint")
    port = parsed.port or (443 if parsed.scheme == "https" else 80)
    try:
        with socket.create_connection((parsed.hostname, port), timeout=1):
            pass
    except OSError as error:
        raise ValueError(f"{name} is not reachable during preflight") from error
    return value


def resolve_prerequisites(scenarios: list[dict[str, Any]], outdir: Path) -> tuple[dict[str, str], dict[str, str]]:
    dependencies = {dependency for scenario in scenarios for dependency in scenario["setup_dependencies"]}
    supported = {"fixture.http", "fixture.job", "provider.fake", "fixture.owned_collection", "fixture.foreign_collection"}
    unknown = dependencies - supported
    if unknown:
        raise ValueError(f"unsupported setup dependencies: {', '.join(sorted(unknown))}")
    resolved: dict[str, str] = {}
    if "fixture.http" in dependencies:
        resolved["fixture.http"] = loopback_url("AXON_E2E_FIXTURE_BASE_URL")
    if "provider.fake" in dependencies:
        resolved["provider.fake"] = loopback_url("AXON_E2E_FAKE_PROVIDER_URL")
    if "fixture.job" in dependencies:
        job_id = os.environ.get("AXON_E2E_FIXTURE_JOB_ID", "")
        if not job_id or any(char in job_id for char in "\r\n\t"):
            raise ValueError("fixture.job requires AXON_E2E_FIXTURE_JOB_ID")
        resolved["fixture.job"] = job_id
    if "fixture.foreign_collection" in dependencies:
        foreign = os.environ.get("E2E_FOREIGN_COLLECTION", "")
        if not foreign or foreign.startswith("axon_e2e_"):
            raise ValueError("fixture.foreign_collection requires a non-owned E2E_FOREIGN_COLLECTION")
        resolved["fixture.foreign_collection"] = foreign

    allocation = subprocess.run(
        [sys.executable, str(ISOLATION), "allocate", "--run-base", str(outdir / "runs"),
         "--manifest-base", str(outdir / "manifests")],
        capture_output=True, text=True, check=False,
    )
    if allocation.returncode:
        raise ValueError(f"run-isolation allocation failed: {allocation.stderr.strip()}")
    isolation = json.loads(allocation.stdout)
    resolved["fixture.owned_collection"] = isolation["namespace"]
    return resolved, isolation


def selected_scenarios(catalog: dict[str, Any], ids: set[str], group: str | None,
                       shard_index: int, shard_count: int) -> list[dict[str, Any]]:
    scenarios = [item for item in catalog["scenarios"] if "cli" in item["surfaces"]]
    if ids:
        known = {item["id"] for item in scenarios}
        missing = ids - known
        if missing:
            raise ValueError(f"unknown CLI scenario(s): {', '.join(sorted(missing))}")
        scenarios = [item for item in scenarios if item["id"] in ids]
    if group:
        scenarios = [item for item in scenarios if item["lifecycle"] == group]
        if not scenarios:
            raise ValueError(f"unknown or empty CLI scenario group: {group}")
    selected = [item for item in scenarios if int(hashlib.sha256(item["id"].encode()).hexdigest(), 16) % shard_count == shard_index]
    if (ids or group) and not selected:
        raise ValueError("CLI scenario selection is empty after sharding")
    return selected


def register_created_identities(manifest: str, run_id: str, scenario: dict[str, Any], stdout: bytes) -> list[dict[str, str]]:
    try:
        envelope = json.loads(stdout)
    except (UnicodeDecodeError, json.JSONDecodeError):
        return []
    if not isinstance(envelope, dict):
        return []
    candidates: list[tuple[str, Any]] = []
    if scenario["capability"] == "source":
        candidates.extend((("job", envelope.get("job_id")), ("source", envelope.get("source_id"))))
    candidates.append(("collection", envelope.get("collection")))
    registered: list[dict[str, str]] = []
    for resource_type, identity in candidates:
        if not isinstance(identity, str) or not identity:
            continue
        if resource_type == "collection" and not identity.startswith("axon_e2e_"):
            raise ValueError(f"refusing to register non-owned collection returned by {scenario['id']}")
        metadata = json.dumps({"run_id": run_id, "scenario_id": scenario["id"]})
        result = subprocess.run(
            [sys.executable, str(ISOLATION), "register", manifest, resource_type, identity,
             "--metadata-json", metadata], capture_output=True, text=True, check=False,
        )
        if result.returncode:
            raise ValueError(f"manifest registration failed for {resource_type} {identity}: {result.stderr.strip()}")
        registered.append({"type": resource_type, "identity": identity})
    return registered


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--catalog", type=Path, default=DEFAULT_CATALOG)
    parser.add_argument("--axon-bin", type=Path, required=True)
    parser.add_argument("--outdir", type=Path, required=True)
    parser.add_argument("--scenario", action="append", default=[])
    parser.add_argument("--scenario-group")
    parser.add_argument("--shard-index", type=int, default=0)
    parser.add_argument("--shard-count", type=int, default=1)
    parser.add_argument("--timeout-secs", type=float, default=120)
    args = parser.parse_args()
    if any(not item for item in args.scenario):
        parser.error("--scenario requires a non-empty ID")
    if args.shard_count < 1 or not 0 <= args.shard_index < args.shard_count:
        parser.error("shard index must be in [0, shard count), and count must be positive")

    catalog = json.loads(args.catalog.read_text())
    scenarios = selected_scenarios(catalog, set(args.scenario), args.scenario_group,
                                   args.shard_index, args.shard_count)
    args.outdir.mkdir(parents=True, exist_ok=True)
    log_dir = args.outdir / "logs"
    log_dir.mkdir(exist_ok=True)
    evidence_path = args.outdir / "cli-evidence.jsonl"
    prerequisites, isolation = resolve_prerequisites(scenarios, args.outdir)
    prepared: list[tuple[dict[str, Any], list[str]]] = []
    for scenario in scenarios:
        fixture_path = ROOT / scenario["requests"]["cli"]
        if not fixture_path.is_file():
            raise ValueError(f"missing CLI fixture for {scenario['id']}: {fixture_path}")
        fixture = apply_hermetic_projection(
            scenario, json.loads(fixture_path.read_text()), prerequisites
        )
        prepared.append((scenario, [str(args.axon_bin), *scenario_argv(scenario, fixture, prerequisites)]))
    tested_sha = subprocess.run(
        ["git", "rev-parse", "HEAD"], cwd=ROOT, capture_output=True, text=True, check=True
    ).stdout.strip()
    failed = False
    base_env = {
        **os.environ,
        "AXON_DATA_DIR": isolation["data_dir"],
        "AXON_SQLITE_PATH": isolation["sqlite"],
        "AXON_COLLECTION": isolation["namespace"],
        "AXON_E2E_RUN_ID": isolation["run_id"],
    }
    with evidence_path.open("w", encoding="utf-8") as evidence:
        for scenario, argv in prepared:
            history: list[dict[str, Any]] = []
            # The canonical catalog is hermetic. Required hermetic scenarios
            # never auto-retry; live diagnostic retry is governed separately
            # after teardown/namespace safety evidence exists.
            max_attempts = 1
            result, failure_class, assertions = "fail", "harness", []
            for attempt in range(1, max_attempts + 1):
                attempt_namespace = f"{isolation['namespace']}_attempt_{attempt}"
                started = time.monotonic_ns()
                timed_out = False
                try:
                    completed = subprocess.run(
                        argv, cwd=ROOT, env={**base_env, "AXON_E2E_ATTEMPT_ID": attempt_namespace},
                        capture_output=True, timeout=args.timeout_secs,
                    )
                    returncode, stdout, stderr = completed.returncode, completed.stdout, completed.stderr
                except subprocess.TimeoutExpired as error:
                    timed_out = True
                    returncode = 124
                    stdout, stderr = error.stdout or b"", error.stderr or b""
                elapsed_ms = (time.monotonic_ns() - started) // 1_000_000
                classified_stdout, normalized_envelope = normalized_failure_envelope(
                    scenario, returncode, stdout, stderr
                )
                result, failure_class, assertions = classify(
                    returncode, classified_stdout, stderr, scenario, prerequisites.get("fixture.job")
                )
                if timed_out:
                    result, failure_class = "timeout", "timeout"
                stdout_path = log_dir / f"{scenario['id']}.attempt-{attempt}.stdout"
                stderr_path = log_dir / f"{scenario['id']}.attempt-{attempt}.stderr"
                stdout_path.write_bytes(stdout)
                stderr_path.write_bytes(stderr)
                history.append({
                    "attempt": attempt, "namespace": attempt_namespace, "result": result,
                    "failure_class": failure_class, "exit_code": returncode, "timing_ms": elapsed_ms,
                    "assertions": assertions, "stdout": str(stdout_path), "stderr": str(stderr_path),
                    "normalized_envelope": normalized_envelope,
                })
                if result == "pass":
                    break
            registered_identities: list[dict[str, str]] = []
            if result == "pass":
                try:
                    registered_identities = register_created_identities(
                        isolation["manifest"], isolation["run_id"], scenario, stdout
                    )
                except ValueError as error:
                    result, failure_class = "fail", "harness"
                    assertions.append({"id": "manifest.registration", "passed": False, "detail": str(error)})
            record = {
                "schema_version": 1, "surface": "cli", "scenario_id": scenario["id"],
                "tested_sha": tested_sha, "result": result, "failure_class": failure_class,
                "exit_code": returncode, "timing_ms": sum(item["timing_ms"] for item in history),
                "attempts": len(history), "attempt_history": history, "assertions": assertions,
                "stdout": str(stdout_path), "stderr": str(stderr_path),
                "cleanup": {"contract": scenario["cleanup_contract"], "registered": True,
                            "status": "registered" if result == "pass" else "partial",
                            "manifest": isolation["manifest"],
                            "returned_identity_registration": "complete" if result == "pass" else "partial",
                            "command_created_resources": registered_identities},
            }
            evidence.write(json.dumps(record, ensure_ascii=False) + "\n")
            failed |= result != "pass"
    return 1 if failed else 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, KeyError, json.JSONDecodeError) as error:
        print(f"CLI catalog error: {error}", file=sys.stderr)
        raise SystemExit(2)
