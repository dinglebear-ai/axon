#!/usr/bin/env python3
"""Contract tests for portable E2E isolation and deterministic providers."""

from __future__ import annotations

import importlib.util
import json
import os
import socket
import subprocess
import sys
import tempfile
import threading
import unittest
import urllib.error
import urllib.request
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]


def load(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    assert spec and spec.loader
    spec.loader.exec_module(module)
    return module


isolation = load("axon_e2e_isolation", ROOT / "scripts/e2e/lib/run-isolation.py")
fixtures = load("axon_e2e_fixtures", ROOT / "scripts/e2e/lib/fixture-server.py")


class IsolationTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)

    def tearDown(self):
        self.temp.cleanup()

    def test_concurrent_allocations_have_disjoint_identity_state_and_ports(self):
        results = []
        lock = threading.Lock()

        def allocate_one():
            result = isolation.allocate(self.root / "runs", self.root / "manifests")
            manifest = isolation.Manifest.open(Path(result["manifest"]))
            reservation = isolation.allocate_port(self.root / "ports", result["run_id"], manifest)
            result["port"] = reservation.port
            result["reservation"] = reservation
            with lock:
                results.append(result)

        threads = [threading.Thread(target=allocate_one) for _ in range(2)]
        for thread in threads: thread.start()
        for thread in threads: thread.join()
        self.assertEqual(len(results), 2)
        for field in ("run_id", "run_root", "data_dir", "sqlite", "manifest", "namespace", "port", "ssrf_token"):
            self.assertNotEqual(results[0][field], results[1][field], field)
        self.assertEqual(results[0]["network_policy"], "deny-external")
        for result in results: result["reservation"].close()

    def test_manifest_create_and_allocate_auto_register_before_resource_growth(self):
        registry = self.root / "stable/registry.json"
        with mock.patch.dict(os.environ, {"AXON_E2E_CLEANUP_REGISTRY": str(registry)}):
            run_id = isolation.new_run_id(); data = self.root / "direct" / run_id / "data"; data.mkdir(parents=True)
            direct = isolation.Manifest.create(self.root / "direct-manifests", run_id, data)
            first = json.loads(registry.read_text())["payload"]["runs"]
            self.assertEqual([run_id], [item["run_id"] for item in first])
            direct.register("data_dir", str(data))
            allocated = isolation.allocate(self.root / "runs", self.root / "manifests")
            runs = json.loads(registry.read_text())["payload"]["runs"]
            self.assertEqual({run_id, allocated["run_id"]}, {item["run_id"] for item in runs})

    def test_no_registry_local_create_has_no_external_registration_side_effect(self):
        registry = self.root / "must-not-exist.json"
        with mock.patch.dict(os.environ, {}, clear=True):
            allocation = isolation.allocate(self.root / "runs", self.root / "manifests")
        self.assertFalse(registry.exists())
        self.assertFalse((Path(allocation["manifest"]).parent / "outer-cleanup-registration.json").exists())

    def test_local_ci_and_rerun_allocations_cannot_collide(self):
        allocations = [isolation.allocate(self.root / "runs", self.root / "manifests") for _ in range(3)]
        self.assertEqual(3, len({item["namespace"] for item in allocations}))
        self.assertEqual(3, len({item["manifest"] for item in allocations}))
        for allocation in allocations:
            records = isolation.Manifest.open(Path(allocation["manifest"])).verify()
            self.assertEqual(allocation["run_id"], records[0]["payload"]["run_id"])
        ci_manifest = isolation.Manifest.open(Path(allocations[1]["manifest"]))
        with self.assertRaisesRegex(isolation.IsolationError, "different local/CI/rerun namespace"):
            ci_manifest.register("collection", allocations[0]["namespace"], {"ownership_generation": "f" * 64})

    def test_port_reservation_holds_socket_until_explicit_handoff(self):
        result = isolation.allocate(self.root / "runs", self.root / "manifests")
        manifest = isolation.Manifest.open(Path(result["manifest"]))
        reservation = isolation.allocate_port(self.root / "ports", result["run_id"], manifest)
        competing = socket.socket()
        with self.assertRaises(OSError):
            competing.bind(("127.0.0.1", reservation.port))
        competing.close(); reservation.close()
        handed_off = socket.socket(); handed_off.bind(("127.0.0.1", reservation.port)); handed_off.close()

    def test_manifest_is_outside_data_dir_append_only_and_tamper_evident(self):
        result = isolation.allocate(self.root / "runs", self.root / "manifests")
        manifest_path = Path(result["manifest"])
        self.assertFalse(isolation._is_within(manifest_path, Path(result["data_dir"])))
        manifest = isolation.Manifest.open(manifest_path)
        before = manifest.verify()
        owned = f"{isolation.RUN_PREFIX}artifact_1"
        manifest.register("artifact", owned, {"path": "evidence/artifact.json"})
        after = manifest.verify()
        self.assertEqual(len(after), len(before) + 1)
        lines = manifest_path.read_text().splitlines()
        record = json.loads(lines[-1]); record["payload"]["identity"] += "tampered"
        lines[-1] = json.dumps(record)
        manifest_path.write_text("\n".join(lines) + "\n")
        with self.assertRaisesRegex(isolation.IsolationError, "integrity failure"):
            manifest.verify()

    def test_parallel_resource_registration_preserves_one_valid_chain(self):
        result = isolation.allocate(self.root / "runs", self.root / "manifests")
        manifest = isolation.Manifest.open(Path(result["manifest"]))
        baseline = len(manifest.verify())
        threads = [
            threading.Thread(
                target=manifest.register,
                args=("artifact", f"{isolation.RUN_PREFIX}artifact_{index}"),
            )
            for index in range(12)
        ]
        for thread in threads: thread.start()
        for thread in threads: thread.join()
        records = manifest.verify()
        self.assertEqual(len(records), baseline + len(threads))
        identities = {record["payload"].get("identity") for record in records}
        for index in range(12):
            self.assertIn(f"{isolation.RUN_PREFIX}artifact_{index}", identities)

    def test_unsafe_paths_and_names_fail_closed(self):
        with self.assertRaisesRegex(isolation.IsolationError, "production state"):
            isolation.validate_run_paths(Path.home() / ".axon" / "test", Path.home() / ".axon/test/data", self.root / "manifests")
        with self.assertRaisesRegex(isolation.IsolationError, "inside the owned run root"):
            isolation.validate_run_paths(self.root / "run", self.root / "foreign", self.root / "manifests")
        with self.assertRaises(isolation.IsolationError):
            isolation.validate_owned_name("production_collection")
        result = isolation.allocate(self.root / "runs", self.root / "manifests")
        with self.assertRaises(isolation.IsolationError):
            isolation.Manifest.open(Path(result["manifest"])).register("collection", "../../prod")

    def test_typed_identity_validation_rejects_unsafe_process_port_and_paths(self):
        result = isolation.allocate(self.root / "runs", self.root / "manifests")
        manifest = isolation.Manifest.open(Path(result["manifest"]))
        with self.assertRaisesRegex(isolation.IsolationError, "data_dir identity"):
            manifest.register("data_dir", str(self.root / "other"))
        with self.assertRaisesRegex(isolation.IsolationError, "SQLite identity"):
            manifest.register("sqlite", str(self.root / "foreign.db"))
        with self.assertRaisesRegex(isolation.IsolationError, "loopback"):
            manifest.register("port", "70000", {"host": "0.0.0.0"})
        with self.assertRaisesRegex(isolation.IsolationError, "strong nonce"):
            manifest.register("process", "42", {"start_time": "1"})

    def test_job_and_source_registration_is_bound_to_manifest_run(self):
        result = isolation.allocate(self.root / "runs", self.root / "manifests")
        manifest = isolation.Manifest.open(Path(result["manifest"]))
        metadata = {"run_id": result["run_id"], "scenario_id": "source.inline.happy"}
        manifest.register("job", "job_1", metadata)
        manifest.register("source", "source_1", metadata)
        resources = [record["payload"] for record in manifest.verify() if record["payload"].get("kind") == "resource"]
        self.assertIn(("job", "job_1"), {(item["resource_type"], item["identity"]) for item in resources})
        self.assertIn(("source", "source_1"), {(item["resource_type"], item["identity"]) for item in resources})
        with self.assertRaisesRegex(isolation.IsolationError, "bound to the manifest run"):
            manifest.register("job", "foreign_job", {"run_id": "axon_e2e_foreign"})

    def test_chat_and_provider_reservation_metadata_are_validated(self):
        result = isolation.allocate(self.root / "runs", self.root / "manifests")
        manifest = isolation.Manifest.open(Path(result["manifest"])); run_id = result["run_id"]
        manifest.register("chat_session", f"{run_id}_chat", {"run_id": run_id, "scenario_id": "retrieval.chat"})
        manifest.register("provider_reservation", f"{run_id}_provider", {
            "run_id": run_id, "provider": "llm", "permits": 1, "requests": 3, "retries": 0,
        })
        with self.assertRaisesRegex(isolation.IsolationError, "scenario_id"):
            manifest.register("chat_session", f"{run_id}_bad_chat", {"run_id": run_id})
        with self.assertRaisesRegex(isolation.IsolationError, "provider is not recognized"):
            manifest.register("provider_reservation", f"{run_id}_bad_provider", {
                "run_id": run_id, "provider": "imaginary", "permits": 1,
            })
        with self.assertRaisesRegex(isolation.IsolationError, "nonnegative integer"):
            manifest.register("provider_reservation", f"{run_id}_negative", {
                "run_id": run_id, "provider": "llm", "permits": -1,
            })

    def test_server_generated_upload_and_artifact_require_registered_parent_binding(self):
        result = isolation.allocate(self.root / "runs", self.root / "manifests")
        manifest = isolation.Manifest.open(Path(result["manifest"]))
        operation = f'{result["run_id"]}_http_upload'
        manifest.register("operation", operation, {"run_id": result["run_id"]})
        binding = {"run_id": result["run_id"], "attempt": 1, "scenario_id": "http.upload.create",
                   "request_id": "request-1", "origin": "server_response",
                   "parent_resource_type": "operation", "parent_identity": operation}
        manifest.register("upload", "upl_550e8400-e29b-41d4-a716-446655440000", binding)
        manifest.register("artifact", "art_550e8400-e29b-41d4-a716-446655440001", binding)
        with self.assertRaisesRegex(isolation.IsolationError, "trusted server binding"):
            manifest.register("upload", "upl_550e8400-e29b-41d4-a716-446655440002", {})
        with self.assertRaisesRegex(isolation.IsolationError, "parent is not registered"):
            manifest.register("artifact", "art_550e8400-e29b-41d4-a716-446655440003",
                              {**binding, "parent_identity": f'{result["run_id"]}_missing'})
        with self.assertRaisesRegex(isolation.IsolationError, "invalid production format"):
            manifest.register("upload", "production", binding)

    def test_owned_process_registration_validates_pid_start_time_and_nonce(self):
        result = isolation.allocate(self.root / "runs", self.root / "manifests")
        manifest = isolation.Manifest.open(Path(result["manifest"]))
        managed = isolation.spawn_owned_process(
            manifest, Path(result["run_root"]), [sys.executable, "-c", "import time; time.sleep(30)"],
        )
        try:
            self.assertTrue(managed.validate_owner())
            managed.nonce_file.write_text("foreign", encoding="utf-8")
            self.assertFalse(managed.validate_owner())
            records = manifest.verify()
            process = [r for r in records if r["payload"].get("resource_type") == "process"][-1]
            self.assertEqual(process["payload"]["identity"], str(managed.process.pid))
            self.assertEqual(process["payload"]["metadata"]["start_time"], managed.start_time)
        finally:
            managed.process.terminate(); managed.process.wait(timeout=5)

    def test_isolated_launcher_allows_owned_loopback_and_denies_external(self):
        listener = socket.socket(); listener.bind(("127.0.0.1", 0)); listener.listen(1)
        port = listener.getsockname()[1]
        accepted = threading.Thread(target=lambda: listener.accept()[0].close())
        accepted.start()
        result = isolation.allocate(self.root / "runs", self.root / "manifests")
        manifest = isolation.Manifest.open(Path(result["manifest"]))
        script = (
            "import socket; "
            f"socket.create_connection(('127.0.0.1',{port}),1).close(); "
            "\ntry: socket.create_connection(('203.0.113.1',80),.1)\n"
            "except PermissionError: raise SystemExit(0)\n"
            "raise SystemExit(9)"
        )
        managed = isolation.spawn_isolated_python(
            manifest, Path(result["run_root"]), [sys.executable, "-c", script], [("127.0.0.1", port)],
        )
        self.assertEqual(managed.process.wait(timeout=5), 0)
        accepted.join(timeout=2); listener.close()
        with self.assertRaisesRegex(isolation.IsolationError, "only the isolated Python"):
            isolation.spawn_isolated_python(manifest, Path(result["run_root"]), ["curl", "https://example.com"], [])

    def test_git_fixture_constructor_is_deterministic_and_initialized(self):
        builder = ROOT / "tests/e2e/fixtures/git/build.py"
        commits = []
        for name in ("first", "second"):
            destination = self.root / name
            commit = subprocess.check_output([sys.executable, str(builder), str(destination)], text=True).strip()
            self.assertTrue((destination / ".git").is_dir())
            self.assertEqual(
                subprocess.check_output(["git", "status", "--porcelain"], cwd=destination, text=True), "",
            )
            commits.append(commit)
        self.assertEqual(commits[0], commits[1])

    def test_windows_process_identity_uses_native_creation_filetime_and_closes_handle(self):
        class FakeKernel32:
            def __init__(self): self.closed = []
            def OpenProcess(self, access, inherit, pid):
                self.opened = (access, inherit, pid); return 0x1_0000_0091
            def GetProcessTimes(self, handle, creation, exit_time, kernel, user):
                creation._obj.low = 0x89ABCDEF
                creation._obj.high = 0x12345678
                return 1
            def CloseHandle(self, handle): self.closed.append(handle)

        api = FakeKernel32()
        self.assertEqual(isolation._windows_process_start_time(44, api), str(0x1234567889ABCDEF))
        self.assertEqual(api.opened, (0x1000, False, 44))
        self.assertEqual(api.closed, [0x1_0000_0091])

    def test_windows_ffi_signatures_use_pointer_sized_handle_and_filetime_structures(self):
        class FakeFunction:
            def __call__(self, *_args): return 1

        class FakeLibrary:
            OpenProcess = FakeFunction()
            GetProcessTimes = FakeFunction()
            CloseHandle = FakeFunction()

        library = FakeLibrary()
        isolation._configure_windows_kernel32(library)
        self.assertIs(library.OpenProcess.restype, isolation.ctypes.c_void_p)
        self.assertEqual(library.OpenProcess.argtypes[2], isolation.ctypes.c_uint32)
        self.assertIs(library.GetProcessTimes.argtypes[0], isolation.ctypes.c_void_p)
        pointee = library.GetProcessTimes.argtypes[1]._type_
        self.assertIs(pointee, isolation._WindowsFileTime)
        self.assertEqual(isolation.ctypes.sizeof(pointee), 8)
        self.assertIs(library.CloseHandle.argtypes[0], isolation.ctypes.c_void_p)

    def test_windows_process_identity_fails_closed_on_api_error(self):
        api = mock.Mock()
        api.OpenProcess.return_value = 0
        with self.assertRaisesRegex(isolation.IsolationError, "could not be opened"):
            isolation._windows_process_start_time(44, api)

    def test_windows_acl_applies_owner_only_dacl_and_rejects_broad_access(self):
        successful = subprocess.CompletedProcess([], 0, "fixture\\owner:(F)\n", "")
        with mock.patch.object(isolation.getpass, "getuser", return_value="fixture\\owner"), \
             mock.patch.object(isolation.subprocess, "run", side_effect=[successful, successful]) as run:
            isolation._windows_acl(self.root / "key", apply=True)
            self.assertIn("/inheritance:r", run.call_args_list[0].args[0])
            self.assertIn("fixture\\owner:(F)", run.call_args_list[0].args[0])
        broad = subprocess.CompletedProcess([], 0, "fixture\\owner:(F)\nEveryone:(R)\n", "")
        with mock.patch.object(isolation.getpass, "getuser", return_value="fixture\\owner"), \
             mock.patch.object(isolation.subprocess, "run", return_value=broad):
            with self.assertRaisesRegex(isolation.IsolationError, "beyond the current owner"):
                isolation._windows_acl(self.root / "key", apply=False)

    def test_windows_lock_branch_secures_lock_and_owned_process_uses_new_group(self):
        lock_path = self.root / "win-lock"
        with mock.patch.object(isolation, "_is_windows", return_value=True), \
             mock.patch.object(isolation, "_windows_acl") as secure:
            with isolation._directory_lock(lock_path):
                self.assertTrue(lock_path.is_dir())
            secure.assert_called_once_with(lock_path, apply=True)

        fake_process = mock.Mock(pid=321)
        fake_process.poll.return_value = None
        fake_manifest = mock.Mock()
        with mock.patch.object(isolation, "_is_windows", return_value=True), \
             mock.patch.object(isolation, "_windows_acl"), \
             mock.patch.object(isolation, "_process_start_time", return_value="win-filetime"), \
             mock.patch.object(isolation.subprocess, "Popen", return_value=fake_process) as popen:
            managed = isolation.spawn_owned_process(
                fake_manifest, self.root, ["python.exe", "fixture.py"],
            )
        self.assertEqual(managed.start_time, "win-filetime")
        self.assertFalse(popen.call_args.kwargs["start_new_session"])
        self.assertEqual(popen.call_args.kwargs["creationflags"], 0x00000200)
        registration = fake_manifest.register.call_args
        self.assertEqual(registration.args[0], "process")
        self.assertEqual(registration.args[1], "321")

    def test_weighted_governor_enforces_global_provider_and_owner_limits(self):
        governor = isolation.ResourceGovernor(self.root / "governor", 4, {"tei": 2, "qdrant": 4})
        run_a, run_b = isolation.new_run_id(), isolation.new_run_id()
        token = governor.acquire(run_a, "tei", 2)
        with self.assertRaisesRegex(isolation.IsolationError, "capacity unavailable"):
            governor.acquire(run_b, "tei", 1)
        with self.assertRaisesRegex(isolation.IsolationError, "not owned"):
            governor.release(token, run_b)
        governor.release(token, run_a)
        qdrant = governor.acquire(run_b, "qdrant", 4)
        with self.assertRaisesRegex(isolation.IsolationError, "capacity unavailable"):
            governor.acquire(run_a, "chrome", 1)
        governor.release(qdrant, run_b)

    def test_hostile_fixture_values_remain_inert_data(self):
        path = ROOT / "tests/e2e/fixtures/hostile/values.json"
        values = json.loads(path.read_text())["values"]
        self.assertTrue(any("$(touch" in value for value in values))
        self.assertTrue(any("東京" in value for value in values))
        self.assertTrue(any("\n" in value for value in values))
        self.assertFalse((ROOT / "SHOULD_NOT_EXIST").exists())


class FixtureServerTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server = fixtures.FixtureServer(("127.0.0.1", 0), "owned-secret", 0.02)
        cls.thread = threading.Thread(target=cls.server.serve_forever, daemon=True)
        cls.thread.start()
        cls.base = f"http://127.0.0.1:{cls.server.server_port}"

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown(); cls.server.server_close(); cls.thread.join()

    def request(self, path: str, payload=None, headers=None):
        data = None if payload is None else json.dumps(payload).encode()
        request = urllib.request.Request(self.base + path, data=data, headers=headers or {})
        with urllib.request.urlopen(request, timeout=1) as response:
            return response.status, response.read()

    def test_fixture_content_and_embedding_are_deterministic(self):
        self.assertEqual(self.request("/page")[1], fixtures.PAGE)
        payload = {"inputs": ["alpha", "beta"]}
        first = json.loads(self.request("/provider/tei/embed", payload)[1])
        second = json.loads(self.request("/provider/tei/embed", payload)[1])
        self.assertEqual(first, second)
        self.assertEqual(len(first), 2); self.assertTrue(all(len(vector) == 8 for vector in first))

    def test_provider_failure_contracts_cover_malformed_retry_dimension_and_partial(self):
        payload = {"inputs": ["alpha", "beta"]}
        malformed = self.request("/provider/tei/embed?mode=malformed", payload)[1]
        with self.assertRaises(json.JSONDecodeError): json.loads(malformed)
        with self.assertRaises(urllib.error.HTTPError) as transient:
            self.request("/provider/tei/embed?mode=transient", payload)
        try:self.assertEqual(transient.exception.code, 429)
        finally:transient.exception.close()
        self.assertEqual(len(json.loads(self.request("/provider/tei/embed?mode=transient", payload)[1])), 2)
        wrong = json.loads(self.request("/provider/tei/embed?mode=wrong-dimension", payload)[1])
        self.assertTrue(all(len(vector) == 7 for vector in wrong))
        partial = json.loads(self.request("/provider/tei/embed?mode=partial", payload)[1])
        self.assertEqual(len(partial), 1)

    def test_ssrf_sentinel_requires_unpredictable_owned_token(self):
        with self.assertRaises(urllib.error.HTTPError) as denied:
            self.request("/ssrf-sentinel")
        try:self.assertEqual(denied.exception.code, 403)
        finally:denied.exception.close()
        status, body = self.request("/ssrf-sentinel", headers={"X-Axon-E2E-SSRF-Token": "owned-secret"})
        self.assertEqual(status, 200); self.assertTrue(json.loads(body)["reached"])

    def test_llm_double_matches_openai_compatible_response_shape(self):
        _, body = self.request("/provider/llm/chat/completions", {"messages": [{"role": "user", "content": "hello"}]})
        result = json.loads(body)
        self.assertEqual(result["choices"][0]["message"]["content"], "fixture:hello")
        self.assertEqual(result["choices"][0]["finish_reason"], "stop")

    def test_non_loopback_binding_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "loopback"):
            fixtures.FixtureServer(("0.0.0.0", 0), "token", 1)


if __name__ == "__main__":
    unittest.main()
