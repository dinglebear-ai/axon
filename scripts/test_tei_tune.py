import importlib.util
from pathlib import Path
import argparse
import unittest
from unittest import mock


SCRIPT = Path(__file__).with_name("tei-tune.py")
SPEC = importlib.util.spec_from_file_location("tei_tune", SCRIPT)
tei_tune = importlib.util.module_from_spec(SPEC)
assert SPEC.loader
SPEC.loader.exec_module(tei_tune)


class TeiTuneTests(unittest.TestCase):
    def test_benchmark_once_reports_fixed_input_count_and_latency(self):
        tei = object.__new__(tei_tune.Tei)
        tei.url = "http://tei.example"
        with mock.patch.object(tei_tune, "request_embeddings", return_value=4), \
             mock.patch.object(tei_tune.time, "perf_counter", side_effect=[0.0, 0.1, 0.2, 0.4, 0.5, 1.0]):
            result = tei_tune.benchmark_once(
                tei, requests=2, batch_size=4, concurrency=1, sample_chars=256
            )
        self.assertEqual(result["inputs"], 8)
        self.assertEqual(result["errors"], 0)
        self.assertIn("latency_ms_p95", result)

    def test_fixed_input_shape_ceilings_requests(self):
        self.assertEqual(tei_tune.fixed_input_shape(1024, 32), (32, 1024))
        self.assertEqual(tei_tune.fixed_input_shape(1000, 128), (8, 1024))

    def test_percentile_uses_nearest_rank(self):
        self.assertEqual(tei_tune.percentile([1.0, 2.0, 3.0, 4.0], 0.95), 4.0)

    def test_benchmark_sample_has_requested_character_length(self):
        self.assertEqual(len(tei_tune.benchmark_sample(1000)), 1000)
        self.assertEqual(len(tei_tune.benchmark_sample(1)), 1)

    def test_snapshot_preserves_entrypoint_needed_for_exact_rollback(self):
        tei = object.__new__(tei_tune.Tei)
        tei.inspect = lambda: {
            "Config": {
                "Image": "example/tei:sm89",
                "Cmd": ["--model-id", "example/model"],
                "Entrypoint": ["/usr/local/bin/text-embeddings-router"],
            }
        }
        snapshot = tei.snapshot()
        self.assertEqual(
            snapshot.get("entrypoint"),
            ["/usr/local/bin/text-embeddings-router"],
        )

    def test_deploy_uses_requested_entrypoint_override(self):
        tei = object.__new__(tei_tune.Tei)
        tei.container = "axon-tei"
        tei.network = "axon"
        tei.gpu = "0"
        tei.port = "52000"
        tei.cache = "/data/tei"
        tei.entrypoint = "/usr/local/bin/text-embeddings-router"
        calls = []

        def remote(argv, check=True):
            calls.append(argv)
            return type("Result", (), {"returncode": 0, "stderr": "", "stdout": "id"})()

        tei.remote = remote
        tei.deploy("example/tei:sm89", ["--model-id", "example/model"])
        run = calls[1]
        self.assertIn("--entrypoint", run)
        self.assertEqual(
            run[run.index("--entrypoint") + 1],
            "/usr/local/bin/text-embeddings-router",
        )

    def test_rtx4070_preset_records_measured_winner(self):
        config = tei_tune.resolve_config("rtx4070-axon", [], False)
        self.assertEqual(config["max-batch-tokens"], 163840)
        self.assertEqual(config["max-batch-requests"], 16)
        self.assertEqual(config["max-concurrent-requests"], 1024)
        self.assertEqual(config["tokenization-workers"], 16)

    def test_stable_command_contains_proven_gpu_safe_limits(self):
        command = tei_tune.command_for(tei_tune.resolve_config("stable", [], False))
        self.assertIn("163840", command)
        self.assertEqual(
            command[command.index("--max-concurrent-requests") + 1],
            "1024",
        )
        self.assertEqual(command[-1], "--auto-truncate")

    def test_arbitrary_override_accepts_underscore_alias(self):
        config = tei_tune.resolve_config("stable", ["tokenization_workers=24"], False)
        self.assertEqual(config["tokenization-workers"], 24)

    def test_unknown_override_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "unknown knob"):
            tei_tune.resolve_config("stable", ["magic=9"], False)

    def test_known_oom_batch_requires_explicit_unsafe_flag(self):
        with self.assertRaisesRegex(ValueError, "CUDA OOM"):
            tei_tune.resolve_config("stable", ["max-batch-tokens=262144"], False)
        config = tei_tune.resolve_config("stable", ["max-batch-tokens=262144"], True)
        self.assertEqual(config["max-batch-tokens"], 262144)


if __name__ == "__main__":
    unittest.main()
