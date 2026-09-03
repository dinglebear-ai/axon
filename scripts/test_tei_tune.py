import importlib.util
from pathlib import Path
import argparse
import subprocess
import tempfile
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
            },
            "HostConfig": {"RestartPolicy": {"Name": "always"}, "Binds": ["/models:/data:ro"]},
            "NetworkSettings": {"Networks": {"tei-net": {}}},
        }
        snapshot = tei.snapshot()
        self.assertEqual(
            snapshot.get("entrypoint"),
            ["/usr/local/bin/text-embeddings-router"],
        )
        self.assertEqual(snapshot["host_config"]["RestartPolicy"]["Name"], "always")
        self.assertEqual(snapshot["host_config"]["Binds"], ["/models:/data:ro"])
        self.assertEqual(snapshot["network_mode"], "tei-net")

    def test_apply_rolls_back_when_replacement_deploy_fails(self):
        tei = object.__new__(tei_tune.Tei)
        tei.state = Path("/tmp/unused-tei-state")
        previous = {"image": "old", "cmd": ["old-cmd"], "entrypoint": None}
        tei.snapshot = mock.Mock(return_value=previous)
        tei.save_snapshot = mock.Mock()
        tei.ready = mock.Mock(return_value=(True, "ready"))
        tei.park_current = mock.Mock()
        tei.restore_parked = mock.Mock()
        tei.discard_parked = mock.Mock()
        tei.deploy = mock.Mock(side_effect=[RuntimeError("docker run failed"), None])
        with self.assertRaisesRegex(RuntimeError, "previous TEI configuration restored"):
            tei.apply("new", ["new-cmd"], dry_run=False)
        self.assertEqual(tei.deploy.call_count, 1)
        tei.restore_parked.assert_called_once_with()

    def test_apply_reports_deployment_and_restoration_failures(self):
        tei = object.__new__(tei_tune.Tei)
        tei.state = Path("/tmp/unused-tei-state")
        tei.snapshot = mock.Mock(return_value={"image": "old", "cmd": []})
        tei.save_snapshot = mock.Mock()
        tei.park_current = mock.Mock()
        tei.deploy = mock.Mock(side_effect=RuntimeError("new deploy failed"))
        tei.restore_parked = mock.Mock(side_effect=RuntimeError("old restore failed"))
        with self.assertRaisesRegex(
            RuntimeError, "new deploy failed.*restoring.*old restore failed"
        ):
            tei.apply("new", ["new-cmd"], dry_run=False)

    def test_mutation_lock_contention_fails_before_docker_mutation(self):
        with tempfile.TemporaryDirectory() as directory:
            tei = object.__new__(tei_tune.Tei)
            tei.state = Path(directory) / "state.json"
            tei.host = "tootie"
            tei.container = "axon-tei"
            tei.snapshot = mock.Mock()
            with tei.mutation_lock():
                with self.assertRaisesRegex(RuntimeError, "already running"):
                    tei.apply("new", ["new-cmd"], dry_run=False)
            tei.snapshot.assert_not_called()

    def test_successful_apply_discards_parked_container_only_after_readiness(self):
        tei = object.__new__(tei_tune.Tei)
        tei.state = Path("/tmp/unused-tei-state")
        tei.snapshot = mock.Mock(return_value={"image": "old", "cmd": []})
        tei.save_snapshot = mock.Mock()
        tei.park_current = mock.Mock()
        tei.restore_parked = mock.Mock()
        tei.discard_parked = mock.Mock()
        tei.deploy = mock.Mock()
        tei.ready = mock.Mock(return_value=(True, "ready"))
        tei.apply("new", ["new-cmd"], dry_run=False)
        tei.park_current.assert_called_once_with()
        tei.discard_parked.assert_called_once_with()
        tei.restore_parked.assert_not_called()

    def test_park_restarts_original_when_rename_fails(self):
        tei = object.__new__(tei_tune.Tei)
        tei.container = "axon-tei"
        calls = []
        def remote(argv, check=True):
            calls.append(argv)
            if argv[:2] == ["docker", "rename"]:
                raise subprocess.CalledProcessError(1, argv)
            return mock.Mock(returncode=0, stderr="", stdout="")
        tei.remote = remote
        with self.assertRaises(subprocess.CalledProcessError):
            tei.park_current()
        self.assertIn(["docker", "start", "axon-tei"], calls)

    def test_park_reports_both_rename_and_restart_failure(self):
        tei = object.__new__(tei_tune.Tei)
        tei.container = "axon-tei"
        def remote(argv, check=True):
            if argv[:2] == ["docker", "rename"]:
                raise subprocess.CalledProcessError(1, argv, stderr="rename failed")
            if argv[:2] == ["docker", "start"]:
                return mock.Mock(returncode=1, stderr="restart failed", stdout="")
            return mock.Mock(returncode=0, stderr="", stdout="")
        tei.remote = remote
        with self.assertRaisesRegex(RuntimeError, "rename failed.*restart failed"):
            tei.park_current()

    def test_manual_rollback_restores_current_when_target_deploy_fails(self):
        tei = object.__new__(tei_tune.Tei)
        tei.park_current = mock.Mock()
        tei.deploy_snapshot = mock.Mock(side_effect=RuntimeError("run failed"))
        tei.restore_parked = mock.Mock()
        with self.assertRaisesRegex(RuntimeError, "current TEI configuration restored"):
            tei.rollback_to_snapshot({"image": "old"})
        tei.restore_parked.assert_called_once_with()

    def test_manual_rollback_reports_target_and_restoration_failures(self):
        tei = object.__new__(tei_tune.Tei)
        tei.park_current = mock.Mock()
        tei.deploy_snapshot = mock.Mock(side_effect=RuntimeError("target failed"))
        tei.restore_parked = mock.Mock(side_effect=RuntimeError("current restore failed"))
        with self.assertRaisesRegex(
            RuntimeError, "target failed.*restoring.*current restore failed"
        ):
            tei.rollback_to_snapshot({"image": "old"})

    def test_saved_snapshot_swap_holds_one_lock_across_read_mutate_and_save(self):
        tei = object.__new__(tei_tune.Tei)
        events = []

        @tei_tune.contextlib.contextmanager
        def mutation_lock():
            events.append("lock")
            yield
            events.append("unlock")

        tei.mutation_lock = mutation_lock
        tei.state = mock.Mock()
        tei.state.read_text.return_value = '{"image": "target"}'
        tei.snapshot = mock.Mock(side_effect=lambda: events.append("snapshot") or {"image": "current"})
        tei._rollback_to_snapshot = mock.Mock(
            side_effect=lambda target, **options: events.append(("rollback", target, options))
        )
        tei.save_snapshot = mock.Mock(side_effect=lambda current: events.append(("save", current)))
        tei.discard_parked = mock.Mock(side_effect=lambda: events.append("discard"))

        tei.swap_with_saved_snapshot()

        self.assertEqual(
            events,
            ["lock", "snapshot", ("rollback", {"image": "target"}, {"discard_parked": False}),
             ("save", {"image": "current"}), "discard", "unlock"],
        )

    def test_saved_snapshot_failure_restores_parked_current_before_discard(self):
        tei = object.__new__(tei_tune.Tei)
        tei.state = mock.Mock()
        tei.mutation_lock = lambda: tei_tune.contextlib.nullcontext()
        tei.state.read_text.return_value = '{"image": "target"}'
        tei.snapshot = mock.Mock(return_value={"image": "current"})
        tei._rollback_to_snapshot = mock.Mock()
        tei.save_snapshot = mock.Mock(side_effect=OSError("disk full"))
        tei.restore_parked = mock.Mock()
        tei.discard_parked = mock.Mock()

        with self.assertRaisesRegex(RuntimeError, "disk full.*current TEI configuration restored"):
            tei.swap_with_saved_snapshot()

        tei._rollback_to_snapshot.assert_called_once_with(
            {"image": "target"}, discard_parked=False
        )
        tei.restore_parked.assert_called_once_with()
        tei.discard_parked.assert_not_called()

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

    def test_snapshot_run_command_preserves_runtime_configuration(self):
        snapshot = {
            "image": "example/tei:old", "cmd": ["--model-id", "old"],
            "entrypoint": ["/router", "--serve"], "network_mode": "tei-net",
            "config": {"Env": ["HF_HUB_CACHE=/models", "CUSTOM=yes"], "Labels": {"owner": "axon"},
                       "User": "1000:1000", "WorkingDir": "/work"},
            "host_config": {
                "RestartPolicy": {"Name": "always"}, "Binds": ["/models:/data:ro"],
                "PortBindings": {"80/tcp": [{"HostIp": "127.0.0.1", "HostPort": "52000"}]},
                "Runtime": "nvidia",
                "DeviceRequests": [{"Driver": "nvidia", "DeviceIDs": ["1"], "Capabilities": [["gpu"]]}],
            },
        }
        command = tei_tune.docker_run_from_snapshot("axon-tei", snapshot)
        for expected in (
            "--restart", "always", "--network", "tei-net", "--runtime", "nvidia",
            "--gpus", "device=1", "-p", "127.0.0.1:52000:80/tcp",
            "-v", "/models:/data:ro", "-e", "CUSTOM=yes", "--label", "owner=axon",
            "--user", "1000:1000", "--workdir", "/work", "--entrypoint", "/router",
        ):
            self.assertIn(expected, command)
        image_index = command.index("example/tei:old")
        self.assertEqual(command[image_index + 1:image_index + 4], ["--serve", "--model-id", "old"])

    def test_apply_readiness_failure_restores_parked_current(self):
        tei = object.__new__(tei_tune.Tei)
        tei.state = Path("/tmp/unused-tei-state")
        tei.snapshot = mock.Mock(return_value={"image": "old", "cmd": []})
        tei.save_snapshot = mock.Mock()
        tei.park_current = mock.Mock()
        tei.deploy = mock.Mock()
        tei.restore_parked = mock.Mock()
        tei.discard_parked = mock.Mock()
        tei.ready = mock.Mock(side_effect=[(False, "new bad"), (True, "old ready")])
        with self.assertRaisesRegex(RuntimeError, "previous TEI configuration restored"):
            tei.apply("new", ["cmd"], False)
        tei.restore_parked.assert_called_once_with()
        tei.discard_parked.assert_not_called()

    def test_apply_reports_when_restored_configuration_is_not_ready(self):
        tei = object.__new__(tei_tune.Tei)
        tei.state = Path("/tmp/unused-tei-state")
        tei.snapshot = mock.Mock(return_value={"image": "old", "cmd": []})
        tei.save_snapshot = mock.Mock()
        tei.park_current = mock.Mock()
        tei.deploy = mock.Mock()
        tei.restore_parked = mock.Mock()
        tei.discard_parked = mock.Mock()
        tei.ready = mock.Mock(side_effect=[(False, "new bad"), (False, "old bad")])
        with self.assertRaisesRegex(RuntimeError, "automatic rollback also failed: old bad"):
            tei.apply("new", ["cmd"], False)

    def test_manual_rollback_readiness_failure_restores_current(self):
        tei = object.__new__(tei_tune.Tei)
        tei.park_current = mock.Mock()
        tei.deploy_snapshot = mock.Mock()
        tei.restore_parked = mock.Mock()
        tei.discard_parked = mock.Mock()
        tei.ready = mock.Mock(side_effect=[(False, "target bad"), (True, "current ready")])
        with self.assertRaisesRegex(RuntimeError, "current config restored"):
            tei.rollback_to_snapshot({"image": "old"})
        tei.restore_parked.assert_called_once_with()
        tei.discard_parked.assert_not_called()

    def test_manual_rollback_reports_failed_current_restoration(self):
        tei = object.__new__(tei_tune.Tei)
        tei.park_current = mock.Mock()
        tei.deploy_snapshot = mock.Mock()
        tei.restore_parked = mock.Mock()
        tei.discard_parked = mock.Mock()
        tei.ready = mock.Mock(side_effect=[(False, "target bad"), (False, "current bad")])
        with self.assertRaisesRegex(RuntimeError, "restoring current configuration also failed"):
            tei.rollback_to_snapshot({"image": "old"})

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
