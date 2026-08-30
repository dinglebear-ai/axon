#!/usr/bin/env python3
"""Transport-neutral oracles for Axon's end-to-end observability contract.

Inputs are sanitized captures from the public transports plus rows read from the
owned run's SQLite database.  The module deliberately emits the canonical
``Scenario.invariants`` shape instead of defining another report format.
"""
from __future__ import annotations

import importlib.util
import json
import sqlite3
import sys
from datetime import datetime
from pathlib import Path
from typing import Any

ORACLE_IDS = (
    "observe.correlation",
    "observe.event_order_cardinality",
    "observe.retry_causality",
    "observe.provider_health",
    "observe.terminal_agreement",
    "observe.redaction",
    "observe.timing_reconciliation",
)
TERMINAL = {"completed", "completed_degraded", "failed", "canceled"}
FAILURE_CLASSES = {"product", "provider", "auth/network"}
PHASE_ORDER = {name: index for index, name in enumerate((
    "queued", "requested", "resolving", "routing", "authorizing", "planning", "leasing",
    "discovering", "diffing", "fetching", "rendering", "enriching", "normalizing", "parsing",
    "graphing", "preparing", "batching", "embedding", "vectorizing", "upserting", "retrieving",
    "synthesizing", "evaluating", "publishing", "cleaning", "complete", "canceled"
))}


class ObservabilityFailure(AssertionError):
    pass


def _redaction_module():
    path = Path(__file__).with_name("redaction.py")
    spec = importlib.util.spec_from_file_location("axon_e2e_observe_redaction", path)
    if spec is None or spec.loader is None:
        raise ObservabilityFailure("redaction boundary is unavailable")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


redaction = _redaction_module()


def load_runtime(db_path: Path, job_id: str, provider_ids: tuple[str, ...] = ()) -> dict[str, Any]:
    """Read authoritative observe rows for exactly one owned job."""
    connection = sqlite3.connect(f"file:{db_path.resolve()}?mode=ro", uri=True)
    connection.row_factory = sqlite3.Row
    try:
        events = []
        for row in connection.execute(
            "SELECT event_json, created_at FROM axon_observe_events WHERE job_id = ? ORDER BY sequence", (job_id,)
        ):
            event = json.loads(row[0]); event["_created_at_ms"] = row[1]; events.append(event)
        heartbeat_row = connection.execute(
            "SELECT heartbeat_json FROM axon_observe_heartbeats WHERE job_id = ?", (job_id,)
        ).fetchone()
        provider_rows = []
        if provider_ids:
            placeholders = ",".join("?" for _ in provider_ids)
            provider_rows = connection.execute(
                "SELECT provider_id, provider_kind, status, cooldown_until, last_error_code "
                f"FROM axon_observe_provider_health WHERE provider_id IN ({placeholders}) ORDER BY provider_id",
                provider_ids,
            ).fetchall()
    except sqlite3.Error as error:
        raise ObservabilityFailure(f"durable observability unavailable: {error}") from error
    finally:
        connection.close()
    return {
        "events": events,
        "heartbeat": json.loads(heartbeat_row[0]) if heartbeat_row else None,
        "provider_health": [
            {"provider_id": row[0], "provider_kind": row[1], "status": row[2],
             "classification": "provider" if row[4] else None,
             "cooldown_until": row[3], "last_error_code": row[4]} for row in provider_rows
        ],
    }


def _passed(oracle_id: str, detail: dict[str, Any]) -> dict[str, Any]:
    return {"id": oracle_id, "passed": True, "detail": detail}


def _require(condition: bool, message: str) -> None:
    if not condition:
        raise ObservabilityFailure(message)


def _correlation(capture: dict[str, Any], runtime: dict[str, Any]) -> dict[str, Any]:
    executions = capture["executions"]
    mode = capture["observation_mode"]
    ids = [item["job_id"] for item in executions]
    if mode == "multi_observer":
        _require(len(set(ids)) == 1, "multi-observer surfaces disagree on job identity")
        job_id = ids[0]
        _require(all(event.get("job_id") == job_id for event in runtime["events"]),
                 "durable event escaped the correlated job stream")
        heartbeat = runtime.get("heartbeat")
        _require(heartbeat is not None and heartbeat.get("job_id") == job_id,
                 "correlated heartbeat is absent")
    elif mode == "parity":
        _require(len(set(ids)) == len(ids), "parity executions were conflated as observers")
        groups = {item.get("equivalence_group") for item in executions}
        _require(None not in groups and len(groups) == 1, "parity equivalence group is absent")
    else:
        raise ObservabilityFailure("unknown observation mode")
    known_ids = set(ids)
    _require(all(item.get("job_id") in known_ids for item in capture["logs"]),
             "structured log lost job correlation")
    _require(capture["metrics"], "bounded lifecycle metric is absent")
    forbidden_metric_labels = {"job_id", "source_id", "url", "path", "canonical_uri"}
    _require(all(not (set(item.get("labels", {})) & forbidden_metric_labels) for item in capture["metrics"]),
             "metric contains forbidden high-cardinality correlation")
    observed_phases = {item.get("phase") for item in runtime["events"]}
    _require(all(item.get("labels", {}).get("phase") in observed_phases for item in capture["metrics"]),
             "metric phase is not attributable to the observed lifecycle")
    _require(all(item.get("job_id") in known_ids for item in capture["evidence"]),
             "suite evidence lost job correlation")
    for execution in executions:
        progress_sequence = execution.get("progress_sequence")
        terminal_sequence = execution.get("terminal_sequence")
        _require(isinstance(progress_sequence, int) and isinstance(terminal_sequence, int)
                 and progress_sequence < terminal_sequence,
                 f"{execution.get('surface')} progress was not observable before terminal")
    return _passed(ORACLE_IDS[0], {"mode": mode, "observer_count": len(executions)})


