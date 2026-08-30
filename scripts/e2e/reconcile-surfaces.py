#!/usr/bin/env python3
"""Offline, deterministic reconciliation of Axon cross-surface E2E evidence."""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import sys
from collections import defaultdict
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[2]
DEFAULT_CATALOG = ROOT / "tests/e2e/catalog/catalog.json"
SURFACES = {"cli", "mcp", "mcp_task_wire", "http"}
TERMINAL = {"completed", "completed_degraded", "succeeded", "success", "failed", "canceled", "cancelled"}
SEMANTIC_FIELDS = (
    "semantic_value", "terminal_state", "error_code", "citations",
    "resource_identity", "lineage", "effects",
)


def load(path: Path) -> Any:
    with path.open(encoding="utf-8") as handle:
        return json.load(handle)


def canonical(value: Any) -> Any:
    if isinstance(value, dict):
        return {key: canonical(value[key]) for key in sorted(value)}
    if isinstance(value, list):
        return sorted((canonical(item) for item in value), key=lambda item: json.dumps(item, sort_keys=True))
    return value


def evidence_path(bundle_path: Path, relative: Any, errors: list[dict[str, Any]], context: dict[str, Any]) -> Path | None:
    if not isinstance(relative, str) or not relative:
        errors.append({**context, "invariant": "evidence_path", "detail": "missing evidence path"})
        return None
    candidate = (bundle_path.parent / relative).resolve()
    try:
        candidate.relative_to(bundle_path.parent.resolve())
    except ValueError:
        errors.append({**context, "invariant": "evidence_path", "detail": "evidence path escapes bundle directory"})
        return None
    if not candidate.is_file():
        errors.append({**context, "invariant": "evidence_path", "detail": f"evidence does not exist: {relative}"})
        return None
    return candidate


def authoritative_operations() -> set[str]:
    module_path = ROOT / "scripts/e2e/validate-catalog.py"
    spec = importlib.util.spec_from_file_location("axon_e2e_catalog_validator", module_path)
    if spec is None or spec.loader is None:
        raise RuntimeError("cannot load catalog validator")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.expected_operations()


def validate_inventory(catalog: dict[str, Any], errors: list[dict[str, Any]]) -> dict[str, dict[str, Any]]:
    rows = {item["id"]: item for item in catalog.get("operations", []) if isinstance(item, dict) and isinstance(item.get("id"), str)}
    expected = authoritative_operations()
    for operation_id in sorted(expected - rows.keys()):
        errors.append({"scenario": None, "capability": operation_id, "surface": None,
                       "invariant": "inventory.classified", "evidence_path": None,
                       "detail": "advertised operation is unclassified"})
    for operation_id in sorted(rows.keys() - expected):
        errors.append({"scenario": None, "capability": operation_id, "surface": None,
                       "invariant": "inventory.current", "evidence_path": None,
                       "detail": "catalog operation is absent from authoritative inventories"})
    for operation_id, row in sorted(rows.items()):
        if row.get("classification") == "unsupported":
            reason = row.get("reason", "")
            owner = row.get("owner", "")
            if not isinstance(reason, str) or not reason.strip() or not isinstance(owner, str) or not owner.strip():
                errors.append({"scenario": None, "capability": operation_id, "surface": None,
                               "invariant": "unsupported.rationale_owner", "evidence_path": None,
                               "detail": "unsupported mapping requires a non-empty reason and owner"})
    return rows


