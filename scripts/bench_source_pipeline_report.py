"""Build the benchmark evidence document from isolated snapshots."""
from datetime import datetime, timezone
import json, os, sqlite3, sys
from types import SimpleNamespace
from mlx_metrics import evidence_gate, metrics_delta

before_p, after_p, provider_p, config_p, env_p, job_id, corpus_hash, start_ns, end_ns, database, acquisition_p, mode = sys.argv[1:]
load = lambda path: json.load(open(path, encoding="utf-8"))
before, after, provider, config, environment, acquisition = map(load, (before_p, after_p, provider_p, config_p, env_p, acquisition_p))
metrics_available = before.get("available", True) and after.get("available", True)
if metrics_available:
    if before.get("requests") != 0:
        raise SystemExit("exclusive MLX benchmark service was already used")
    expected = after.get("requests", 0) - before.get("requests", 0)
    if expected <= 0:
        raise SystemExit("benchmark issued no MLX requests")
    delta = metrics_delta(before, after, expected_requests=expected)
    passed, reasons = evidence_gate(delta)
else:
    delta = SimpleNamespace(
        epoch=after.get("epoch") or before.get("epoch") or "unavailable",
        values={"metal_busy_us": 0},
        padding_ratio=None,
        row_occupancy=None,
        token_occupancy=None,
        metal_idle_ratio=None,
    )
    passed, reasons = False, ("provider_metrics_endpoint_unavailable",)
environment["provider_metrics_before"] = {
    "available": metrics_available,
    "requests": before.get("requests"),
    "request_wall_us": before.get("request_wall_us"),
    "metal_busy_us": before.get("metal_busy_us"),
    "dispatcher_idle_us": before.get("dispatcher_idle_us"),
}
environment["provider_exclusive"] = metrics_available and before.get("requests") == 0
with sqlite3.connect(database) as connection:
    rows = connection.execute("SELECT phase,started_at,completed_at FROM job_stages WHERE job_id=? AND started_at IS NOT NULL AND completed_at IS NOT NULL ORDER BY phase", (job_id,)).fetchall()
    events = connection.execute("SELECT phase,MIN(timestamp),MAX(timestamp),COUNT(*) FROM job_events WHERE job_id=? GROUP BY phase ORDER BY phase", (job_id,)).fetchall()
    has_reservations = connection.execute("SELECT 1 FROM sqlite_master WHERE type='table' AND name='provider_reservations'").fetchone()
    setup_reservations = [] if not has_reservations else connection.execute(
        "SELECT fence,acquired_at,updated_at,status FROM provider_reservations WHERE job_id=? AND acquired_at IS NOT NULL AND updated_at IS NOT NULL AND fence LIKE '%:ensure-collection:%' ORDER BY acquired_at",
        (job_id,),
    ).fetchall()
def parse(value):
    parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    # SQLite CURRENT_TIMESTAMP values are UTC but omit an offset. Treating
    # them as host-local time shifts scheduler reservations by the timezone.
    return parsed if parsed.tzinfo is not None else parsed.replace(tzinfo=timezone.utc)
origin = datetime.fromtimestamp(int(start_ns) / 1e9).astimezone()
wall = (int(end_ns) - int(start_ns)) / 1e9
stages = {phase: {"semantics": "active_stage_duration", "seconds": round((parse(end) - parse(start)).total_seconds(), 6)} for phase, start, end in rows}
windows = {phase: {"semantics": "observational_event_window_not_active_time", "first_offset_seconds": round((parse(first) - origin).total_seconds(), 6), "last_offset_seconds": round((parse(last) - origin).total_seconds(), 6), "events": count} for phase, first, last, count in events}
active_intervals = [(parse(start).timestamp(), parse(end).timestamp()) for _, start, end in rows]
observational_intervals = [(parse(first).timestamp(), parse(last).timestamp()) for _, first, last, _ in events]
setup_intervals = []
for fence, acquired, updated, status in setup_reservations:
    start, end = parse(acquired).timestamp(), parse(updated).timestamp()
    if end >= start:
        observational_intervals.append((start, end))
        setup_intervals.append({
            "semantics": "durable_provider_reservation_lifetime",
            "operation": "ensure-collection",
            "status": status,
            "seconds": round(end - start, 6),
        })