def _event_order(runtime: dict[str, Any]) -> dict[str, Any]:
    events = runtime["events"]
    _require(events, "critical lifecycle emitted no durable events")
    sequences = [item.get("sequence") for item in events]
    _require(all(isinstance(value, int) and value > 0 for value in sequences), "event sequence is invalid")
    _require(sequences == sorted(sequences) and len(set(sequences)) == len(sequences),
             "events are duplicate or out of causal order")
    _require(sequences == list(range(sequences[0], sequences[0] + len(sequences))),
             "event stream has a cardinality gap")
    unknown = {str(item.get("phase")) for item in events} - set(PHASE_ORDER)
    _require(not unknown, f"unknown production pipeline phase: {sorted(unknown)}")
    ranks = [PHASE_ORDER[item["phase"]] for item in events]
    _require(ranks == sorted(ranks), "pipeline phases violate declared causal order")
    _require(events[0].get("status") in {"running", "waiting", "queued"}, "lifecycle has no causal start")
    _require(events[-1].get("phase") in {"complete", "canceled"} and events[-1].get("status") in TERMINAL,
             "lifecycle has no production terminal phase/status")
    done_history: dict[tuple[str, str], list[int]] = {}
    for event in events:
        _require(isinstance(event.get("attempt", 0), int) and event.get("attempt", 0) >= 0,
                 "event attempt is invalid")
        counts = event.get("counts") or {}
        _require(isinstance(counts, dict), "progress counts are invalid")
        for prefix in ("items", "documents", "chunks", "bytes"):
            done, total = counts.get(f"{prefix}_done"), counts.get(f"{prefix}_total")
            if done is None: continue
            _require(isinstance(done, int) and done >= 0, "progress is unbounded")
            _require(total is None or isinstance(total, int) and done <= total, "progress is unbounded")
            done_history.setdefault((str(event["phase"]), prefix), []).append(done)
    for (_phase, prefix), values in done_history.items():
        _require(all(right >= left for left, right in zip(values, values[1:])), f"{prefix} progress regressed")
    heartbeat = runtime.get("heartbeat")
    _require(heartbeat is not None and isinstance(heartbeat.get("last_event_sequence"), int),
             "heartbeat lacks last durable event sequence")
    _require(0 < heartbeat["last_event_sequence"] <= sequences[-1],
             "heartbeat contradicts durable event sequence")
    return _passed(ORACLE_IDS[1], {"event_count": len(events), "phase_count": len(set(item["phase"] for item in events))})


def _retry(events: list[dict[str, Any]]) -> dict[str, Any]:
    retries = [(index, item["retry"]) for index, item in enumerate(events) if isinstance(item.get("retry"), dict)]
    retry_attempts = [item.get("attempt") for _, item in retries]
    _require(all(isinstance(value, int) and value >= 1 for value in retry_attempts), "retry attempt is invalid")
    _require(retry_attempts == sorted(retry_attempts), "retry attempts are causally reordered")
    for index, _retry_state in retries:
        prior = events[:index]
        _require(any(item.get("error") or item.get("status") in {"failed", "completed_degraded"} for item in prior),
                 "retry has no preceding failure/degradation")
    return _passed(ORACLE_IDS[2], {"retry_count": len(retries), "highest_attempt": max(retry_attempts, default=0)})


def classify_error(error: dict[str, Any]) -> str:
    code = str(error.get("code", ""))
    if code.startswith(("auth.", "security.", "network.", "http.ssrf", "route.local_path")):
        return "auth/network"
    if code.startswith(("provider.", "embedding.", "llm.", "qdrant.", "chrome.")):
        return "provider"
    return "product"


