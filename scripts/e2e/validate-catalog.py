#!/usr/bin/env python3
"""Validate the static Axon E2E catalog without executing catalog data."""

from __future__ import annotations

import argparse
import json
import re
import sys
from collections import Counter, defaultdict
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[2]
DEFAULT_CATALOG = ROOT / "tests/e2e/catalog/catalog.json"
CLI_REGISTRY = ROOT / "docs/reference/cli/commands.json"
CROSS_MATRIX = ROOT / "tests/fixtures/cross-surface/operation_matrix.json"
MCP_REGISTRY = ROOT / "docs/reference/mcp/tool-schema.json"
REST_REGISTRY = ROOT / "docs/reference/rest/openapi.json"

CLASSIFICATIONS = {"behavioral_e2e", "contract_only", "unsupported", "out_of_scope"}
SURFACES = {"cli", "mcp", "mcp_task_wire", "http"}
MODES = {"inline", "detached", "streamed", "destructive_plan", "provider_backed"}
FORBIDDEN_KEYS = {
    "command", "commands", "hook", "hooks", "condition", "conditions", "eval",
    "plugin", "plugins", "shell", "script", "template",
}
SCENARIO_KEYS = {
    "id", "capability", "lifecycle", "polarity", "execution_mode",
    "setup_dependencies", "surfaces",
    "tier", "workload_tier", "fixture", "requests", "semantic_oracles",
    "envelope_oracles", "mutable", "resource_classes", "resource_ownership",
    "cleanup_contract",
    "timeout_class", "estimated_seconds", "weights", "provider",
    "setup_sharing_group", "isolation", "shard_eligible", "retry_class", "retry",
    "evidence_kib", "redaction_class", "failure_taxonomy",
    "tags",
}

JSON_TYPES = {
    "object": dict,
    "array": list,
    "string": str,
    "integer": int,
    "boolean": bool,
    "null": type(None),
}


def load(path: Path) -> Any:
    with path.open(encoding="utf-8") as handle:
        return json.load(handle)


def duplicate_values(values: list[str]) -> set[str]:
    return {value for value, count in Counter(values).items() if count > 1}


def schema_errors(value: Any, schema: dict[str, Any], root_schema: dict[str, Any], location: str = "catalog") -> list[str]:
    """Validate the JSON-Schema subset used by catalog.schema.json."""
    if "$ref" in schema:
        target: Any = root_schema
        for component in schema["$ref"].removeprefix("#/").split("/"):
            target = target[component]
        return schema_errors(value, target, root_schema, location)

    errors: list[str] = []
    allowed_types = schema.get("type")
    if allowed_types:
        names = [allowed_types] if isinstance(allowed_types, str) else allowed_types
        valid_type = any(
            isinstance(value, JSON_TYPES[name])
            and not (name == "integer" and isinstance(value, bool))
            for name in names
        )
        if not valid_type:
            return [f"{location}: expected type {names}, got {type(value).__name__}"]
    if "const" in schema and value != schema["const"]:
        errors.append(f"{location}: must equal {schema['const']!r}")
    if "enum" in schema and value not in schema["enum"]:
        errors.append(f"{location}: value {value!r} is not in {schema['enum']}")
    if isinstance(value, str):
        if len(value) < schema.get("minLength", 0):
            errors.append(f"{location}: string is too short")
        if "pattern" in schema and not re.fullmatch(schema["pattern"], value):
            errors.append(f"{location}: does not match required pattern")
    if isinstance(value, int) and not isinstance(value, bool):
        if "minimum" in schema and value < schema["minimum"]:
            errors.append(f"{location}: value is below minimum {schema['minimum']}")
        if "maximum" in schema and value > schema["maximum"]:
            errors.append(f"{location}: value exceeds maximum {schema['maximum']}")
    if isinstance(value, list):
        if len(value) < schema.get("minItems", 0):
            errors.append(f"{location}: array has too few items")
        if schema.get("uniqueItems"):
            encoded = [json.dumps(item, sort_keys=True) for item in value]
            if len(encoded) != len(set(encoded)):
                errors.append(f"{location}: array items must be unique")
        item_schema = schema.get("items")
        if item_schema:
            for index, item in enumerate(value):
                errors.extend(schema_errors(item, item_schema, root_schema, f"{location}[{index}]"))
    if isinstance(value, dict):
        if len(value) < schema.get("minProperties", 0):
            errors.append(f"{location}: object has too few properties")
        properties = schema.get("properties", {})
        for required in schema.get("required", []):
            if required not in value:
                errors.append(f"{location}: missing required property `{required}`")
        additional = schema.get("additionalProperties", True)
        for key, child in value.items():
            if key in properties:
                errors.extend(schema_errors(child, properties[key], root_schema, f"{location}.{key}"))
            elif additional is False:
                errors.append(f"{location}: unknown property `{key}`")
            elif isinstance(additional, dict):
                errors.extend(schema_errors(child, additional, root_schema, f"{location}.{key}"))
    return errors


