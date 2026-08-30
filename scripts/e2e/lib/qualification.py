#!/usr/bin/env python3
"""Deterministic, fail-closed release qualification projection."""
from __future__ import annotations

import hashlib
import importlib.util
import json
import re
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

FAMILIES = {"hermetic", "live", "platform", "upgrade", "sensitivity", "reliability",
            "observability", "performance", "cleanup", "security", "deployed_compatibility"}
STATES = {"passed", "failed", "unavailable", "not_run", "not_applicable"}
REQUIREMENTS = {"required", "optional", "not_applicable"}
SHA256 = re.compile(r"[0-9a-f]{64}")
GIT_SHA = re.compile(r"[0-9a-f]{40}")
SAFE_PATH = re.compile(r"[A-Za-z0-9_.-]+(?:/[A-Za-z0-9_.-]+)*")
FORBIDDEN_KEYS = re.compile(r"(?i)^(?:api_?key|api_?token|access_?token|refresh_?token|password|client_?secret|raw_content|database_snapshot|tailnet_(?:ip|metadata)|private_host(?:name)?)$")
URL_OR_PRIVATE_IP = re.compile(r"https?://|(?<![0-9])(?:10\.|127\.|169\.254\.|192\.168\.|172\.(?:1[6-9]|2[0-9]|3[01])\.)")


class QualificationError(RuntimeError): pass


def require(value: bool, message: str) -> None:
    if not value: raise QualificationError(message)


def canonical(value: Any) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()


def digest(value: bytes) -> str: return hashlib.sha256(value).hexdigest()


def parse_time(value: Any) -> datetime:
    require(isinstance(value, str) and value.endswith("Z"), "timestamp must be UTC RFC3339")
    try: parsed = datetime.fromisoformat(value[:-1] + "+00:00")
    except ValueError as error: raise QualificationError("timestamp is invalid") from error
    require(parsed.tzinfo is not None, "timestamp timezone is missing")
    return parsed.astimezone(timezone.utc)


def load_json(path: Path) -> dict[str, Any]:
    try: value = json.loads(path.read_text())
    except (OSError, json.JSONDecodeError) as error: raise QualificationError(f"invalid JSON evidence: {path.name}") from error
    require(isinstance(value, dict), "evidence root must be an object")
    return value


def _validate_canonical_report(value: dict[str, Any]) -> str:
    module_path = Path(__file__).with_name("reporting.py")
    spec = importlib.util.spec_from_file_location("axon_qualification_reporting", module_path)
    require(spec is not None and spec.loader is not None, "canonical reporting validator unavailable")
    module = importlib.util.module_from_spec(spec); sys.modules[spec.name] = module; spec.loader.exec_module(module)
    try: module.validate_report(value)
    except Exception as error: raise QualificationError(f"canonical report invalid: {error}") from error
    return value["summary"]["status"]


def _result(value: dict[str, Any], evidence_format: str) -> str:
    if evidence_format == "canonical-report": return _validate_canonical_report(value)
    if evidence_format == "performance-projection":
        status = value.get("status")
        require(status in {"reported", "baseline", "measurement_ineligible", "regressed", "incomparable"}, "performance result invalid")
        return "passed" if status in {"reported", "baseline"} else "failed"
    require(evidence_format == "qualification-record", "unknown evidence format")
    result = value.get("release_qualification", {}).get("result")
    require(result in {"pass", "fail", "unavailable", "not_run"}, "qualification result missing")
    return {"pass": "passed", "fail": "failed", "unavailable": "unavailable", "not_run": "not_run"}[result]


def _projection(value: dict[str, Any], evidence_format: str) -> dict[str, Any]:
    if evidence_format == "canonical-report":
        cleanups = [item.get("cleanup", {}) for item in value.get("scenarios", [])]
        return {"summary": value["summary"], "provider_versions": value.get("provider_versions", {}),
                "scenario_ids": sorted(item["scenario_id"] for item in value["scenarios"]),
                "teardown": {"success": all(item.get("success") is True for item in cleanups),
                             "manifest_digests": sorted({item.get("manifest_digest") for item in cleanups if item.get("manifest_digest")})}}
    if evidence_format == "performance-projection":
        return {"status": value["status"], "fingerprint_sha256": value.get("fingerprint_sha256"),
                "metrics": value.get("metrics", []), "comparison": value.get("comparison")}
    projection = value.get("release_qualification", {}).get("projection", {})
    require(isinstance(projection, dict) and len(canonical(projection)) <= 16384, "qualification projection missing or over limit")
    return projection