def reconcile_parity(bundle: dict[str, Any], bundle_path: Path, errors: list[dict[str, Any]]) -> int:
    groups: dict[str, list[dict[str, Any]]] = defaultdict(list)
    execution_ids: set[str] = set()
    for item in bundle.get("executions", []):
        scenario = item.get("parent_scenario_id")
        execution_id = item.get("execution_id")
        surface = item.get("surface")
        context = {"scenario": scenario, "capability": item.get("capability"), "surface": surface,
                   "evidence_path": item.get("evidence_path")}
        if not isinstance(scenario, str) or not isinstance(execution_id, str) or surface not in SURFACES:
            errors.append({**context, "invariant": "execution.identity", "detail": "invalid scenario, execution ID, or surface"})
            continue
        if execution_id in execution_ids:
            errors.append({**context, "invariant": "execution.unique", "detail": f"duplicate execution ID: {execution_id}"})
        execution_ids.add(execution_id)
        path = evidence_path(bundle_path, item.get("evidence_path"), errors, context)
        if path is not None:
            digest = hashlib.sha256(path.read_bytes()).hexdigest()
            if item.get("evidence_sha256") != digest:
                errors.append({**context, "invariant": "evidence.sha256", "detail": "saved evidence digest differs"})
        semantics = item.get("semantics")
        if not isinstance(semantics, dict):
            errors.append({**context, "invariant": "semantics.object", "detail": "semantic projection must be an object"})
            continue
        missing_fields = [field for field in SEMANTIC_FIELDS if field not in semantics]
        if missing_fields:
            errors.append({**context, "invariant": "semantics.complete",
                           "detail": f"semantic projection is missing: {', '.join(missing_fields)}"})
        terminal = semantics.get("terminal_state")
        if terminal is not None and terminal not in TERMINAL:
            errors.append({**context, "invariant": "terminal_state.known", "detail": f"unknown terminal state: {terminal}"})
        envelope = item.get("envelope")
        if not isinstance(envelope, dict):
            errors.append({**context, "invariant": "envelope.surface_specific", "detail": "transport envelope projection is required"})
        else:
            assertions = envelope.get("assertions")
            prefix = "mcp" if surface == "mcp_task_wire" else surface
            valid = isinstance(assertions, list) and bool(assertions) and all(
                isinstance(assertion, dict)
                and isinstance(assertion.get("id"), str)
                and assertion["id"].startswith(f"{prefix}.")
                and assertion.get("passed") is True
                for assertion in assertions
            )
            if not valid:
                errors.append({**context, "invariant": "envelope.surface_specific",
                               "detail": "surface-prefixed envelope assertions must exist and pass"})
        groups[scenario].append(item)

    comparisons = 0
    for scenario, members in sorted(groups.items()):
        surfaces = [item["surface"] for item in members]
        capabilities = {item.get("capability") for item in members}
        if len(capabilities) != 1 or not all(isinstance(value, str) and value for value in capabilities):
            errors.append({"scenario": scenario, "capability": None, "surface": None,
                           "invariant": "capability.consistent", "evidence_path": None,
                           "detail": "all executions for a parent scenario must name the same capability"})
        if len(surfaces) != len(set(surfaces)):
            errors.append({"scenario": scenario, "capability": members[0].get("capability"), "surface": None,
                           "invariant": "surface.unique", "evidence_path": None,
                           "detail": "scenario has duplicate surface executions"})
        if len(members) < 2:
            continue
        reference = members[0]
        mode = reference.get("comparison_mode")
        if mode not in {"independent", "multi_observer"} or any(item.get("comparison_mode") != mode for item in members):
            errors.append({"scenario": scenario, "capability": reference.get("capability"), "surface": None,
                           "invariant": "comparison_mode", "evidence_path": None,
                           "detail": "group must consistently declare independent or multi_observer"})
            continue
        for item in members[1:]:
            comparisons += 1
            for field in SEMANTIC_FIELDS:
                left = canonical(reference["semantics"].get(field))
                right = canonical(item["semantics"].get(field))
                if left != right:
                    errors.append({"scenario": scenario, "capability": item.get("capability"), "surface": item["surface"],
                                   "invariant": field, "evidence_path": item.get("evidence_path"),
                                   "detail": f"differs from {reference['surface']} evidence {reference.get('evidence_path')}"})
            if mode == "multi_observer" and reference.get("observed_operation_id") != item.get("observed_operation_id"):
                errors.append({"scenario": scenario, "capability": item.get("capability"), "surface": item["surface"],
                               "invariant": "observed_operation_id", "evidence_path": item.get("evidence_path"),
                               "detail": "multi-observer evidence must identify the same literal operation"})
    return comparisons