def walk_forbidden(value: Any, errors: list[str], location: str = "catalog") -> None:
    if isinstance(value, dict):
        for key, child in value.items():
            if key.lower() in FORBIDDEN_KEYS:
                errors.append(f"{location}: forbidden executable key `{key}`")
            walk_forbidden(child, errors, f"{location}.{key}")
    elif isinstance(value, list):
        for index, child in enumerate(value):
            walk_forbidden(child, errors, f"{location}[{index}]")


def registry_records(path: Path, *keys: str) -> list[dict[str, Any]]:
    """Load a complete registry, never turn missing input into an empty surface."""
    value = load(path)
    for key in keys:
        if not isinstance(value, dict) or key not in value:
            raise ValueError(f"{path}: missing registry {'/'.join(keys)}")
        value = value[key]
    if not isinstance(value, list) or not value or not all(isinstance(row, dict) for row in value):
        raise ValueError(f"{path}: registry {'/'.join(keys)} must be a nonempty object array")
    return value


def required_name(row: dict[str, Any], key: str, path: Path) -> str:
    value = row.get(key)
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f"{path}: registry entry requires nonempty {key}")
    return value


def expected_operations() -> set[str]:
    # The human parity table is an output, not a machine-readable input.
    # Preserve literal names: CLI code-search and MCP code_search are distinct
    # existing coverage IDs even though the presentation normalizes spelling.
    cli_rows = registry_records(CLI_REGISTRY, "commands")
    cli = {f"cli:{required_name(item, 'name', CLI_REGISTRY)}" for item in cli_rows}
    cross = {
        f"cross:{required_name(item, 'op', CROSS_MATRIX)}"
        for item in registry_records(CROSS_MATRIX, "operations")
    }
    surface: set[str] = set()
    for item in cli_rows:
        path = item.get("path")
        if not isinstance(path, list) or not path or not all(isinstance(part, str) and part for part in path):
            raise ValueError(f"{CLI_REGISTRY}: command entry has no valid path")
        surface.add(f"surface:{path[0]}")
    for item in registry_records(MCP_REGISTRY, "x-axon", "operations"):
        surface.add(f"surface:{required_name(item, 'action', MCP_REGISTRY)}")
    for item in registry_records(REST_REGISTRY, "routes"):
        path = required_name(item, "path", REST_REGISTRY)
        if path.startswith("/v1/"):
            family = path[4:].split("/", 1)[0]
            if not family:
                raise ValueError(f"{REST_REGISTRY}: route has no /v1 family segment")
            surface.add(f"surface:{family}")
    return cli | cross | surface


