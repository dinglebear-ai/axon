#!/usr/bin/env python3
"""Canonical Axon E2E execution/evidence report and JUnit projection."""
from __future__ import annotations

import hashlib
import importlib.util
import json
import re
import sys
import xml.etree.ElementTree as ET
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

FAILURE_CLASSES = {"product", "fixture", "provider", "auth_network", "cleanup", "harness"}
TERMINAL = {"passed", "failed", "timed_out", "canceled"}


class ReportingError(RuntimeError): pass


def _load_redaction():
    spec = importlib.util.spec_from_file_location("axon_e2e_reporting_redaction", Path(__file__).with_name("redaction.py"))
    if spec is None or spec.loader is None: raise ReportingError("redaction boundary is unavailable")
    module = importlib.util.module_from_spec(spec); sys.modules[spec.name] = module; spec.loader.exec_module(module); return module


redaction = _load_redaction()


def _bounded_summary(value: str | None) -> str | None:
    if value is None or len(value.encode()) <= 4096: return value
    digest = hashlib.sha256(value.encode()).hexdigest()
    return f"{value[:1536]}\n[TRUNCATED sha256={digest}]\n{value[-1536:]}"


def opaque(value: str) -> str: return hashlib.sha256(value.encode()).hexdigest()[:16]


def evidence_ref(path: Path, root: Path) -> dict[str, Any]:
    resolved, base = path.resolve(strict=True), root.resolve(strict=True)
    if resolved != base and base not in resolved.parents: raise ReportingError("evidence path escaped suite root")
    data = resolved.read_bytes()
    return {"path": resolved.relative_to(base).as_posix(), "sha256": hashlib.sha256(data).hexdigest(), "bytes": len(data)}


@dataclass
class Scenario:
    scenario_id: str
    tier: str
    capability: str
    surface: str
    attempts: list[dict[str, Any]] = field(default_factory=list)
    invariants: list[dict[str, Any]] = field(default_factory=list)
    evidence: list[dict[str, Any]] = field(default_factory=list)
    cleanup: dict[str, Any] = field(default_factory=dict)

    def attempt(self, status: str, duration_ms: int, *, classification: str | None = None,
                summary: str | None = None, namespace: str | None = None,
                serialized: bool | None = None, backoff_ms: int | None = None,
                teardown_verified: bool | None = None) -> None:
        if status not in TERMINAL: raise ReportingError("attempt status is not terminal")
        if status != "passed" and classification not in FAILURE_CLASSES: raise ReportingError("failure classification is required")
        record={"attempt": len(self.attempts) + 1, "status": status, "duration_ms": duration_ms,
                "classification": classification, "summary": _bounded_summary(summary)}
        if namespace is not None:record["namespace"]=namespace
        if serialized is not None:record["serialized"]=serialized
        if backoff_ms is not None:record["backoff_ms"]=backoff_ms
        if teardown_verified is not None:record["teardown_verified"]=teardown_verified
        self.attempts.append(record)

    def record(self) -> dict[str, Any]:
        if not self.attempts: raise ReportingError("scenario has no attempt history")
        cleanup_ok = self.cleanup.get("success") is True and not self.cleanup.get("residual") and not self.cleanup.get("refused")
        effective = self.attempts[-1]["status"]
        if not cleanup_ok: effective = "failed"
        first_failure = next((item for item in self.attempts if item["status"] != "passed"), None)
        return {"scenario_id": self.scenario_id, "tier": self.tier, "capability": self.capability,
                "surface": self.surface, "status": effective, "attempts": self.attempts,
                "first_attempt_failure": first_failure, "invariants": self.invariants,
                "evidence": self.evidence, "cleanup": sanitize_cleanup(self.cleanup)}


def sanitize_cleanup(report: dict[str, Any]) -> dict[str, Any]:
    residuals = []
    for key in ("refused", "residual"):
        for item in report.get(key, []):
            residuals.append({"class": item.get("class", "unknown"),
                              "opaque_id": item.get("opaque_id") or opaque(str(item.get("identity", "unknown"))),
                              "reason_class": "cleanup"})
    phases = [{key: item[key] for key in ("name", "count", "duration_ms", "timed_out") if key in item}
              for item in report.get("phases", [])]
    return {"success": report.get("success") is True and not residuals,
            "manifest_digest": report.get("manifest_digest"), "residuals": residuals,
            "classes": report.get("classes", {}), "phases": phases}