def _provider(capture: dict[str, Any], runtime: dict[str, Any]) -> dict[str, Any]:
    expected = capture.get("expected_failure")
    if expected:
        classification = expected.get("classification")
        _require(classification in FAILURE_CLASSES, "unknown failure taxonomy")
        surfaces = [item.get("failure_classification") for item in capture["executions"]]
        _require(surfaces and all(item == classification for item in surfaces),
                 "transport failure classifications disagree")
        event_classes = [classify_error(item["error"]) for item in runtime["events"]
                         if isinstance(item.get("error"), dict)]
        _require(event_classes and all(item == classification for item in event_classes),
                 "runtime failure classification disagrees")
        if classification == "provider":
            health = runtime.get("provider_health", [])
            expected_ids = set(capture.get("owned_provider_ids", []))
            _require(health and expected_ids == {item["provider_id"] for item in health}
                     and any(item.get("last_error_code") for item in health),
                     "provider failure has no provider-health evidence")
    return _passed(ORACLE_IDS[3], {"failure_classification": expected and expected["classification"],
                                   "provider_records": len(runtime.get("provider_health", []))})


def _terminal(capture: dict[str, Any], runtime: dict[str, Any]) -> dict[str, Any]:
    terminals = [item.get("terminal_status") for item in capture["executions"]]
    terminal_events = [item.get("status") for item in runtime["events"]
                       if item.get("phase") in {"complete", "canceled"}]
    _require(terminals and len(set(terminals)) == 1, "public terminal states contradict")
    _require(len(terminal_events) == 1 and terminal_events[0] == terminals[0],
             "durable and public terminal states contradict")
    return _passed(ORACLE_IDS[4], {"terminal_status": terminals[0]})


def _redaction(capture: dict[str, Any], runtime: dict[str, Any]) -> dict[str, Any]:
    protected = tuple(capture.get("protected_canaries", [])) + tuple(capture.get("private_paths", []))
    sanitized_capture = {key: value for key, value in capture.items()
                         if key not in {"protected_canaries", "private_paths"}}
    raw_channels = sanitized_capture.pop("raw_channels", {})
    _require(isinstance(raw_channels, dict) and raw_channels, "raw observability channels are absent")
    channels = {"capture": sanitized_capture, "runtime": runtime,
                **{f"transport.{name}": value for name, value in raw_channels.items()}}
    for channel, value in channels.items():
        encoded = json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
        try:
            redaction.scan_bytes(encoded, protected)
        except redaction.RedactionError as error:
            raise ObservabilityFailure(f"protected data leaked through {channel} observability: {error}") from error
    return _passed(ORACLE_IDS[5], {"channels_scanned": ["transport", "events", "heartbeat", "provider_health", "logs", "metrics", "evidence"]})


def _timing(capture: dict[str, Any], runtime: dict[str, Any]) -> dict[str, Any]:
    timing = capture["timing"]
    duration = timing["finished_monotonic_ms"] - timing["started_monotonic_ms"]
    _require(duration >= 0, "monotonic execution duration is negative")
    declared = timing["reported_duration_ms"]
    tolerance = timing.get("tolerance_ms", 250)
    _require(abs(duration - declared) <= tolerance, "reported timing exceeds declared tolerance")
    timestamps = [item.get("_created_at_ms") for item in runtime["events"]]
    if not all(isinstance(item, int) for item in timestamps):
        try:
            timestamps = [int(datetime.fromisoformat(str(item["timestamp"]).replace("Z", "+00:00")).timestamp() * 1000)
                          for item in runtime["events"]]
        except (KeyError, TypeError, ValueError) as error:
            raise ObservabilityFailure("production event timestamp is invalid") from error
    if len(timestamps) > 1:
        _require(max(timestamps) - min(timestamps) <= duration + tolerance,
                 "event elapsed time cannot reconcile with scenario timing")
    return _passed(ORACLE_IDS[6], {"duration_ms": duration, "tolerance_ms": tolerance,
                                   "clock_contract": "elapsed-monotonic; epoch-order-only"})


def evaluate(capture: dict[str, Any], runtime: dict[str, Any]) -> list[dict[str, Any]]:
    """Evaluate every critical oracle, failing closed on absent evidence."""
    required = {"observation_mode", "executions", "timing", "logs", "metrics", "evidence",
                "protected_canaries", "private_paths"}
    _require(required <= set(capture), f"capture is incomplete: {sorted(required - set(capture))}")
    _require({"events", "heartbeat", "provider_health"} <= set(runtime), "runtime evidence is incomplete")
    return [_correlation(capture, runtime), _event_order(runtime), _retry(runtime["events"]),
            _provider(capture, runtime), _terminal(capture, runtime), _redaction(capture, runtime),
            _timing(capture, runtime)]
