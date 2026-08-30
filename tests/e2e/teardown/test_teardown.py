from __future__ import annotations

import importlib.util
import json
import sys
import tempfile
import sqlite3
import os
from contextlib import closing
from types import SimpleNamespace
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]
LIB = ROOT / "scripts/e2e/lib"
FIXTURES = ROOT / "tests/e2e/fixtures/teardown"


def load(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec); sys.modules[name] = module; spec.loader.exec_module(module); return module


teardown = load("test_teardown_engine", LIB / "teardown.py")
fake_module = load("test_fake_provider", FIXTURES / "fake_provider.py")
isolation = teardown.isolation


class TeardownTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.root = Path(self.temp.name)
        self.allocation = isolation.allocate(self.root / "runs", self.root / "manifests")
        self.path = Path(self.allocation["manifest"])

    def tearDown(self): self.temp.cleanup()

    def engine(self, *, unknown=False):
        header, resources = teardown.manifest_api.load(self.path)
        fake = fake_module.FakeProvider(teardown.manifest_api, header, resources, unknown=unknown)
        adapters = {kind: fake for kind in teardown.PROVIDER_TYPES}
        return teardown.Engine(self.path, adapters, global_timeout=5, phase_timeout=2), fake

    def test_dependency_order_exact_cleanup_and_idempotence(self):
        engine, fake = self.engine(); first = engine.run().json()
        self.assertTrue(first["success"], first); self.assertFalse(Path(self.allocation["data_dir"]).exists())
        self.assertEqual([name for name, _ in teardown.PHASES], [item["name"] for item in first["phases"]])
        engine2 = teardown.Engine(self.path, {kind: fake for kind in teardown.PROVIDER_TYPES})
        second = engine2.run().json(); self.assertTrue(second["success"], second)

    def test_qdrant_marker_point_is_generation_bound_and_deterministic(self):
        header, resources = teardown.manifest_api.load(self.path)
        collection = next(item for item in resources if item.resource_type == "collection")
        first = teardown.manifest_api.qdrant_ownership_point(header, collection)
        second = teardown.manifest_api.qdrant_ownership_point(header, collection)
        self.assertEqual(first, second); self.assertTrue(first["payload"]["axon_e2e_marker"])
        self.assertEqual(collection.metadata["ownership_generation"],
                         first["payload"]["axon_e2e_ownership"]["generation"])

    def test_marker_mismatch_refuses_without_deleting(self):
        engine, fake = self.engine()
        key = next(key for key in fake.state if key[0] == "collection")
        fake.state[key]["marker"]["run_id"] = "axon_e2e_foreign"
        report = engine.run().json()
        self.assertFalse(report["success"]); self.assertNotIn(key, fake.deleted)
        self.assertTrue(any(item["class"] == "collection" for item in report["refused"]))

    def test_unknown_post_delete_state_fails_closed(self):
        engine, _ = self.engine(unknown=True); report = engine.run().json()
        self.assertFalse(report["success"])
        self.assertTrue(any(item["reason"] == "post-delete state is unknown" for item in report["refused"]))

    def test_high_water_batches_exact_resources_only(self):
        manifest = isolation.Manifest.open(self.path)
        for index in range(128): manifest.register("collection", f"{self.allocation['run_id']}_{index}",
                                                   {"ownership_generation": f"{index:064x}"})
        engine, fake = self.engine(); report = engine.run().json()
        self.assertTrue(report["success"], report)
        self.assertEqual(129, sum(1 for kind, _ in fake.deleted if kind == "collection"))
        self.assertGreater(fake.batch_calls, 0)

    def test_provider_reservation_is_cleaned_exactly_once_in_application_phase(self):
        reservation = f"{self.allocation['run_id']}_reservation"
        isolation.Manifest.open(self.path).register("provider_reservation", reservation, {
            "run_id": self.allocation["run_id"], "provider": "llm", "permits": 1, "requests": 1, "retries": 0,
        })
        db_path = Path(self.allocation["sqlite"])
        with closing(sqlite3.connect(db_path)) as db, db:
            db.execute("CREATE TABLE provider_reservations (reservation_id TEXT PRIMARY KEY, status TEXT, granted_units INTEGER)")
            db.execute("INSERT INTO provider_reservations VALUES (?, 'active', 1)", (reservation,))
        header, resources = teardown.manifest_api.load(self.path); fake = fake_module.FakeProvider(teardown.manifest_api, header, resources)
        durable = teardown.provider_api.DurableStateAdapter(header, teardown.manifest_api)
        adapters = {kind: fake for kind in teardown.PROVIDER_TYPES}; adapters["provider_reservation"] = durable
        report = teardown.Engine(self.path, adapters).run().json()
        self.assertTrue(report["success"], report); self.assertEqual(1, report["classes"]["provider_reservation"]["count"])
        application = next(item for item in report["phases"] if item["name"] == "application")
        child_phase = next(item for item in report["phases"] if item["name"] == "provider-children")
        collection_phase = next(item for item in report["phases"] if item["name"] == "provider-collections")
        self.assertGreaterEqual(application["count"], 1); self.assertEqual(0, child_phase["count"])
        self.assertEqual(1, collection_phase["count"])

    def test_retention_requires_redaction_approval(self):
        manifest = isolation.Manifest.open(self.path)
        identity = f"{self.allocation['run_id']}_evidence"
        state = Path(self.allocation["data_dir"]).parent / "evidence.txt"; state.write_text("token=super-secret-value\nsafe line\n")
        manifest.register("evidence", identity, {"retain": True, "state_file": str(state)})
        header, resources = teardown.manifest_api.load(self.path); fake = fake_module.FakeProvider(teardown.manifest_api, header, resources)
        durable = teardown.provider_api.DurableStateAdapter(header, teardown.manifest_api)
        adapters = {kind: fake for kind in teardown.PROVIDER_TYPES}; adapters["evidence"] = durable
        report = teardown.Engine(self.path, adapters).run().json()
        self.assertTrue(report["success"], report); retained = report["retained"][0]
        self.assertEqual("evidence", retained["class"]); self.assertGreater(int(retained["redactions"]), 0)
        sanitized = Path(retained["path"]); self.assertNotIn("super-secret-value", sanitized.read_text())

    def test_failed_redaction_destroys_evidence_and_never_retains_it(self):
        manifest = isolation.Manifest.open(self.path); identity = f"{self.allocation['run_id']}_unsafe_evidence"
        state = Path(self.allocation["data_dir"]).parent / "binary-evidence"; state.write_bytes(b"\xff\xfe")
        manifest.register("evidence", identity, {"retain": True, "state_file": str(state)})
        header, resources = teardown.manifest_api.load(self.path); fake = fake_module.FakeProvider(teardown.manifest_api, header, resources)
        durable = teardown.provider_api.DurableStateAdapter(header, teardown.manifest_api)
        adapters = {kind: fake for kind in teardown.PROVIDER_TYPES}; adapters["evidence"] = durable
        report = teardown.Engine(self.path, adapters).run().json()
        self.assertTrue(report["success"], report); self.assertEqual([], report["retained"]); self.assertFalse(state.exists())

    def test_failed_redaction_suppresses_artifact_and_upload_residue(self):
        manifest = isolation.Manifest.open(self.path); run_id = self.allocation["run_id"]
        evidence = f"{run_id}_unsafe_evidence"; artifact = f"{run_id}_unsafe_artifact"; upload = f"{run_id}_unsafe_upload"
        state = Path(self.allocation["data_dir"]).parent / "unsafe-evidence"; state.write_bytes(b"\xff\xfe")
        manifest.register("evidence", evidence, {"retain": True, "state_file": str(state)})
        manifest.register("artifact", artifact, {"redaction_parent": evidence})
        manifest.register("upload", upload, {"redaction_parent": evidence})
        header, resources = teardown.manifest_api.load(self.path); fake = fake_module.FakeProvider(teardown.manifest_api, header, resources)
        adapters = {kind: fake for kind in teardown.PROVIDER_TYPES}
        adapters["evidence"] = teardown.provider_api.DurableStateAdapter(header, teardown.manifest_api)
        report = teardown.Engine(self.path, adapters).run().json()
        self.assertTrue(report["success"], report); self.assertEqual([], report["retained"])
        self.assertFalse(state.exists())
        for resource in (("artifact", artifact), ("upload", upload)):
            self.assertIn(resource, fake.deleted)

    def test_unregistered_lookalike_shared_resource_is_unchanged(self):
        engine, fake = self.engine(); shared = ("collection", "axon_e2e_lookalike_not_registered")
        fake.state[shared] = {"exists": True, "marker": {"run_id": "foreign"}}
        report = engine.run().json()
        self.assertTrue(report["success"], report); self.assertTrue(fake.state[shared]["exists"])
        self.assertNotIn(shared, fake.deleted)

    def test_shared_provider_operator_and_tailnet_resources_are_invariant(self):
        engine, fake = self.engine()
        shared = [(kind, f"shared-production-{kind}") for kind in (
            "collection", "provider_reservation", "container", "network", "volume", "tailscale_node",
            "qdrant_alias", "qdrant_snapshot", "point", "payload_index", "watch", "upload",
        )]
        for key in shared: fake.state[key] = {"exists": True, "marker": {"run_id": "operator-owned"}}
        report = engine.run().json(); self.assertTrue(report["success"], report)
        for key in shared:
            self.assertTrue(fake.state[key]["exists"]); self.assertNotIn(key, fake.deleted)

    def test_failure_after_each_local_setup_stage_leaves_no_residual(self):
        manifest = isolation.Manifest.open(self.path); root = Path(self.allocation["data_dir"]).parent
        for index, kind in enumerate(("cache", "chrome_diagnostic", "chrome_profile", "credential_file",
                                      "download", "feed_fixture", "git_fixture", "http_cache", "output",
                                      "screenshot", "socket", "sqlite_sidecar", "temp_path", "warc")):
            path = root / f"stage-{index}-{kind}"; path.write_text("assertion-failure residue")
            manifest.register(kind, str(path), {"setup_stage": index})
        engine, _ = self.engine(); report = engine.run().json()
        self.assertTrue(report["success"], report)
        self.assertFalse(any((root / f"stage-{index}-{kind}").exists() for index, kind in enumerate(
            ("cache", "chrome_diagnostic", "chrome_profile", "credential_file", "download", "feed_fixture",
             "git_fixture", "http_cache", "output", "screenshot", "socket", "sqlite_sidecar", "temp_path", "warc"))))

    def test_provider_crash_fails_run_but_other_classes_still_teardown(self):
        header, resources = teardown.manifest_api.load(self.path)
        target = next(item for item in resources if item.resource_type == "collection")
        fake = fake_module.FakeProvider(teardown.manifest_api, header, resources,
                                        fail_delete={(target.resource_type, target.identity)})
        report = teardown.Engine(self.path, {kind: fake for kind in teardown.PROVIDER_TYPES}).run().json()
        self.assertFalse(report["success"]); self.assertTrue(any("provider outage" in item["reason"] for item in report["refused"]))
        self.assertTrue(Path(self.allocation["data_dir"]).exists())
        self.assertTrue(any("preserved after upstream cleanup refusal" in item["reason"] for item in report["refused"]))

    def test_sqlite_sidecars_and_port_lease_are_removed(self):
        sqlite = Path(self.allocation["sqlite"])
        for suffix in ("", "-wal", "-shm", "-journal"): Path(str(sqlite) + suffix).write_text("fixture")
        manifest = isolation.Manifest.open(self.path)
        reservation = isolation.allocate_port(self.root / "ports", self.allocation["run_id"], manifest)
        reservation.close(); lease = reservation.lease_path
        engine, _ = self.engine(); report = engine.run().json()
        self.assertTrue(report["success"], report); self.assertFalse(lease.exists())
        self.assertFalse(any(Path(str(sqlite) + suffix).exists() for suffix in ("", "-wal", "-shm", "-journal")))

    @unittest.skipIf(sys.platform == "win32", "POSIX escalation contract")
    def test_term_immune_owned_process_is_force_killed(self):
        manifest = isolation.Manifest.open(self.path)
        managed = isolation.spawn_owned_process(manifest, Path(self.allocation["data_dir"]).parent,
            [sys.executable, "-c", "import signal,time; signal.signal(signal.SIGTERM, signal.SIG_IGN); time.sleep(30)"])
        import time; time.sleep(0.1)
        engine, _ = self.engine(); report = engine.run().json()
        managed.process.wait(timeout=3)
        outcomes = [item.get("outcome") for item in report["removed"] if item["class"] == "process"]
        self.assertTrue(report["success"], report); self.assertIn("force-killed", outcomes)

    def test_delayed_old_process_record_cannot_kill_recycled_pid(self):
        header, _ = teardown.manifest_api.load(self.path); adapter = teardown.LocalAdapter(header)
        nonce_file = Path(self.allocation["data_dir"]).parent / "old-process.nonce"; nonce = "n" * 64; nonce_file.write_text(nonce)
        resource = SimpleNamespace(resource_type="process", identity=str(os.getpid()), metadata={
            "start_time": "stale-start-time", "nonce": nonce, "nonce_file": str(nonce_file), "process_group": os.getpid(),
        })
        with self.assertRaisesRegex(teardown.CleanupError, "identity changed before TERM"):
            adapter.delete(resource, float("inf"))


if __name__ == "__main__": unittest.main()
