import importlib.util
import os
import pathlib
import sys
import unittest

os.environ["AXON_MLX_TEST_MODE"] = "1"
MODULE_PATH = pathlib.Path(__file__).with_name("mlx_tei_direct.py")
SPEC = importlib.util.spec_from_file_location("mlx_tei_direct", MODULE_PATH)
SERVER = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules[SPEC.name] = SERVER
SPEC.loader.exec_module(SERVER)


class MlxTeiDirectTests(unittest.TestCase):
    def test_summarize_shapes_counts_tokens_and_occupancy(self):
        summary = SERVER.summarize_shapes(
            [SERVER.BatchShape(3, (2, 4, 4)), SERVER.BatchShape(2, (5, 7))], 3, 24
        )
        self.assertEqual(summary.useful_tokens, 22)
        self.assertEqual(summary.padded_tokens, 26)
        self.assertEqual(summary.dispatches, 2)
        self.assertEqual(summary.partial_dispatches, 1)
        self.assertAlmostEqual(summary.row_occupancy, 5 / 6)
        self.assertAlmostEqual(summary.token_occupancy, 26 / 48)

    def test_empty_shapes_have_zero_ratios(self):
        summary = SERVER.summarize_shapes([], 16, 8192)
        self.assertEqual(summary.padding_ratio, 0.0)
        self.assertEqual(summary.row_occupancy, 0.0)
        self.assertEqual(summary.token_occupancy, 0.0)

    def test_interval_union_and_idle_do_not_double_count_overlap(self):
        intervals = [(0, 10_000), (5_000, 20_000), (30_000, 40_000)]
        self.assertEqual(SERVER.merge_intervals(intervals), [(0, 20_000), (30_000, 40_000)])
        self.assertEqual(SERVER.interval_union_us(intervals), 30)
        self.assertEqual(SERVER.interval_idle_us(intervals, 0, 50_000), 20)
        self.assertEqual(SERVER.interval_window_metrics(intervals, 0, 50_000), (50, 30, 20))

    def test_non_loopback_requires_token(self):
        with self.assertRaisesRegex(ValueError, "requires MLX_TEI_AUTH_TOKEN"):
            SERVER.validate_bind("0.0.0.0", "")
        SERVER.validate_bind("127.0.0.1", "")
        SERVER.validate_bind("100.64.0.1", "secret")

    def test_bearer_authorization(self):
        self.assertTrue(SERVER.authorized(None, ""))
        self.assertFalse(SERVER.authorized(None, "secret"))
        self.assertFalse(SERVER.authorized("Bearer wrong", "secret"))
        self.assertTrue(SERVER.authorized("Bearer secret", "secret"))

    def test_payload_refuses_truncation_and_unknown_fields(self):
        self.assertEqual(SERVER.validate_payload({"inputs": ["hello"], "truncate": False}), ["hello"])
        with self.assertRaises(SERVER.RequestLimitError):
            SERVER.validate_payload({"inputs": ["hello"], "truncate": True})
        with self.assertRaises(SERVER.RequestLimitError):
            SERVER.validate_payload({"inputs": ["hello"], "secret": "value"})

    def test_payload_depth_limit(self):
        nested = value = {}
        for _ in range(SERVER.MAX_JSON_DEPTH + 1):
            value["x"] = {}
            value = value["x"]
        with self.assertRaises(SERVER.RequestLimitError):
            SERVER.validate_payload(nested)

    def test_input_row_and_byte_limits_are_exact(self):
        old_inputs, old_bytes = SERVER.MAX_INPUTS, SERVER.MAX_INPUT_BYTES
        try:
            SERVER.MAX_INPUTS = 2
            SERVER.MAX_INPUT_BYTES = 4
            self.assertEqual(SERVER.validate_payload({"inputs": ["1234", "x"]}), ["1234", "x"])
            with self.assertRaises(SERVER.RequestLimitError):
                SERVER.validate_payload({"inputs": ["a", "b", "c"]})
            with self.assertRaises(SERVER.RequestLimitError):
                SERVER.validate_payload({"inputs": ["12345"]})
        finally:
            SERVER.MAX_INPUTS, SERVER.MAX_INPUT_BYTES = old_inputs, old_bytes


if __name__ == "__main__":
    unittest.main()