for batch in acquisition:
    completed = parse(batch["timestamp"]).timestamp()
    observational_intervals.append((completed - float(batch["wall_ms"]) / 1000.0, completed))
# Some transports persist the stage plan without mutating stage timestamps.
# Fall back to job-scoped event envelopes for completeness accounting; retain
# their observational (not active-time) label in the result.
interval_basis = "active_stage_intervals" if active_intervals else "observational_event_envelopes"
intervals = sorted(active_intervals or observational_intervals)
union = 0.0
if intervals:
    left, right = intervals[0]
    for start, end in intervals[1:]:
        if start <= right:
            right = max(right, end)
        else:
            union += right - left
            left, right = start, end
    union += right - left
active_sum = sum(end - start for start, end in intervals)
overlap = max(0.0, active_sum - union)
critical_path = 0.0 if not intervals else max(end for _, end in intervals) - min(start for start, _ in intervals)
unattributed = max(0.0, critical_path - union)
unattributed_ratio = 0.0 if critical_path == 0 else unattributed / critical_path
attribution_ratio = 1.0 - unattributed_ratio
attribution_gate = critical_path > 0 and attribution_ratio >= 0.95
metal = delta.values["metal_busy_us"] / 1e6
baseline = os.environ.get("AXON_BENCH_COMPARISON_ENV_SHA256")
environment_comparable = bool(metrics_available and attribution_gate and baseline and baseline == environment["fingerprint_sha256"] and environment["load_average"][0] <= float(os.environ.get("AXON_BENCH_MAX_LOAD", "8")))
if not attribution_gate:
    reasons = (*reasons, "critical_path_attribution_below_95_percent")
passed = passed and attribution_gate
result = {
    "benchmark_mode": mode,
    "acceptance_claim": "deterministic pipeline comparison" if mode == "pinned-replay" else "live cold crawl qualification",
    "job_id": job_id, "corpus_hash": corpus_hash, "collection_owned": True,
    "collection_retained": os.environ.get("AXON_BENCH_RETAIN_COLLECTION") == "1",
    "work_directory_retained": os.environ.get("AXON_BENCH_RETAIN_WORK_DIR") == "1",
    "work_directory": os.environ.get("AXON_BENCH_WORK_DIR") if os.environ.get("AXON_BENCH_RETAIN_WORK_DIR") == "1" else None,
    "wall_seconds": wall, "provider_contract": provider,
    "throughput_configuration": config, "environment": environment,
    "environment_comparable": environment_comparable,
    "provider_metrics_available": metrics_available,
    "comparison_baseline_sha256": baseline, "metrics_epoch": delta.epoch,
    "padding_ratio": delta.padding_ratio, "row_occupancy": delta.row_occupancy,
    "token_occupancy": delta.token_occupancy, "metal_idle_ratio": delta.metal_idle_ratio,
    "metal_busy_interval": {"semantics": "union_of_provider_reported_accelerator_busy_intervals_within_metrics_epoch", "seconds": metal},
    "wall_minus_metal_busy_seconds": max(0.0, wall - metal),
    "evidence_gate": passed, "evidence_reasons": reasons,
    "attribution_gate": attribution_gate,
    "critical_path_seconds": round(critical_path, 6),
    "overlap_seconds": round(overlap, 6),
    "unattributed_seconds": round(unattributed, 6),
    "unattributed_ratio": round(unattributed_ratio, 9),
    "attribution_ratio": round(attribution_ratio, 9),
    "timing": {"critical_path": {"semantics": "job_scoped_earliest_stage_start_to_latest_stage_completion_or_event", "interval_basis": interval_basis, "seconds": round(critical_path, 6)}, "stage_active": stages, "event_windows": windows, "setup_reservations": setup_intervals, "stage_union_seconds": round(union, 6), "stage_overlap_seconds": round(overlap, 6), "unattributed_critical_path_seconds": round(unattributed, 6), "unattributed_ratio": round(unattributed_ratio, 9)},
    "acquisition_batches": acquisition,
}
print(json.dumps(result, sort_keys=True))
