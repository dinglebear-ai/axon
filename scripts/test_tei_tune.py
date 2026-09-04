import importlib.util
from pathlib import Path
import argparse
import subprocess
import sys
import tempfile
import unittest
from unittest import mock


SCRIPT = Path(__file__).with_name("tei-tune.py")
sys.path.insert(0, str(SCRIPT.parent))
SPEC = importlib.util.spec_from_file_location("tei_tune", SCRIPT)
tei_tune = importlib.util.module_from_spec(SPEC)
assert SPEC.loader
SPEC.loader.exec_module(tei_tune)


class FakeLockHolder:
    def __init__(self, banner="axon-tei-tune-locked\n", returncode=0, stderr=""):
        self.stdin = mock.Mock()
        self.stdout = mock.Mock()
        self.stdout.readline.return_value = banner
        self.stderr = mock.Mock()
        self.stderr.read.return_value = stderr
        self.returncode = returncode

    def communicate(self):
        self.returncode = self.returncode or 1
        return "", self.stderr.read()

    def wait(self):
        return self.returncode

    def poll(self):
        return self.returncode if self.returncode else None


class TimedOutRemoteCommand:
    def __init__(self):
        self.returncode = -15
        self.terminated = False
        self.calls = 0

    def communicate(self, input=None, timeout=None):
        self.calls += 1
        if self.calls == 1:
            raise subprocess.TimeoutExpired("ssh", timeout)
        return "", ""

    def terminate(self):
        self.terminated = True