def validate(catalog: dict[str, Any]) -> list[str]:
    schema = load(ROOT / "tests/e2e/schema/catalog.schema.json")
    errors = schema_errors(catalog, schema, schema)
    walk_forbidden(catalog, errors)
    # Semantic reconciliation below may rely on schema-guaranteed types. A
    # malformed document is already decisively rejected and must not make the
    # validator itself crash while attempting inventory checks.
    if errors:
        return errors
    if catalog.get("schema_version") != 1:
        errors.append("schema_version must be exactly 1")

    operations = catalog.get("operations")
    scenarios = catalog.get("scenarios")
    if not isinstance(operations, list) or not isinstance(scenarios, list):
        return errors + ["operations and scenarios must be arrays"]

    operation_ids = [item.get("id", "") for item in operations if isinstance(item, dict)]
    duplicates = duplicate_values(operation_ids)
    if duplicates:
        errors.append(f"duplicate operation IDs: {sorted(duplicates)}")
    actual = set(operation_ids)
    try:
        expected = expected_operations()
    except (OSError, ValueError, KeyError, TypeError) as error:
        return errors + [f"authoritative operation inventory unavailable: {error}; regenerate the owning contracts and retry"]
    if missing := expected - actual:
        errors.append(f"unclassified advertised operations: {sorted(missing)}")
    if stale := actual - expected:
        errors.append(f"catalog operations absent from authoritative inventories: {sorted(stale)}")
    for item in operations:
        if not isinstance(item, dict):
            errors.append("operation entries must be objects")
            continue
        if item.get("classification") not in CLASSIFICATIONS:
            errors.append(f"{item.get('id')}: unknown classification")
        if not isinstance(item.get("reason"), str) or not item["reason"].strip():
            errors.append(f"{item.get('id')}: classification reason is required")
        if item.get("classification") == "unsupported" and (
            not isinstance(item.get("owner"), str) or not item["owner"].strip()
        ):
            errors.append(f"{item.get('id')}: unsupported mapping requires an owner")

    scenario_ids = [item.get("id", "") for item in scenarios if isinstance(item, dict)]
    if duplicates := duplicate_values(scenario_ids):
        errors.append(f"duplicate scenario IDs: {sorted(duplicates)}")
    lifecycle_polarities: dict[str, set[str]] = defaultdict(set)
    observed_modes: set[str] = set()
    for item in scenarios:
        if not isinstance(item, dict):
            errors.append("scenario entries must be objects")
            continue
        scenario_id = item.get("id") or "<missing-id>"
        unknown = set(item) - SCENARIO_KEYS
        missing = SCENARIO_KEYS - set(item)
        if unknown:
            errors.append(f"{scenario_id}: unknown fields {sorted(unknown)}")
        if missing:
            errors.append(f"{scenario_id}: missing fields {sorted(missing)}")
        surfaces = item.get("surfaces", [])
        if not surfaces or set(surfaces) - SURFACES:
            errors.append(f"{scenario_id}: surfaces are empty or unknown")
        requests = item.get("requests", {})
        envelopes = item.get("envelope_oracles", {})
        if set(requests) != set(surfaces) or set(envelopes) != set(surfaces):
            errors.append(f"{scenario_id}: every surface needs request and envelope assertions")
        if not item.get("semantic_oracles"):
            errors.append(f"{scenario_id}: shared semantic assertions are required")
        if item.get("mutable") and not item.get("cleanup_contract"):
            errors.append(f"{scenario_id}: mutable scenario requires cleanup_contract")
        for fixture in [item.get("fixture"), *requests.values()]:
            if not isinstance(fixture, str):
                continue
            resolved = (ROOT / fixture).resolve()
            if ROOT not in resolved.parents or not resolved.is_file():
                errors.append(f"{scenario_id}: fixture does not exist inside repository: {fixture}")
        ownership = item.get("resource_ownership", {})
        if item.get("mutable") and ownership.get("strategy") != "run_manifest":
            errors.append(f"{scenario_id}: mutable scenario requires run_manifest ownership")
        mode = item.get("execution_mode")
        if mode not in MODES:
            errors.append(f"{scenario_id}: unknown execution_mode")
        observed_modes.add(mode)
        lifecycle_polarities[item.get("lifecycle", "")].add(item.get("polarity", ""))

    if missing_modes := MODES - observed_modes:
        errors.append(f"representative execution modes missing: {sorted(missing_modes)}")
    policy = catalog.get("coverage_policy", {})
    threshold = policy.get("behavioral_percent", 0)
    behavioral = sum(item.get("classification") == "behavioral_e2e" for item in operations)
    percent = 100 * behavioral / len(operations) if operations else 0
    if threshold < 80 or percent < threshold:
        errors.append(f"behavioral classification {percent:.1f}% is below {threshold}%")
    for lifecycle in policy.get("critical_lifecycles", []):
        if lifecycle_polarities[lifecycle] != {"happy", "negative"}:
            errors.append(f"critical lifecycle `{lifecycle}` requires happy and negative scenarios")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("catalog", nargs="?", type=Path, default=DEFAULT_CATALOG)
    parser.add_argument("--report", action="store_true")
    args = parser.parse_args()
    catalog = load(args.catalog)
    errors = validate(catalog)
    if errors:
        for error in errors:
            print(f"catalog error: {error}", file=sys.stderr)
        return 1
    if args.report:
        counts = Counter(item["classification"] for item in catalog["operations"])
        print(json.dumps({"schema_version": 1, "denominator": sum(counts.values()), "classifications": dict(sorted(counts.items()))}, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
