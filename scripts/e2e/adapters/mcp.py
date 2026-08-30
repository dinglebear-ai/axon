#!/usr/bin/env python3
"""Project the static E2E catalog onto Axon's MCP surface.

Catalog values are parsed as JSON and emitted as JSON/argv data only.  They are
never evaluated by a shell.  The full mcporter and raw task-wire harnesses own
transport control flow; this module owns only projection and evidence shape.
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import re
import secrets
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[3]
DEFAULT_CATALOG = ROOT / "tests/e2e/catalog/catalog.json"
SECRET_KEY = re.compile(r"token|secret|password|authorization|api[_-]?key", re.I)
SECRET_VALUE = re.compile(r"(?i)(bearer\s+|(?:api[_-]?key|token|secret|password)[=:]\s*)[^\s,;]+")


class McpAdapterError(ValueError):
    pass


def load_catalog(path: Path = DEFAULT_CATALOG) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if value.get("schema_version") != 1 or not isinstance(value.get("scenarios"), list):
        raise McpAdapterError("unsupported E2E catalog")
    return value


def scenarios(path: Path = DEFAULT_CATALOG, *, tier: str | None = None) -> list[dict[str, Any]]:
    selected = []
    for scenario in load_catalog(path)["scenarios"]:
        if "mcp" not in scenario.get("surfaces", []):
            continue
        if tier is not None and scenario.get("tier") != tier:
            continue
        request_path = scenario.get("requests", {}).get("mcp")
        if not request_path:
            raise McpAdapterError(f"{scenario.get('id')}: MCP request fixture is missing")
        fixture = (ROOT / request_path).resolve()
        try:
            fixture.relative_to(ROOT / "tests")
        except ValueError as error:
            raise McpAdapterError(f"{scenario.get('id')}: request fixture escapes tests") from error
        request = json.loads(fixture.read_text(encoding="utf-8"))
        selected.append({
            "id": scenario["id"],
            "capability": scenario["capability"],
            "polarity": scenario["polarity"],
            "execution_mode": scenario["execution_mode"],
            "request": request,
            "semantic_oracles": scenario["semantic_oracles"],
            "envelope_oracles": scenario["envelope_oracles"]["mcp"],
            "cleanup_contract": scenario["cleanup_contract"],
            "resource_classes": scenario["resource_classes"],
            "resource_ownership": scenario["resource_ownership"],
            "provider": scenario["provider"]["class"],
        })
    return selected


def tool_arguments(item: dict[str, Any]) -> dict[str, Any]:
    """Translate a catalog fixture into typed Axon tool arguments."""
    request = dict(item["request"])
    capability = item["capability"]
    if capability == "source":
        if item["polarity"] == "negative":
            return {"action": "source", "source": "", "scope": "page", "detached": True}
        return {"action": "source", **request, "detached": False}
    if capability == "jobs":
        subaction = "stream" if item["polarity"] == "happy" else "cancel"
        job_id = "00000000-0000-0000-0000-000000000000" if item["polarity"] == "negative" else "${E2E_JOB_ID}"
        return {"action": "jobs", "subaction": subaction, "job_id": job_id}
    if capability == "prune":
        subaction = "plan" if item["polarity"] == "happy" else "exec"
        target = "collection:${E2E_FOREIGN_COLLECTION}" if item["polarity"] == "negative" else "collection:${E2E_OWNED_COLLECTION}"
        return {"action": "prune", "subaction": subaction, "target": target, "dry_run": bool(request.get("dry_run", True))}
    raise McpAdapterError(f"{item['id']}: unsupported MCP capability {capability!r}")


def mcporter_argv(selector: str, arguments: dict[str, Any]) -> list[str]:
    """Return argv suitable for subprocess execution without interpolation."""
    argv = ["call", selector, "--args", json.dumps(arguments, separators=(",", ":")), "--output", "json"]
    if selector.startswith("http://127.0.0.1:") or selector.startswith("http://localhost:"):
        argv.append("--allow-http")
    return argv


def redact(value: Any) -> Any:
    if isinstance(value, dict):
        return {key: "[REDACTED]" if SECRET_KEY.search(key) else redact(item) for key, item in value.items()}
    if isinstance(value, list):
        return [redact(item) for item in value]
    if isinstance(value, str):
        return SECRET_VALUE.sub(lambda match: match.group(1) + "[REDACTED]", value)
    return value


def normalize(scenario: dict[str, Any], transport: str, envelope: dict[str, Any]) -> dict[str, Any]:
    error = envelope.get("error")
    data = envelope.get("data") if isinstance(envelope.get("data"), dict) else {}
    provider_error = isinstance(error, str) and any(
        marker in error.casefold() for marker in ("qdrant", "tei", "chrome", "llm", "provider")
    )
    success = envelope.get("ok") is True and not error
    if scenario["provider"] not in {"none", "fake"} and provider_error:
        success = False
    return redact({
        "schema_version": 1,
        "scenario_id": scenario["id"],
        "surface": "mcp",
        "transport": transport,
        "success": success,
        "semantic_oracles": scenario["semantic_oracles"],
        "envelope_oracles": scenario["envelope_oracles"],
        "cleanup": {"contract": scenario["cleanup_contract"], "state": "registered_only", "executed": False},
        "facts": facts(envelope),
        "protocol": {
            "ok": envelope.get("ok"),
            "action": envelope.get("action"),
            "subaction": envelope.get("subaction"),
            "has_task": isinstance(envelope.get("result", {}).get("taskId"), str),
            "response_mode": data.get("response_mode"),
            "error": error,
        },
    })


def _values(value: Any, key_names: set[str]) -> list[Any]:
    found = []
    if isinstance(value, dict):
        for key, item in value.items():
            if key in key_names:
                found.append(item)
            found.extend(_values(item, key_names))
    elif isinstance(value, list):
        for item in value:
            found.extend(_values(item, key_names))
    return found


def facts(envelope: dict[str, Any]) -> dict[str, Any]:
    statuses = [value for value in _values(envelope, {"status", "state"}) if isinstance(value, str)]
    return {
        "job_ids": [value for value in _values(envelope, {"job_id", "jobId", "id"}) if isinstance(value, str)],
        "source_ids": [value for value in _values(envelope, {"source_id", "sourceId"}) if isinstance(value, str)],
        "collections": [value for value in _values(envelope, {"collection", "collection_name"}) if isinstance(value, str)],
        "statuses": statuses,
        "terminal_success": any(value in {"completed", "success", "succeeded"} for value in statuses),
        "plan_digests": [value for value in _values(envelope, {"digest", "plan_digest", "plan_id"}) if isinstance(value, str) and value],
        "ownership_markers": _values(envelope, {"owned", "ownership_checked", "owner_run_id"}),
    }


def evaluate(scenario: dict[str, Any], evidence: dict[str, Any]) -> list[str]:
    failures = []
    protocol = evidence.get("protocol", {})
    structured_error = isinstance(protocol.get("error"), (str, dict))
    known_envelopes = {"mcp.content_or_task"}
    unknown_envelopes = set(scenario["envelope_oracles"]) - known_envelopes
    if unknown_envelopes:
        failures.append(f"unknown envelope oracles: {sorted(unknown_envelopes)}")
    if "mcp.content_or_task" in scenario["envelope_oracles"]:
        expected_error = scenario["polarity"] == "negative"
        if expected_error and not structured_error:
            failures.append("expected structured MCP error")
        elif not expected_error and not evidence.get("success"):
            failures.append("expected successful MCP content or task envelope")
    if scenario.get("cleanup_contract") != "cleanup.owned_source_run":
        failures.append("unknown or missing cleanup contract")
    cleanup = evidence.get("cleanup", {})
    if cleanup.get("contract") != scenario.get("cleanup_contract") or cleanup.get("state") != "registered_only" or cleanup.get("executed") is not False:
        failures.append("cleanup declaration is not registration-only")
    facts_ = evidence.get("facts", {})
    error_text = str(protocol.get("error", "")).casefold()
    handlers = {
        "source.accepted": lambda: evidence.get("success") and protocol.get("action") == "source" and bool(facts_.get("job_ids")),
        "job.terminal_success": lambda: evidence.get("success") and bool(facts_.get("terminal_success")),
        "rejection.source_invalid": lambda: structured_error and not evidence.get("success") and any(marker in error_text for marker in ("source", "invalid", "empty", "required")),
        "rejection.job_missing": lambda: structured_error and not evidence.get("success") and any(marker in error_text for marker in ("job", "not found", "missing", "unknown")),
        "failure.taxonomy": lambda: structured_error and any(marker in str(protocol.get("error", "")).casefold() for marker in ("invalid", "missing", "not found", "unknown", "forbidden", "ownership", "confirm", "required", "empty")),
        "job.visible": lambda: evidence.get("success") and protocol.get("action") == "jobs" and bool(facts_.get("job_ids")),
        "job.transition_valid": lambda: evidence.get("success") and bool(facts_.get("statuses")),
        "prune.plan_digest_bound": lambda: evidence.get("success") and bool(facts_.get("plan_digests")),
        "resource.ownership_checked": lambda: evidence.get("success") and bool(facts_.get("ownership_markers") or facts_.get("collections")),
        "rejection.ownership_guard": lambda: structured_error and not evidence.get("success") and any(marker in error_text for marker in ("ownership", "owned", "foreign", "plan", "confirm")),
    }
    for oracle in scenario["semantic_oracles"]:
        handler = handlers.get(oracle)
        if handler is None:
            failures.append(f"unknown semantic oracle: {oracle}")
        elif not handler():
            failures.append(f"semantic oracle failed: {oracle}")
    if scenario.get("resource_ownership", {}).get("strategy") != "run_manifest":
        failures.append("cleanup contract requires run_manifest ownership")
    return failures


def register_evidence(manifest_path: Path, scenario: dict[str, Any], evidence_path: Path, envelope_path: Path,
                      owned_collection: str | None = None) -> dict[str, Any]:
    spec = importlib.util.spec_from_file_location("run_isolation", ROOT / "scripts/e2e/lib/run-isolation.py")
    isolation = importlib.util.module_from_spec(spec); spec.loader.exec_module(isolation)
    manifest = isolation.Manifest.open(manifest_path)
    envelope = json.loads(envelope_path.read_text(encoding="utf-8"))
    extracted = facts(envelope)
    safe_id = re.sub(r"[^A-Za-z0-9_-]", "_", scenario["id"])
    manifest.register("artifact", f"axon_e2e_{safe_id}_evidence", {"path": str(evidence_path.resolve()), "role": "normalized_evidence"})
    for index, job_id in enumerate(extracted["job_ids"][:8]):
        manifest.register("artifact", f"axon_e2e_{safe_id}_job_{index}", {"external_type": "job", "external_id": job_id})
    for index, source_id in enumerate(extracted["source_ids"][:8]):
        manifest.register("artifact", f"axon_e2e_{safe_id}_source_{index}", {"external_type": "source", "external_id": source_id})
    for collection in extracted["collections"]:
        if isinstance(collection, str) and collection.startswith("axon_e2e_"):
            manifest.register("collection", collection, {"scenario_id": scenario["id"],
                                                         "ownership_generation": secrets.token_hex(32)})
    if owned_collection and owned_collection.startswith("axon_e2e_"):
        manifest.register("collection", owned_collection, {"scenario_id": scenario["id"], "role": "requested_collection",
                                                            "ownership_generation": secrets.token_hex(32)})
    return {"registered": True, "cleanup_state": "registered_only", "cleanup_executed": False}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--catalog", type=Path, default=DEFAULT_CATALOG)
    sub = parser.add_subparsers(dest="command", required=True)
    listing = sub.add_parser("list")
    listing.add_argument("--tier")
    project = sub.add_parser("project")
    project.add_argument("scenario_id")
    project.add_argument("--selector", default="axon.axon")
    normalizer = sub.add_parser("normalize")
    normalizer.add_argument("scenario_id")
    normalizer.add_argument("transport", choices=("stdio", "http"))
    normalizer.add_argument("envelope", type=Path)
    evaluator = sub.add_parser("evaluate")
    evaluator.add_argument("scenario_id")
    evaluator.add_argument("evidence", type=Path)
    registrar = sub.add_parser("register-evidence")
    registrar.add_argument("scenario_id")
    registrar.add_argument("manifest", type=Path)
    registrar.add_argument("evidence", type=Path)
    registrar.add_argument("envelope", type=Path)
    registrar.add_argument("--owned-collection")
    args = parser.parse_args()
    items = scenarios(args.catalog, tier=getattr(args, "tier", None))
    if args.command == "list":
        print(json.dumps(items, sort_keys=True))
        return 0
    item = next((candidate for candidate in items if candidate["id"] == args.scenario_id), None)
    if item is None:
        raise McpAdapterError(f"unknown MCP scenario: {args.scenario_id}")
    if args.command == "project":
        print(json.dumps({"arguments": tool_arguments(item), "argv": mcporter_argv(args.selector, tool_arguments(item))}, sort_keys=True))
    elif args.command == "normalize":
        print(json.dumps(normalize(item, args.transport, json.loads(args.envelope.read_text(encoding="utf-8"))), sort_keys=True))
    elif args.command == "evaluate":
        failures = evaluate(item, json.loads(args.evidence.read_text(encoding="utf-8")))
        print(json.dumps({"scenario_id": item["id"], "passed": not failures, "failures": failures}, sort_keys=True))
        return int(bool(failures))
    else:
        print(json.dumps(register_evidence(args.manifest, item, args.evidence, args.envelope, args.owned_collection), sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