def reconcile_coverage(catalog: dict[str, Any], rows: dict[str, dict[str, Any]], bundle: dict[str, Any],
                       bundle_path: Path, errors: list[dict[str, Any]]) -> dict[str, Any]:
    covered: set[str] = set()
    lifecycle_evidence: dict[str, set[str]] = defaultdict(set)
    for item in bundle.get("coverage", []):
        operation_id = item.get("operation_id")
        context = {"scenario": item.get("scenario_id"), "capability": operation_id,
                   "surface": item.get("surface"), "evidence_path": item.get("evidence_path")}
        if item.get("surface") not in SURFACES:
            errors.append({**context, "invariant": "coverage.surface", "detail": "behavioral evidence requires a known execution surface"})
            continue
        row = rows.get(operation_id)
        if row is None:
            errors.append({**context, "invariant": "coverage.inventory", "detail": "coverage references an unknown operation"})
            continue
        if row.get("classification") != "behavioral_e2e":
            errors.append({**context, "invariant": "coverage.classification", "detail": "only behavioral_e2e evidence earns coverage"})
            continue
        if item.get("result") != "pass" or item.get("kind") != "behavioral":
            continue
        path = evidence_path(bundle_path, item.get("evidence_path"), errors, context)
        if path is None:
            continue
        try:
            saved = load(path)
        except (OSError, json.JSONDecodeError) as error:
            errors.append({**context, "invariant": "coverage.evidence_json", "detail": f"invalid JSON evidence: {error}"})
            continue
        expected = {"operation_id": operation_id, "scenario_id": item.get("scenario_id"),
                    "kind": "behavioral", "result": "pass", "surface": item.get("surface"),
                    "lifecycle": item.get("lifecycle"), "polarity": item.get("polarity")}
        if not isinstance(saved, dict) or any(saved.get(key) != value for key, value in expected.items()):
            errors.append({**context, "invariant": "coverage.evidence_binding",
                           "detail": "saved evidence does not bind this operation, scenario, kind, and passing result"})
            continue
        covered.add(operation_id)
        lifecycle = item.get("lifecycle")
        polarity = item.get("polarity")
        if isinstance(lifecycle, str) and polarity in {"happy", "negative"}:
            lifecycle_evidence[lifecycle].add(polarity)
    denominator = len(rows)
    percent = 100 * len(covered) / denominator if denominator else 0.0
    threshold = catalog.get("coverage_policy", {}).get("behavioral_percent", 80)
    if percent < threshold:
        errors.append({"scenario": None, "capability": None, "surface": None,
                       "invariant": "coverage.threshold", "evidence_path": None,
                       "detail": f"behavioral evidence coverage {percent:.1f}% is below {threshold}% ({len(covered)}/{denominator})"})
    for lifecycle in catalog.get("coverage_policy", {}).get("critical_lifecycles", []):
        if lifecycle_evidence[lifecycle] != {"happy", "negative"}:
            errors.append({"scenario": None, "capability": lifecycle, "surface": None,
                           "invariant": "coverage.critical_lifecycle", "evidence_path": None,
                           "detail": "critical lifecycle requires passing happy and negative behavioral evidence"})
    return {"denominator": denominator, "covered": len(covered), "percent": round(percent, 1), "threshold": threshold}


def reconcile(catalog: dict[str, Any], bundle: dict[str, Any], bundle_path: Path) -> dict[str, Any]:
    errors: list[dict[str, Any]] = []
    if bundle.get("schema_version") != 1:
        errors.append({"scenario": None, "capability": None, "surface": None, "invariant": "schema_version",
                       "evidence_path": str(bundle_path), "detail": "bundle schema_version must equal 1"})
    rows = validate_inventory(catalog, errors)
    comparisons = reconcile_parity(bundle, bundle_path, errors)
    coverage = reconcile_coverage(catalog, rows, bundle, bundle_path, errors)
    errors.sort(key=lambda item: tuple(str(item.get(key) or "") for key in
                                      ("scenario", "capability", "surface", "invariant", "evidence_path", "detail")))
    return {"schema_version": 1, "passed": not errors, "coverage": coverage,
            "parity_comparisons": comparisons, "failures": errors}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("bundle", type=Path)
    parser.add_argument("--catalog", type=Path, default=DEFAULT_CATALOG)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    result = reconcile(load(args.catalog), load(args.bundle), args.bundle.resolve())
    encoded = json.dumps(result, indent=2, sort_keys=True) + "\n"
    if args.output:
        args.output.write_text(encoded, encoding="utf-8")
    else:
        sys.stdout.write(encoded)
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