class TeiTuneTests(unittest.TestCase):
    def test_ready_rejects_decoy_endpoint_when_container_identity_mismatches(self):
        tei = object.__new__(tei_tune.Tei)
        tei.url = "http://tei.example"
        tei.container = "axon-tei"
        tei.remote = mock.Mock(return_value=mock.Mock(stdout="", stderr="", returncode=0))
        tei.inspect = mock.Mock(return_value={
            "Id": "old-container",
            "State": {"Running": True},
            "Config": {"Image": "tei:old", "Cmd": ["--model-id", "old/model"]},
            "HostConfig": {"DeviceRequests": [{"Driver": "nvidia", "DeviceIDs": ["0"]}]},
        })
        response = mock.MagicMock()
        response.__enter__.return_value.status = 200
        response.__enter__.return_value.read.return_value = b'{"model_id":"new/model"}'
        with mock.patch.object(tei_tune.urllib.request, "urlopen", return_value=response), \
             mock.patch.object(tei_tune.time, "monotonic", side_effect=[0, 0, 2]), \
             mock.patch.object(tei_tune.time, "sleep"):
            ok, detail = tei.ready(
                timeout=1,
                expected={"image": "tei:new", "model_id": "new/model", "gpu": "0"},
            )
        self.assertFalse(ok)
        self.assertIn("image mismatch", detail)

    def test_attestation_rejects_mutable_image_or_wrong_published_port(self):
        tei = object.__new__(tei_tune.Tei)
        tei.container = "axon-tei"
        tei.port = 52000
        tei.inspect = mock.Mock(return_value={
            "State": {"Running": True},
            "Config": {"Image": "tei:new", "Cmd": ["--model-id", "new/model"]},
            "Image": "sha256:actual",
            "HostConfig": {"DeviceRequests": [{"Driver": "nvidia", "DeviceIDs": ["0"]}]},
            "NetworkSettings": {"Ports": {"80/tcp": [{"HostIp": "127.0.0.1", "HostPort": "59999"}]}},
        })
        tei.remote = mock.Mock(return_value=mock.Mock(returncode=0, stdout="text-embeddings-router\n"))
        ok, detail = tei.readiness_attestation(
            {"image": "tei:new", "image_id": "sha256:expected", "model_id": "new/model", "gpu": "0"},
            {"model_id": "new/model"},
        )
        self.assertFalse(ok)
        self.assertIn("image ID mismatch", detail)

    def test_snapshot_rejects_multiple_device_requests_before_mutation(self):
        snapshot = {
            "image": "tei:old",
            "config": {},
            "host_config": {
                "NetworkMode": "axon",
                "DeviceRequests": [
                    {"Driver": "nvidia", "DeviceIDs": ["0"], "Capabilities": [["gpu"]]},
                    {"Driver": "nvidia", "DeviceIDs": ["1"], "Capabilities": [["gpu"]]},
                ],
            },
        }
        with self.assertRaisesRegex(ValueError, "DeviceRequests"):
            tei_tune.docker_run_from_snapshot("axon-tei", snapshot)

    def test_rejects_ssh_option_injection_before_subprocess(self):
        tei = object.__new__(tei_tune.Tei)
        tei.host = "-oProxyCommand=touch /tmp/pwned"
        with mock.patch.object(tei_tune.subprocess, "run") as run:
            with self.assertRaisesRegex(ValueError, "invalid SSH host"):
                tei.remote(["true"])
        run.assert_not_called()

    def test_remote_lock_loss_cancels_inflight_mutation(self):
        tei = object.__new__(tei_tune.Tei)
        tei.host = "tootie"
        tei.container = "axon-tei"
        holder = mock.Mock()
        holder.poll.side_effect = [None, 255]
        tei._remote_lock_holder = holder
        tei._remote_operation_lock = "/tmp/axon-tei-tune-axon-tei.operation.lock"
        command = TimedOutRemoteCommand()
        with mock.patch.object(tei_tune.subprocess, "Popen", return_value=command) as popen:
            with self.assertRaisesRegex(RuntimeError, "remote state is uncertain"):
                tei.remote(["docker", "stop", "axon-tei"])
        self.assertTrue(command.terminated)
        self.assertIn("flock -s /tmp/axon-tei-tune-axon-tei.operation.lock -- docker stop axon-tei", popen.call_args.args[0][2])

    def test_benchmark_once_reports_fixed_input_count_and_latency(self):
        tei = object.__new__(tei_tune.Tei)
        tei.url = "http://tei.example"
        with mock.patch.object(tei_tune.tei_tune_benchmark, "request_embeddings", return_value=4), \
             mock.patch.object(tei_tune.tei_tune_benchmark.time, "perf_counter", side_effect=[0.0, 0.1, 0.2, 0.4, 0.5, 1.0]):
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
            "HostConfig": {"RestartPolicy": {"Name": "always"}, "Binds": ["/models:/data:ro"], "NetworkMode": "tei-net"},
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
        tei.mutation_lock = lambda: tei_tune.contextlib.nullcontext()
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
        tei.mutation_lock = lambda: tei_tune.contextlib.nullcontext()
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
            with mock.patch.object(tei_tune.subprocess, "Popen", return_value=FakeLockHolder()):
                with tei.mutation_lock():
                    with self.assertRaisesRegex(RuntimeError, "already running"):
                        tei.apply("new", ["new-cmd"], dry_run=False)
            tei.snapshot.assert_not_called()

    def test_remote_mutation_lock_contention_fails_before_body(self):
        with tempfile.TemporaryDirectory() as directory:
            tei = object.__new__(tei_tune.Tei)
            tei.state = Path(directory) / "state.json"
            tei.host = "tootie"
            tei.container = "axon-tei"
            entered = False
            holder = FakeLockHolder(banner="", returncode=1, stderr="busy")
            with mock.patch.object(tei_tune.subprocess, "Popen", return_value=holder):
                with self.assertRaisesRegex(RuntimeError, "already running"):
                    with tei.mutation_lock():
                        entered = True
            self.assertFalse(entered)

    def test_remote_lock_loss_after_banner_aborts_before_body(self):
        with tempfile.TemporaryDirectory() as directory:
            tei = object.__new__(tei_tune.Tei)
            tei.state = Path(directory) / "state.json"
            tei.host = "tootie"
            tei.container = "axon-tei"
            holder = FakeLockHolder(returncode=255, stderr="connection lost")
            with mock.patch.object(tei_tune.subprocess, "Popen", return_value=holder):
                with self.assertRaisesRegex(RuntimeError, "lock was lost"):
                    with tei.mutation_lock():
                        self.fail("mutation body must not run after lock loss")

    def test_successful_apply_discards_parked_container_only_after_readiness(self):
        tei = object.__new__(tei_tune.Tei)
        tei.state = Path("/tmp/unused-tei-state")
        tei.mutation_lock = lambda: tei_tune.contextlib.nullcontext()
        tei.snapshot = mock.Mock(return_value={"image": "old", "cmd": []})
        tei.save_snapshot = mock.Mock()
        tei.park_current = mock.Mock()
        tei.restore_parked = mock.Mock()
        tei.discard_parked = mock.Mock()
        tei.deploy = mock.Mock()
        tei.resolve_image_id = mock.Mock(return_value="sha256:new")
        tei.ready = mock.Mock(return_value=(True, "ready"))
        tei.apply("new", ["new-cmd"], dry_run=False)
        tei.park_current.assert_called_once_with()
        tei.discard_parked.assert_called_once_with()
        tei.restore_parked.assert_not_called()

    def test_apply_reports_live_configuration_when_parked_cleanup_fails(self):
        tei = object.__new__(tei_tune.Tei)
        tei.state = Path("/tmp/unused-tei-state")
        tei.host = "tootie"
        tei.container = "axon-tei"
        tei.mutation_lock = lambda: tei_tune.contextlib.nullcontext()
        tei.snapshot = mock.Mock(return_value={"image": "old", "cmd": []})
        tei.save_snapshot = mock.Mock()
        tei.park_current = mock.Mock()
        tei.deploy = mock.Mock()
        tei.resolve_image_id = mock.Mock(return_value="sha256:new")
        tei.ready = mock.Mock(return_value=(True, "ready"))
        tei.discard_parked = mock.Mock(side_effect=RuntimeError("rm failed"))
        with self.assertRaisesRegex(RuntimeError, "succeeded and is live.*retry safely"):
            tei.apply("new", ["new-cmd"], dry_run=False)

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
        tei.container = "axon-tei"
        tei.mutation_lock = lambda: tei_tune.contextlib.nullcontext()
        tei.park_current = mock.Mock()
        tei.deploy_snapshot = mock.Mock(side_effect=RuntimeError("run failed"))
        tei.restore_parked = mock.Mock()
        with self.assertRaisesRegex(RuntimeError, "current TEI configuration restored"):
            tei.rollback_to_snapshot({"image": "old"})
        tei.restore_parked.assert_called_once_with()

    def test_manual_rollback_reports_target_and_restoration_failures(self):
        tei = object.__new__(tei_tune.Tei)
        tei.container = "axon-tei"
        tei.mutation_lock = lambda: tei_tune.contextlib.nullcontext()
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

    def test_saved_snapshot_swap_reports_live_target_when_cleanup_fails(self):
        tei = object.__new__(tei_tune.Tei)
        tei.host = "tootie"
        tei.container = "axon-tei"
        tei.state = mock.Mock()
        tei.mutation_lock = lambda: tei_tune.contextlib.nullcontext()
        tei.state.read_text.return_value = '{"image": "target"}'
        tei.snapshot = mock.Mock(return_value={"image": "current"})
        tei._rollback_to_snapshot = mock.Mock()
        tei.save_snapshot = mock.Mock()
        tei.discard_parked = mock.Mock(side_effect=RuntimeError("rm failed"))
        with self.assertRaisesRegex(RuntimeError, "succeeded and is live.*retry safely"):
            tei.swap_with_saved_snapshot()

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
            "networks": {"tei-net": {"Aliases": ["axon-tei", "embeddings"]}},
            "config": {"Env": ["HF_HUB_CACHE=/models", "CUSTOM=yes"], "Labels": {"owner": "axon"},
                       "User": "1000:1000", "WorkingDir": "/work"},
            "host_config": {
                "RestartPolicy": {"Name": "on-failure", "MaximumRetryCount": 7},
                "Binds": ["/models:/data:ro"],
                "NetworkMode": "tei-net",
                "PortBindings": {"80/tcp": [{"HostIp": "127.0.0.1", "HostPort": "52000"}]},
                "Runtime": "nvidia",
                "Memory": 8_589_934_592, "MemorySwap": -1, "NanoCpus": 2_000_000_000,
                "DeviceRequests": [{"Driver": "nvidia", "DeviceIDs": ["1"], "Capabilities": [["gpu"]]}],
            },
        }
        command = tei_tune.docker_run_from_snapshot("axon-tei", snapshot)
        for expected in (
            "--restart", "on-failure:7", "--network", "tei-net", "--runtime", "nvidia",
            "--gpus", "device=1", "-p", "127.0.0.1:52000:80/tcp",
            "-v", "/models:/data:ro", "--env-file", "/dev/stdin",
            "--network-alias", "embeddings", "--label", "owner=axon",
            "--memory", "8589934592", "--memory-swap", "-1", "--cpus", "2.0",
            "--user", "1000:1000", "--workdir", "/work", "--entrypoint", "/router",
        ):
            self.assertIn(expected, command)
        image_index = command.index("example/tei:old")
        self.assertEqual(command[image_index + 1:image_index + 4], ["--serve", "--model-id", "old"])

    def test_snapshot_deploy_streams_environment_and_attaches_secondary_networks(self):
        tei = object.__new__(tei_tune.Tei)
        tei.container = "axon-tei"
        calls = []

        def remote(argv, check=True, input_text=None):
            calls.append((argv, check, input_text))
            return mock.Mock(returncode=0, stderr="", stdout="id")

        tei.remote = remote
        snapshot = {
            "image": "tei:old", "cmd": [], "entrypoint": [],
            "config": {"Env": ["HF_TOKEN=super-secret"]},
            "host_config": {"NetworkMode": "primary"},
            "networks": {
                "primary": {"Aliases": ["axon-tei"]},
                "secondary": {"Aliases": ["embeddings"]},
            },
        }
        tei.deploy_snapshot(snapshot)
        run_argv, _, stdin = calls[1]
        self.assertNotIn("super-secret", " ".join(run_argv))
        self.assertEqual(stdin, "HF_TOKEN=super-secret\n")
        self.assertEqual(
            calls[2][0],
            ["docker", "network", "connect", "--alias", "embeddings", "secondary", "axon-tei"],
        )

    def test_snapshot_rejects_unsupported_material_runtime_settings(self):
        snapshot = {
            "image": "tei:old", "config": {},
            "host_config": {"NetworkMode": "axon", "Privileged": True},
        }
        with self.assertRaisesRegex(ValueError, "Privileged"):
            tei_tune.docker_run_from_snapshot("axon-tei", snapshot)

    def test_snapshot_rejects_unsupported_material_network_settings(self):
        snapshot = {
            "image": "tei:old", "config": {},
            "host_config": {"NetworkMode": "axon"},
            "networks": {"axon": {"IPAMConfig": {"IPv4Address": "10.0.0.9"}}},
        }
        with self.assertRaisesRegex(ValueError, "IPAMConfig"):
            tei_tune.secondary_network_commands("axon-tei", snapshot)

    def test_rollback_validates_snapshot_before_parking_current(self):
        tei = object.__new__(tei_tune.Tei)
        tei.container = "axon-tei"
        tei.park_current = mock.Mock()
        with self.assertRaisesRegex(ValueError, "Privileged"):
            tei._rollback_to_snapshot({
                "image": "tei:old", "config": {},
                "host_config": {"Privileged": True},
            })
        tei.park_current.assert_not_called()

    def test_secondary_network_failure_is_normalized_for_rollback_compensation(self):
        tei = object.__new__(tei_tune.Tei)
        tei.container = "axon-tei"
        results = [
            mock.Mock(returncode=0, stderr="", stdout=""),
            mock.Mock(returncode=0, stderr="", stdout="id"),
            mock.Mock(returncode=1, stderr="network failed", stdout=""),
        ]
        tei.remote = mock.Mock(side_effect=results)
        snapshot = {
            "image": "tei:old", "config": {},
            "host_config": {"NetworkMode": "primary"},
            "networks": {"primary": {}, "secondary": {}},
        }
        with self.assertRaisesRegex(RuntimeError, "secondary.*network failed"):
            tei.deploy_snapshot(snapshot)

    def test_apply_readiness_failure_restores_parked_current(self):
        tei = object.__new__(tei_tune.Tei)
        tei.state = Path("/tmp/unused-tei-state")
        tei.mutation_lock = lambda: tei_tune.contextlib.nullcontext()
        tei.snapshot = mock.Mock(return_value={"image": "old", "cmd": []})
        tei.save_snapshot = mock.Mock()
        tei.park_current = mock.Mock()
        tei.deploy = mock.Mock()
        tei.resolve_image_id = mock.Mock(return_value="sha256:new")
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
        tei.mutation_lock = lambda: tei_tune.contextlib.nullcontext()
        tei.snapshot = mock.Mock(return_value={"image": "old", "cmd": []})
        tei.save_snapshot = mock.Mock()
        tei.park_current = mock.Mock()
        tei.deploy = mock.Mock()
        tei.resolve_image_id = mock.Mock(return_value="sha256:new")
        tei.restore_parked = mock.Mock()
        tei.discard_parked = mock.Mock()
        tei.ready = mock.Mock(side_effect=[(False, "new bad"), (False, "old bad")])
        with self.assertRaisesRegex(RuntimeError, "automatic rollback also failed: old bad"):
            tei.apply("new", ["cmd"], False)

    def test_manual_rollback_readiness_failure_restores_current(self):
        tei = object.__new__(tei_tune.Tei)
        tei.container = "axon-tei"
        tei.mutation_lock = lambda: tei_tune.contextlib.nullcontext()
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
        tei.container = "axon-tei"
        tei.mutation_lock = lambda: tei_tune.contextlib.nullcontext()
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

    def test_stable_and_rtx4070_presets_share_one_canonical_value(self):
        self.assertIs(
            tei_tune.PRESETS["stable"],
            tei_tune.PRESETS["rtx4070-axon"],
        )
        self.assertIsNot(
            tei_tune.resolve_config("stable", [], False),
            tei_tune.PRESETS["stable"],
        )

    def test_option_value_handles_present_missing_and_trailing_options(self):
        self.assertEqual(
            tei_tune.option_value(["--model-id", "example/model"], "--model-id"),
            "example/model",
        )
        self.assertIsNone(tei_tune.option_value(["--model-id"], "--model-id"))
        self.assertIsNone(tei_tune.option_value(["--port", "80"], "--model-id"))

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
