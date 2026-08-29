"""Validation and evidence-gate helpers for aggregate MLX metrics."""

from __future__ import annotations

from dataclasses import dataclass

COUNTER_FIELDS = (
    "requests",
    "useful_tokens",
    "padded_tokens",
    "dispatches",
    "partial_dispatches",
    "row_capacity",
    "token_capacity",
    "tokenize_us",
    "serialize_us",
    "request_wall_us",
    "metal_busy_us",
    "dispatcher_idle_us",
)
MAX_COUNTER = 2**63 - 1


class MetricsValidationError(ValueError):
    def __init__(self, code: str):
        super().__init__(code)
        self.code = code


@dataclass(frozen=True)
class MetricsDelta:
    epoch: str
    values: dict[str, int]

    @property
    def padding_ratio(self) -> float:
        padded = self.values["padded_tokens"]
        return 0.0 if padded == 0 else 1.0 - self.values["useful_tokens"] / padded

    @property
    def row_occupancy(self) -> float:
        capacity = self.values["row_capacity"]
        rows = self.values["dispatches"] * 0  # rows are derived from occupancy counters below
        useful_rows = self.values.get("rows_total", rows)
        return 0.0 if capacity == 0 else useful_rows / capacity

    @property
    def token_occupancy(self) -> float:
        capacity = self.values["token_capacity"]
        return 0.0 if capacity == 0 else self.values["padded_tokens"] / capacity

    @property
    def metal_idle_ratio(self) -> float:
        wall = self.values["request_wall_us"]
        return 0.0 if wall == 0 else self.values["dispatcher_idle_us"] / wall


def _validated_snapshot(snapshot: object) -> dict[str, int | str]:
    if not isinstance(snapshot, dict) or set(snapshot) != {"epoch", *COUNTER_FIELDS, "rows_total"}:
        raise MetricsValidationError("metrics_schema")
    epoch = snapshot["epoch"]
    if not isinstance(epoch, str) or len(epoch) != 32:
        raise MetricsValidationError("metrics_epoch")
    output: dict[str, int | str] = {"epoch": epoch}
    for field in (*COUNTER_FIELDS, "rows_total"):
        value = snapshot[field]
        if isinstance(value, bool) or not isinstance(value, int) or value < 0 or value > MAX_COUNTER:
            raise MetricsValidationError("metrics_counter")
        output[field] = value
    if output["useful_tokens"] > output["padded_tokens"]:
        raise MetricsValidationError("metrics_token_relation")
    if output["partial_dispatches"] > output["dispatches"]:
        raise MetricsValidationError("metrics_dispatch_relation")
    if output["rows_total"] > output["row_capacity"]:
        raise MetricsValidationError("metrics_row_relation")
    if output["padded_tokens"] > output["token_capacity"]:
        raise MetricsValidationError("metrics_capacity_relation")
    return output


def metrics_delta(before: object, after: object, expected_requests: int) -> MetricsDelta:
    left = _validated_snapshot(before)
    right = _validated_snapshot(after)
    if left["epoch"] != right["epoch"]:
        raise MetricsValidationError("metrics_epoch_changed")
    values: dict[str, int] = {}
    for field in (*COUNTER_FIELDS, "rows_total"):
        old, new = int(left[field]), int(right[field])
        if new < old:
            raise MetricsValidationError("metrics_counter_regressed")
        values[field] = new - old
    if values["requests"] != expected_requests:
        raise MetricsValidationError("metrics_request_contamination")
    _validated_snapshot({"epoch": left["epoch"], **values})
    return MetricsDelta(str(left["epoch"]), values)


def evidence_gate(delta: MetricsDelta) -> tuple[bool, tuple[str, ...]]:
    reasons = []
    if delta.padding_ratio >= 0.20:
        reasons.append("padding")
    if delta.row_occupancy < 0.85 or delta.token_occupancy < 0.85:
        reasons.append("occupancy")
    if delta.metal_idle_ratio >= 0.05:
        reasons.append("metal_idle")
    return bool(reasons), tuple(reasons)
