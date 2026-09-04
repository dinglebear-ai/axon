#!/usr/bin/env python3
"""Stable, reporting-first performance contract for Axon E2E measurements."""
from __future__ import annotations

import hashlib
import json
import math
import statistics
from pathlib import Path
from typing import Any

SCHEMA = "axon-e2e-performance/v1"
MODES = {"cold", "warm"}
STATUSES = {"measured", "unsupported", "censored"}
ATTRIBUTIONS = {"axon", "provider", "infrastructure"}
REQUIRED_METRICS = (
    "cold_start_ms", "warm_start_ms", "source_to_terminal_ms", "embedding_throughput_items_s",
    "embedding_batch_utilization_ratio", "vector_publication_ms", "retrieval_ms",
    "http_first_response_ms", "mcp_first_response_ms", "progress_first_observed_ms",
    "sqlite_growth_bytes", "peak_rss_bytes", "peak_process_count", "cleanup_ms", "llm_ms",
    "retrieval_context_ms",
)
FINGERPRINT_KEYS = {
    "machine": ("runner_class", "os", "arch", "cpu", "memory_bytes", "power_mode", "thermal_state"),
    "provider": ("provider_versions", "model_versions", "endpoint_class"),
    "scenario": ("corpus_version", "corpus_digest", "config_digest", "workload_cardinality", "concurrency", "queue_depth"),
}


class PerformanceError(AssertionError):
    pass


def classify_censor(message: str, *, timed_out: bool = False) -> str:
    lowered = message.casefold()
    if timed_out or any(token in lowered for token in ("did not become ready", "connection refused", "resource pressure")):
        return "infrastructure"
    if any(token in lowered for token in ("provider", "429", "503", "model unavailable")):
        return "provider"
    return "product"


def censored_report(tested_sha: str, attempts: int, censored: list[dict[str, Any]]) -> dict[str, Any]:
    _require(len(tested_sha) == 40 and attempts >= 1 and censored, "censored report provenance is incomplete")
    classes = {item.get("classification") for item in censored}
    classification = "infrastructure" if "infrastructure" in classes else "provider" if classes == {"provider"} else "product"
    return {"schema": "axon-e2e-performance-censored/v1", "tested_sha": tested_sha, "status": "censored",
            "classification": classification, "attempts": attempts, "censored": censored[:50], "correctness_retries": 0}


def _require(condition: bool, message: str) -> None:
    if not condition:
        raise PerformanceError(message)


def percentile(values: list[float], quantile: float) -> float:
    ordered = sorted(values)
    _require(bool(ordered), "percentile requires samples")
    position = (len(ordered) - 1) * quantile
    lower, upper = math.floor(position), math.ceil(position)
    if lower == upper:
        return ordered[lower]
    return ordered[lower] + (ordered[upper] - ordered[lower]) * (position - lower)


def summarize(values: list[float], warmup_discarded: int, timeout_ms: int) -> dict[str, Any]:
    _require(values and all(isinstance(item, (int, float)) and item >= 0 for item in values), "samples must be nonnegative numbers")
    bounded = values[:50]
    median = statistics.median(bounded)
    mean = statistics.fmean(bounded)
    deviation = statistics.pstdev(bounded) if len(bounded) > 1 else 0.0
    return {
        "count": len(bounded), "warmup_discarded": warmup_discarded, "timeout_ms": timeout_ms,
        "min": min(bounded), "p50": median, "p90": percentile(bounded, .90),
        "p95": percentile(bounded, .95), "max": max(bounded), "mean": mean,
        "cv": deviation / mean if mean else 0.0,
    }