def suite_report(scenarios: list[Scenario], *, tested_sha: str, provider_versions: dict[str, str],
                 policy: dict[str, Any], upload: dict[str, Any] | None = None) -> dict[str, Any]:
    if not isinstance(tested_sha, str) or len(tested_sha) != 40: raise ReportingError("tested SHA must be full length")
    if not scenarios: raise ReportingError("canonical report requires at least one scenario")
    records = sorted((scenario.record() for scenario in scenarios), key=lambda item: (item["scenario_id"], item["surface"]))
    failures = sum(item["status"] != "passed" for item in records)
    report = {"schema": 1, "tested_sha": tested_sha, "provider_versions": dict(sorted(provider_versions.items())),
              "policy": policy, "scenarios": records,
              "timing": {"total_ms": sum(sum(a["duration_ms"] for a in item["attempts"]) for item in records),
                         "scenario_count": len(records)},
              "summary": {"passed": len(records) - failures, "failed": failures,
                          "status": "passed" if failures == 0 else "failed"},
              "upload": upload or {"status": "not_attempted", "local_evidence_path": None}}
    report["report_sha256"] = hashlib.sha256(json.dumps(report, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
    return report


def validate_report(report: dict[str, Any]) -> None:
    required = {"schema", "tested_sha", "provider_versions", "policy", "scenarios", "timing", "summary", "upload", "report_sha256"}
    if set(report) != required or report["schema"] != 1: raise ReportingError("canonical report fields changed")
    if not re.fullmatch(r"[0-9a-f]{40}", str(report["tested_sha"])): raise ReportingError("tested SHA is invalid")
    unsigned = {key: value for key, value in report.items() if key != "report_sha256"}
    expected = hashlib.sha256(json.dumps(unsigned, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
    if report["report_sha256"] != expected: raise ReportingError("report provenance digest mismatch")
    if not isinstance(report["scenarios"], list) or not report["scenarios"]:
        raise ReportingError("canonical report requires at least one scenario")
    total_ms = 0
    passed = failed = 0
    for scenario in report["scenarios"]:
        attempts = scenario.get("attempts", [])
        if not attempts or [item.get("attempt") for item in attempts] != list(range(1, len(attempts) + 1)):
            raise ReportingError("attempt history is incomplete or reordered")
        for attempt in attempts:
            if attempt.get("status") not in TERMINAL: raise ReportingError("invalid terminal status")
            if not isinstance(attempt.get("duration_ms"), int) or attempt["duration_ms"] < 0:
                raise ReportingError("attempt timing is invalid")
            if attempt["status"] != "passed" and attempt.get("classification") not in FAILURE_CLASSES:
                raise ReportingError("invalid failure taxonomy")
        cleanup = scenario.get("cleanup", {})
        cleanup_fields = {"success", "manifest_digest", "residuals", "classes", "phases"}
        if not isinstance(cleanup, dict) or set(cleanup) != cleanup_fields:
            raise ReportingError("cleanup audit shape is invalid")
        residuals = cleanup.get("residuals")
        if not isinstance(residuals, list): raise ReportingError("cleanup residuals are invalid")
        for residual in residuals:
            if (not isinstance(residual, dict)
                    or set(residual) != {"class", "opaque_id", "reason_class"}
                    or not isinstance(residual["class"], str)
                    or re.fullmatch(r"[0-9a-f]{16}", str(residual["opaque_id"])) is None
                    or residual["reason_class"] != "cleanup"):
                raise ReportingError("cleanup residual shape is invalid")
        if cleanup.get("success") is not (not residuals):
            raise ReportingError("cleanup success disagrees with residuals")
        if cleanup["manifest_digest"] is not None and re.fullmatch(r"[0-9a-f]{64}", str(cleanup["manifest_digest"])) is None:
            raise ReportingError("cleanup manifest digest is invalid")
        if not isinstance(cleanup["classes"], dict) or not isinstance(cleanup["phases"], list):
            raise ReportingError("cleanup classes or phases are invalid")
        expected_status = attempts[-1]["status"] if cleanup["success"] else "failed"
        if scenario.get("status") != expected_status: raise ReportingError("scenario status disagrees with attempts or cleanup")
        expected_first = next((item for item in attempts if item["status"] != "passed"), None)
        if scenario.get("first_attempt_failure") != expected_first: raise ReportingError("first-attempt failure evidence disagrees with attempts")
        total_ms += sum(item["duration_ms"] for item in attempts)
        if expected_status == "passed": passed += 1
        else: failed += 1
    expected_summary = {"passed": passed, "failed": failed, "status": "passed" if failed == 0 else "failed"}
    if report.get("summary") != expected_summary: raise ReportingError("aggregate summary disagrees with scenario results")
    if report.get("timing") != {"total_ms": total_ms, "scenario_count": len(report["scenarios"])}:
        raise ReportingError("aggregate timing disagrees with attempt results")


def write_json(report: dict[str, Any], path: Path, *, secrets: tuple[str, ...] = ()) -> None:
    validate_report(report)
    encoded = json.dumps(report, indent=2, sort_keys=True) + "\n"
    redaction.scan_bytes(encoded.encode(), secrets)
    path.parent.mkdir(parents=True, exist_ok=True); path.write_text(encoded)


def write_junit(report: dict[str, Any], path: Path, *, secrets: tuple[str, ...] = ()) -> None:
    validate_report(report)
    root = ET.Element("testsuite", name="axon-e2e", tests=str(len(report["scenarios"])),
                      failures=str(report["summary"]["failed"]), time=f'{report["timing"]["total_ms"] / 1000:.3f}')
    ET.SubElement(root, "properties")
    for item in report["scenarios"]:
        case = ET.SubElement(root, "testcase", classname=item["capability"], name=f'{item["scenario_id"]}[{item["surface"]}]',
                             time=f'{sum(a["duration_ms"] for a in item["attempts"]) / 1000:.3f}')
        first = item["first_attempt_failure"]
        if item["status"] != "passed":
            classification = "cleanup" if not item["cleanup"]["success"] else (first or {}).get("classification", "harness")
            failure = ET.SubElement(case, "failure", type=classification, message=(first or {}).get("summary") or classification)
            failure.text = json.dumps({"attempts": item["attempts"], "cleanup": item["cleanup"]}, sort_keys=True)
        ET.SubElement(case, "system-out").text = json.dumps({"attempts": item["attempts"], "first_attempt_failure": first,
                                                               "cleanup": item["cleanup"], "evidence": item["evidence"],
                                                               "invariants": item["invariants"]}, sort_keys=True)
    encoded = ET.tostring(root, encoding="unicode", xml_declaration=True)
    redaction.scan_bytes(encoded.encode(), secrets)
    path.parent.mkdir(parents=True, exist_ok=True); path.write_text(encoded)