def _no_sensitive_keys(value: Any, path: str = "$") -> None:
    if isinstance(value, dict):
        for key, child in value.items():
            require(not FORBIDDEN_KEYS.search(str(key)), f"forbidden evidence field at {path}.{key}")
            _no_sensitive_keys(child, f"{path}.{key}")
    elif isinstance(value, list):
        for index, child in enumerate(value): _no_sensitive_keys(child, f"{path}[{index}]")
    elif isinstance(value, str): require(not URL_OR_PRIVATE_IP.search(value), f"private endpoint-like value at {path}")


def build(index: dict[str, Any], policy: dict[str, Any], root: Path) -> tuple[dict[str, Any], str]:
    require(index.get("schema") == 1 and policy.get("schema") == 1, "schema version unsupported")
    profile = index.get("profile"); require(profile in policy.get("profiles", {}), "qualification profile unknown")
    require(index.get("policy_version") == policy.get("policy_version"), "policy version mismatch")
    tested_sha = index.get("tested_sha"); require(isinstance(tested_sha, str) and GIT_SHA.fullmatch(tested_sha), "tested SHA invalid")
    as_of = parse_time(index.get("as_of")); subject = index.get("subject", {})
    require(subject.get("tested_sha") == tested_sha, "subject SHA mismatch")
    require(isinstance(subject.get("product_version"), str) and subject["product_version"], "product version missing")
    for key in ("catalog_sha256", "corpus_sha256"):
        require(isinstance(subject.get(key), str) and SHA256.fullmatch(subject[key]), f"{key} invalid")
    require(isinstance(subject.get("catalog_version"), (str, int)), "catalog version missing")
    require(isinstance(subject.get("corpus_version"), str), "corpus version missing")
    sources = subject.get("sources", {})
    for name, expected_version_key, actual_version_key in (("catalog", "catalog_version", "schema_version"), ("corpus", "corpus_version", "corpus_version")):
        source = sources.get(name, {}); relative = source.get("path")
        require(isinstance(relative, str) and SAFE_PATH.fullmatch(relative), f"{name} source path invalid")
        source_path = (root / relative).resolve(); require(root.resolve() in source_path.parents, f"{name} source escaped evidence root")
        source_bytes = source_path.read_bytes(); source_digest = digest(source_bytes)
        require(source.get("sha256") == source_digest == subject[f"{name}_sha256"], f"{name} source digest mismatch")
        source_value = load_json(source_path)
        require(str(source_value.get(actual_version_key)) == str(subject[expected_version_key]), f"{name} version mismatch")
    requirements = policy["profiles"][profile]["families"]
    require(set(requirements) == FAMILIES and set(requirements.values()) <= REQUIREMENTS, "profile family policy incomplete")
    exception = index.get("outage_exception")
    if exception is not None:
        require(exception in policy.get("approved_outage_exceptions", []), "outage exception is not policy-approved")
    artifacts = index.get("artifacts"); require(isinstance(artifacts, list), "artifact index missing")
    seen_ids: set[str] = set(); family_records: dict[str, list[dict[str, Any]]] = {family: [] for family in FAMILIES}
    projections: list[dict[str, Any]] = []
    max_age = int(policy["max_evidence_age_seconds"])
    for item in artifacts:
        require(isinstance(item, dict), "artifact descriptor invalid")
        artifact_id, family = item.get("id"), item.get("family")
        require(isinstance(artifact_id, str) and artifact_id not in seen_ids, "artifact id missing or duplicated"); seen_ids.add(artifact_id)
        require(family in FAMILIES, "artifact family unknown")
        relative = item.get("path"); require(isinstance(relative, str) and SAFE_PATH.fullmatch(relative) and not relative.startswith("/"), "artifact path unsafe")
        path = (root / relative).resolve(); require(root.resolve() in path.parents, "artifact escaped evidence root")
        data = path.read_bytes(); actual = digest(data)
        require(item.get("sha256") == actual and SHA256.fullmatch(actual), f"artifact digest mismatch: {artifact_id}")
        require(item.get("bytes") == len(data) and len(data) <= 524288, f"artifact size mismatch or over limit: {artifact_id}")
        redaction = item.get("redaction_class"); require(redaction in policy["allowed_redaction_classes"], "artifact redaction class rejected")
        retention = item.get("retention", {})
        require(retention.get("location") in policy["allowed_retention_locations"] and isinstance(retention.get("days"), int) and retention["days"] > 0, "artifact retention invalid")
        producer = item.get("producer", {})
        require(producer.get("tested_sha") == tested_sha, "producer tested SHA mismatch")
        require(isinstance(producer.get("workflow"), str) and producer["workflow"], "producer workflow missing")
        require(isinstance(producer.get("repository"), str) and re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", producer["repository"]), "producer repository invalid")
        require(isinstance(producer.get("ref"), str) and producer["ref"].startswith("refs/"), "producer ref invalid")
        require(isinstance(producer.get("run_id"), str) and producer["run_id"].isdigit(), "producer run id invalid")
        require(isinstance(producer.get("run_attempt"), int) and producer["run_attempt"] > 0, "producer run attempt invalid")
        completed = parse_time(producer.get("completed_at")); require(completed <= as_of, "evidence is from the future")
        require((as_of - completed).total_seconds() <= max_age, "evidence is stale")
        redaction_module_path = Path(__file__).with_name("redaction.py")
        redaction_spec = importlib.util.spec_from_file_location("axon_qualification_redaction", redaction_module_path)
        require(redaction_spec is not None and redaction_spec.loader is not None, "redaction validator unavailable")
        redaction_module = importlib.util.module_from_spec(redaction_spec); redaction_spec.loader.exec_module(redaction_module)
        try: redaction_module.scan_bytes(data, ())
        except Exception as error: raise QualificationError(f"artifact redaction validation failed: {artifact_id}") from error
        value = load_json(path); require(value.get("tested_sha") == tested_sha, "evidence tested SHA mismatch")
        _no_sensitive_keys(value)
        state = _result(value, item.get("format"))
        claim = _projection(value, item.get("format")); _no_sensitive_keys(claim)
        require(len(canonical(claim)) <= 65536, "artifact projection exceeds bound")
        record = {"artifact_id": artifact_id, "state": state, "projection": claim}
        family_records[family].append(record)
        projections.append({"id": artifact_id, "family": family, "path": relative, "sha256": actual, "bytes": len(data),
                            "redaction_class": redaction, "retention": retention,
                            "producer": producer, "format": item["format"]})
    families = []
    incomplete = failed = False
    na_rationales = index.get("not_applicable", {})
    for family in sorted(FAMILIES):
        requirement, records = requirements[family], family_records[family]
        if requirement == "not_applicable":
            require(not records, f"not-applicable family has evidence: {family}")
            rationale = na_rationales.get(family)
            require(isinstance(rationale, str) and 10 <= len(rationale) <= 240, f"not-applicable rationale missing: {family}")
            state = "not_applicable"
        elif not records:
            state, rationale = "not_run", None
            if requirement == "required": incomplete = True
        else:
            states = {item["state"] for item in records}; rationale = None
            state = "failed" if "failed" in states else ("unavailable" if "unavailable" in states else ("not_run" if "not_run" in states else "passed"))
            if requirement == "required" and state in {"unavailable", "not_run"}: incomplete = True
            if state == "failed": failed = True
        families.append({"family": family, "requirement": requirement, "state": state,
                         "rationale": rationale, "evidence": sorted(records, key=lambda x: x["artifact_id"])})
    outcome = "failed" if failed else ("incomplete" if incomplete else "passed")
    coverage = index.get("coverage", {})
    require(isinstance(coverage.get("capabilities"), list) and isinstance(coverage.get("surfaces"), list), "coverage projection missing")
    require(all(isinstance(item, str) and 0 < len(item) <= 80 for item in coverage["capabilities"] + coverage["surfaces"]), "coverage identifiers invalid")
    require(isinstance(coverage.get("catalog_covered"), int) and isinstance(coverage.get("catalog_total"), int) and 0 <= coverage["catalog_covered"] <= coverage["catalog_total"], "catalog coverage counts invalid")
    manifest = {"schema": 1, "kind": "axon-e2e-release-qualification", "profile": profile,
                "policy": {"version": policy["policy_version"], "outage_exception": exception},
                "as_of": index["as_of"], "subject": subject,
                "coverage": {"capabilities": sorted(set(coverage["capabilities"])), "surfaces": sorted(set(coverage["surfaces"])),
                             "catalog_covered": int(coverage.get("catalog_covered", 0)), "catalog_total": int(coverage.get("catalog_total", 0))},
                "families": families, "artifacts": sorted(projections, key=lambda x: x["id"]),
                "qualification": {"outcome": outcome, "unsigned": True, "release_eligible": False,
                                  "reason": "unsigned evidence projection; signing is a separate approved-identity operation"},
                "exclusions": ["credentials", "private hostnames and addresses", "raw source content", "database snapshots", "unnecessary tailnet metadata"]}
    _no_sensitive_keys(manifest)
    return manifest, digest(canonical(manifest))


def summary(manifest: dict[str, Any], manifest_sha256: str) -> str:
    lines = ["# Axon E2E qualification", "", f"- Profile: `{manifest['profile']}`",
             f"- Tested SHA: `{manifest['subject']['tested_sha']}`", f"- Outcome: **{manifest['qualification']['outcome']}**",
             "- Attestation: **unsigned / not release eligible**", f"- Manifest SHA-256: `{manifest_sha256}`", "", "| Family | Requirement | State |", "|---|---|---|"]
    lines += [f"| {item['family']} | {item['requirement']} | {item['state']} |" for item in manifest["families"]]
    return "\n".join(lines) + "\n"