def fingerprint_digest(fingerprint: dict[str, Any]) -> str:
    validate_fingerprint(fingerprint)
    return hashlib.sha256(json.dumps(fingerprint, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def validate_fingerprint(value: dict[str, Any]) -> None:
    _require(set(value) == set(FINGERPRINT_KEYS), "fingerprint must separate machine/provider/scenario buckets")
    for bucket, keys in FINGERPRINT_KEYS.items():
        _require(all(key in value[bucket] for key in keys), f"fingerprint {bucket} bucket is incomplete")


def validate_report(report: dict[str, Any]) -> None:
    _require(report.get("schema") == SCHEMA, "unknown performance schema")
    _require(isinstance(report.get("tested_sha"), str) and len(report["tested_sha"]) == 40, "tested SHA is invalid")
    validate_fingerprint(report.get("fingerprint", {}))
    _require(report.get("fingerprint_sha256") == fingerprint_digest(report["fingerprint"]), "fingerprint digest drift")
    _require(report.get("policy", {}).get("exclusive_group") and report["policy"].get("correctness_retries") == 0,
             "performance policy must be exclusive and cannot retry correctness")
    _require(report["policy"].get("baseline_retention") and report["policy"].get("timeout_censoring") == "record",
             "retention and timeout censoring policy are required")
    _require(report["policy"].get("minimum_promotion_samples", 0) >= 5, "promotion sample minimum is unsafe")
    metrics = report.get("metrics", [])
    _require({item.get("id") for item in metrics} == set(REQUIRED_METRICS), "locked metric inventory is incomplete")
    for metric in metrics:
        _require(metric.get("status") in STATUSES and metric.get("attribution") in ATTRIBUTIONS, "metric status/attribution is invalid")
        if metric["status"] == "measured":
            _require(metric.get("mode") in MODES and metric.get("unit") and metric.get("samples"), "measured metric is incomplete")
            expected = summarize(metric["samples"], metric.get("warmup_discarded", 0), metric["timeout_ms"])
            _require(metric.get("summary") == expected, "metric summary does not match raw samples")
        else:
            _require(bool(metric.get("reason")), "unsupported/censored metric needs a reason")
    _require(report.get("contention", {}).get("exclusive_acquired") is True, "exclusive contention group was not acquired")
    contention = report["contention"]
    minimum_samples = report["policy"]["minimum_promotion_samples"]
    measured = [item for item in metrics if item["status"] == "measured"]
    supported_count = contention.get("supported_metrics", len(measured))
    minimum_supported = contention.get("minimum_supported_metrics", 0)
    blockers = {
        "pressure": contention.get("pressure_detected") is True,
        "censoring": bool(report.get("censored")),
        "samples": any(item["summary"]["count"] < minimum_samples for item in measured),
        "supported_metrics": supported_count < minimum_supported,
    }
    eligible = contention.get("baseline_eligible") is True
    _require(not eligible or not any(blockers.values()), "contention or measurement prerequisites contradict baseline eligibility")
    _require(eligible or any(blockers.values()), "measurement is ineligible without an unmet prerequisite")
    if eligible:
        _require(all(item["status"] != "measured" or item["summary"]["count"] >= 5 for item in metrics),
                 "baseline-eligible report contains an undersampled measured metric")
    _require(report.get("cleanup", {}).get("success") is True and not report["cleanup"].get("residual"), "benchmark cleanup is incomplete")
    _require(report.get("redaction", {}).get("scanned") is True and report["redaction"].get("oracle") == "observe.redaction",
             "benchmark evidence did not cross the canonical redaction boundary")
    _require(isinstance(report.get("evidence"), list) and len(report["evidence"]) <= 32, "evidence must be bounded")


def comparable(left: dict[str, Any], right: dict[str, Any]) -> tuple[bool, list[str]]:
    mismatches = []
    for bucket, keys in FINGERPRINT_KEYS.items():
        for key in keys:
            if left["fingerprint"][bucket][key] != right["fingerprint"][bucket][key]:
                mismatches.append(f"{bucket}.{key}")
    return not mismatches, mismatches


def validate_budgets(config: dict[str, Any]) -> None:
    _require(config.get("schema") == "axon-e2e-performance-budgets/v1", "unknown budget schema")
    promotion = config.get("promotion", {})
    for budget in config.get("budgets", []):
        if budget.get("state") == "gate":
            _require(config.get("mode") == "gating", "gate budget requires gating mode")
            _require(bool(budget.get("owner_approval")), "gate budget lacks owner approval")
            _require(budget.get("baseline_count", 0) >= promotion.get("minimum_baselines", 10), "gate lacks baseline history")
            _require(budget.get("sample_count", 0) >= promotion.get("minimum_samples_per_mode", 5), "gate lacks samples")
            _require(budget.get("baseline_cv", 1.0) <= promotion.get("maximum_cv", .15), "gate variance is unstable")


def compare(current: dict[str, Any], baseline: dict[str, Any], budgets: dict[str, Any]) -> dict[str, Any]:
    validate_report(current); validate_report(baseline); validate_budgets(budgets)
    gates = {item["metric"]: item for item in budgets.get("budgets", []) if item.get("state") == "gate"}
    minimum_samples = max(5, budgets.get("promotion", {}).get("minimum_samples_per_mode", 5))
    for metric_id in gates:
        metric = next((item for item in current["metrics"] if item["id"] == metric_id), None)
        previous = next((item for item in baseline["metrics"] if item["id"] == metric_id), None)
        _require(metric is not None and metric.get("summary", {}).get("count", 0) >= minimum_samples,
                 f"promoted metric {metric_id} lacks measured candidate samples")
        _require(previous is not None and previous.get("summary", {}).get("count", 0) >= minimum_samples,
                 f"promoted metric {metric_id} lacks measured baseline samples")
    if not current["contention"]["baseline_eligible"] or not baseline["contention"]["baseline_eligible"]:
        return {"status": "incomparable", "classification": "infrastructure", "mismatches": ["contention.pressure"], "deltas": []}
    valid, mismatches = comparable(current, baseline)
    if not valid:
        return {"status": "incomparable", "classification": "infrastructure", "mismatches": mismatches, "deltas": []}
    baseline_metrics = {item["id"]: item for item in baseline["metrics"]}
    deltas = []
    for metric in current["metrics"]:
        previous = baseline_metrics.get(metric["id"])
        if metric["status"] != "measured" or not previous or previous["status"] != "measured": continue
        before, after = previous["summary"]["p95"], metric["summary"]["p95"]
        deltas.append({"metric": metric["id"], "baseline_p95": before, "current_p95": after,
                       "change_ratio": (after - before) / before if before else 0.0,
                       "attribution": metric["attribution"]})
    failures = [item for item in deltas if item["metric"] in gates and item["change_ratio"] > gates[item["metric"]]["max_regression_ratio"]]
    classification = ("provider" if all(item["attribution"] == "provider" for item in failures) else "product") if failures else None
    return {"status": "regressed" if failures else "reported", "classification": classification,
            "mismatches": [], "deltas": deltas, "gate_failures": failures}


def release_projection(report: dict[str, Any], comparison: dict[str, Any] | None = None) -> dict[str, Any]:
    validate_report(report)
    return {"schema": SCHEMA, "tested_sha": report["tested_sha"], "fingerprint_sha256": report["fingerprint_sha256"],
            "status": (comparison or {}).get("status", "baseline" if report["contention"]["baseline_eligible"] else "measurement_ineligible"), "comparison": comparison,
            "metrics": [{"id": item["id"], "status": item["status"], "summary": item.get("summary"),
                         "reason": item.get("reason")} for item in report["metrics"]]}


def write_report(path: Path, report: dict[str, Any]) -> None:
    validate_report(report)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
