from __future__ import annotations

import importlib.util
import json
import os
import signal
import sqlite3
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
RUNNER = ROOT / "scripts/e2e/run-upgrade.py"
OUTER_CLEANUP = ROOT / "scripts/e2e/cleanup-owned-runs.py"


def load(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


builder = load("upgrade_builder_tests", ROOT / "scripts/e2e/build-upgrade-fixture.py")
runner = load("upgrade_runner_tests", ROOT / "scripts/e2e/run-upgrade.py")


class UpgradeFixtureTests(unittest.TestCase):
    def _wait_for_record(self, record: Path) -> Path:
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            if record.exists() and record.read_text().strip(): return Path(record.read_text().strip())
            time.sleep(0.02)
        self.fail("upgrade runner did not publish its owned root")

    def _wait_for_manifest(self, root: Path) -> None:
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            manifests = list(root.rglob("resources.jsonl"))
            if manifests and len(manifests[0].read_text().splitlines()) >= 3: return
            time.sleep(0.02)
        self.fail("upgrade runner did not publish its signed manifest")

    def test_builder_is_deterministic_and_seeds_all_persistent_domains(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); one = root / "one.db"; two = root / "two.db"
            first = builder.build(builder.DEFAULT_MANIFEST, one)
            second = builder.build(builder.DEFAULT_MANIFEST, two)
            self.assertEqual(first["sha256"], second["sha256"])
            conn = sqlite3.connect(one)
            try:
                tables = {row[0] for row in conn.execute("SELECT name FROM sqlite_schema WHERE type='table'")}
                for required in ("jobs", "job_events", "job_artifacts", "axon_source_watches", "sources",
                                 "source_generations", "graph_nodes", "memory_records"):
                    self.assertIn(required, tables)
            finally:
                conn.close()

    def test_provenance_tampering_and_missing_fields_fail_closed(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); original = json.loads(builder.DEFAULT_MANIFEST.read_text())
            original["migrations"][0]["sha256"] = "0" * 64
            tampered = root / "tampered.json"; tampered.write_text(json.dumps(original))
            with self.assertRaisesRegex(builder.FixtureError, "fixture input drift"):
                builder.build(tampered, root / "bad.db")
            original.pop("source")
            missing = root / "missing.json"; missing.write_text(json.dumps(original))
            with self.assertRaisesRegex(builder.FixtureError, "missing fields"):
                builder.build(missing, root / "missing.db")

    def test_generator_drift_fails_closed(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); manifest = json.loads(builder.DEFAULT_MANIFEST.read_text())
            manifest["generator"]["version"] = 2
            drifted = root / "generator-drift.json"; drifted.write_text(json.dumps(manifest))
            with self.assertRaisesRegex(builder.FixtureError, "generator drift"):
                builder.build(drifted, root / "drifted.db")

    def test_supported_window_is_explicit_and_release_blobs_are_excluded(self):
        manifest = json.loads(builder.DEFAULT_MANIFEST.read_text())
        policy = manifest["support_policy"]
        self.assertEqual(policy["supported_transitions"], ["schema-epoch-1 receipt-prefix to current schema-epoch-1"])
        self.assertEqual({item["window"] for item in policy["excluded_release_windows"]}, {
            "N-1/N-2 product release database blobs", "client-server.v0 and older"})
        collection = manifest["explicit_collection_migration"]
        self.assertEqual(collection["driver"], "tests/e2e/scenarios/admin/hermetic_entry.py")
        self.assertIn("non-owned refusal", collection["assertions"])

    def test_negative_fixture_mutations_are_distinct(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); source = root / "source.db"; builder.build(builder.DEFAULT_MANIFEST, source)
            for mutation in ("forward_epoch", "receipt_tamper", "partial_fixture", "interrupted"):
                target = root / f"{mutation}.db"
                runner.incompatible_copy(source, target, mutation)
                self.assertNotEqual(target.read_bytes(), source.read_bytes())

    def test_setup_failure_uses_manifest_teardown_and_leaves_no_database(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); output = root / "upgrade"; record = root / "owned.txt"
            result = subprocess.run([sys.executable, str(RUNNER), "--binary", sys.executable,
                "--output", str(output), "--manifest-root", str(root / "manifests"),
                "--run-root-record", str(record), "--failure-at", "after_setup"],
                cwd=ROOT, capture_output=True, text=True, timeout=15)
            self.assertNotEqual(0, result.returncode)
            owned = Path(record.read_text())
            self.assertFalse(owned.exists())
            self.assertFalse(any(output.rglob("*.db*")))

    def test_default_output_is_fresh_for_runner_owned_creation(self):
        result = subprocess.run(
            [sys.executable, str(RUNNER), "--binary", sys.executable,
             "--failure-at", "after_setup"],
            cwd=ROOT, capture_output=True, text=True, timeout=15,
        )
        self.assertNotEqual(0, result.returncode)
        self.assertIn("injected failure after setup", result.stderr)
        self.assertNotIn("FileExistsError", result.stderr)

    @unittest.skipIf(os.name == "nt", "POSIX signal contract")
    def test_sigterm_tears_down_owned_upgrade_state(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); output = root / "upgrade"; record = root / "owned.txt"
            proc = subprocess.Popen([sys.executable, str(RUNNER), "--binary", sys.executable,
                "--output", str(output), "--manifest-root", str(root / "manifests"),
                "--run-root-record", str(record), "--test-hold-seconds", "60"],
                cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
            owned = self._wait_for_record(record); os.kill(proc.pid, signal.SIGTERM)
            proc.communicate(timeout=15)
            self.assertNotEqual(0, proc.returncode)
            self.assertFalse(owned.exists())
            self.assertFalse(any(output.rglob("*.db*")))

    @unittest.skipIf(os.name == "nt", "POSIX hard-kill contract")
    def test_outer_cleanup_discovers_hard_killed_upgrade_and_preserves_evidence(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); output = root / "upgrade"; record = root / "owned.txt"; authority = root / "owned-runs"
            registry = root / "registry/owned.json"; env = {**os.environ, "AXON_E2E_CLEANUP_REGISTRY": str(registry)}
            proc = subprocess.Popen([sys.executable, str(RUNNER), "--binary", sys.executable,
                "--output", str(output), "--manifest-root", str(authority / "manifests"),
                "--run-root-record", str(record), "--test-hold-seconds", "60"],
                cwd=ROOT, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
            owned = self._wait_for_record(record); self._wait_for_manifest(authority)
            os.kill(proc.pid, signal.SIGKILL); proc.communicate(timeout=10)
            self.assertTrue(owned.exists(), "hard-kill fixture did not abandon state")
            cleanup_report = root / "outer.json"
            cleaned = subprocess.run([sys.executable, str(OUTER_CLEANUP), "--manifest-root", str(authority),
                "--registry", str(registry), "--report", str(cleanup_report)], cwd=ROOT,
                capture_output=True, text=True, timeout=20)
            self.assertEqual(0, cleaned.returncode, cleaned.stderr + cleaned.stdout)
            self.assertFalse(owned.exists())
            self.assertTrue((output / "evidence").exists())
            self.assertTrue(json.loads(cleanup_report.read_text())["success"])


if __name__ == "__main__":
    unittest.main()
