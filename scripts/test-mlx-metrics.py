import importlib.util
import pathlib
import sys
import unittest

MODULE_PATH = pathlib.Path(__file__).with_name("mlx_metrics.py")
SPEC = importlib.util.spec_from_file_location("mlx_metrics", MODULE_PATH)
METRICS = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules[SPEC.name] = METRICS
SPEC.loader.exec_module(METRICS)


def snapshot(**overrides):
    values = {
        "epoch": "a" * 32,
        "requests": 0,
        "useful_tokens": 0,
        "padded_tokens": 0,
        "dispatches": 0,
        "partial_dispatches": 0,
        "rows_total": 0,
        "row_capacity": 0,
        "token_capacity": 0,
        "tokenize_us": 0,
        "serialize_us": 0,
        "request_wall_us": 0,
        "metal_busy_us": 0,
        "dispatcher_idle_us": 0,
    }
    values.update(overrides)
    return values


class MlxMetricsTests(unittest.TestCase):
    def test_invalid_delta_fails_evidence_gate_with_reasons(self):
        after = snapshot(
            requests=1, useful_tokens=70, padded_tokens=100,
            dispatches=2, partial_dispatches=1, rows_total=20,
            row_capacity=32, token_capacity=200, request_wall_us=100,
            metal_busy_us=90, dispatcher_idle_us=10,
        )
        delta = METRICS.metrics_delta(snapshot(), after, 1)
        self.assertEqual(METRICS.evidence_gate(delta), (False, ("padding", "occupancy", "metal_idle")))

    def test_healthy_delta_passes_evidence_gate(self):
        after = snapshot(
            requests=1, useful_tokens=95, padded_tokens=100,
            dispatches=1, rows_total=95, row_capacity=100,
            token_capacity=110, request_wall_us=100,
            metal_busy_us=98, dispatcher_idle_us=2,
        )
        delta = METRICS.metrics_delta(snapshot(), after, 1)
        self.assertEqual(METRICS.evidence_gate(delta), (True, ()))

    def test_epoch_change_fails_closed(self):
        with self.assertRaisesRegex(METRICS.MetricsValidationError, "metrics_epoch_changed"):
            METRICS.metrics_delta(snapshot(), snapshot(epoch="b" * 32), 0)

    def test_unrelated_request_is_contamination(self):
        with self.assertRaisesRegex(METRICS.MetricsValidationError, "metrics_request_contamination"):
            METRICS.metrics_delta(snapshot(), snapshot(requests=2), 1)

    def test_relations_and_bad_integer_forms_fail(self):
        cases = [
            snapshot(useful_tokens=2, padded_tokens=1),
            snapshot(dispatches=1, partial_dispatches=2),
            snapshot(rows_total=2, row_capacity=1),
            snapshot(padded_tokens=2, token_capacity=1),
            snapshot(requests=-1),
            snapshot(requests=str(2**64)),
            snapshot(requests=2**64),
        ]
        for case in cases:
            with self.subTest(case=case):
                with self.assertRaises(METRICS.MetricsValidationError):
                    METRICS.metrics_delta(snapshot(), case, 0)

    def test_missing_and_extra_fields_fail(self):
        missing = snapshot()
        missing.pop("metal_busy_us")
        extra = {**snapshot(), "raw_value": "secret"}
        for case in (missing, extra):
            with self.assertRaisesRegex(METRICS.MetricsValidationError, "metrics_schema"):
                METRICS.metrics_delta(case, snapshot(), 0)


if __name__ == "__main__":
    unittest.main()
