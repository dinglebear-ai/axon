#!/usr/bin/env python3
"""Run fail-closed security observations emitted by real transport adapters."""
from __future__ import annotations

import argparse
import importlib.util
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location("security_pack", HERE / "security_pack.py")
security = importlib.util.module_from_spec(SPEC); SPEC.loader.exec_module(security)


def evaluate(record: dict, evidence_root: Path, secrets: list[str]) -> dict:
    failures = []
    execution = record.get("execution", {})
    if execution.get("axon_invoked") is not True or execution.get("adapter_clients") != ["http", "mcp_http", "mcp_stdio"]:
        failures.append("real Axon and all required adapter clients were not attested")
    manifest_path = execution.get("manifest")
    if not isinstance(manifest_path, str) or not Path(manifest_path).is_file():
        failures.append("integrity-protected ownership manifest is missing")
    observed = record.get("auth_observations", [])
    observed_keys = {(item.get("surface"), item.get("route")) for item in observed}
    required_keys = {(surface, route) for surface, route, *_ in security.AUTH_MATRIX}
    if required_keys - observed_keys:
        failures.append(f"auth matrix missing: {sorted(required_keys - observed_keys)}")
    for item in observed:
        try: security.validate_auth_observation(item)
        except security.SecurityError as error: failures.append(str(error))
    probes = record.get("ssrf_observations", [])
    if {item.get("url") for item in probes} != set(security.SSRF_CASES):
        failures.append("SSRF alternate-form matrix is incomplete")
    for item in probes:
        classification = security.forbidden_destination(item.get("url", ""), record.get("dns"))
        try: security.assert_zero_connections(item.get("connections_before", -1),
                                              item.get("connections_after", -2), classification)
        except security.SecurityError as error: failures.append(str(error))
    providers = record.get("provider_observations", [])
    required_provider_cases = {"qdrant.collection", "qdrant.alias", "qdrant.snapshot",
                               "qdrant.admin", "qdrant.enumeration", "chrome.profile",
                               "chrome.session", "chrome.admin", "chrome.enumeration"}
    if {item.get("name") for item in providers} != required_provider_cases:
        failures.append("provider-boundary execution matrix is incomplete")
    for item in providers:
        outcome = security.provider_boundary(**item["request"])
        if outcome != item.get("classification") or outcome == "allowed":
            failures.append(f"provider boundary did not reject {item.get('name')}")
    try: security.scan_tree(evidence_root, secrets)
    except security.SecurityError as error: failures.append(str(error))
    return {"schema_version": 1, "passed": not failures, "failures": failures,
            "auth_cases": len(observed), "ssrf_cases": len(probes)}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--observations", type=Path, required=True)
    parser.add_argument("--evidence-root", type=Path, required=True)
    parser.add_argument("--canary", action="append", required=True)
    args = parser.parse_args()
    result = evaluate(json.loads(args.observations.read_text()), args.evidence_root, args.canary)
    print(json.dumps(result, sort_keys=True)); return 0 if result["passed"] else 1


if __name__ == "__main__": raise SystemExit(main())
