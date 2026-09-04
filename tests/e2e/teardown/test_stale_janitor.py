from __future__ import annotations

import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]; LIB = ROOT / "scripts/e2e/lib"
def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path); module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module; spec.loader.exec_module(module); return module


janitor = load("test_stale_janitor_module", LIB / "stale-janitor.py")
isolation = janitor.manifest_api.isolation


class FakeEngine:
    calls = []
    def __init__(self, path): self.path = path
    def provider_lease_state(self): return {"heartbeat_unix_ms": 1_000_000, "expires_unix_ms": 2_000_000}
    def run(self):
        self.calls.append(self.path)
        class Result:
            @staticmethod
            def json(): return {"success": True}
        return Result()


class JanitorTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.root = Path(self.temp.name)
        allocation = isolation.allocate(self.root / "runs", self.root / "manifests")
        self.path = Path(allocation["manifest"]); self.header, _ = janitor.manifest_api.load(self.path)
        self.now = 10_000_000

    def tearDown(self): self.temp.cleanup()
    def registry(self, **changes):
        entry = {"run_id": self.header.run_id, "manifest": str(self.path), "manifest_digest": self.header.digest,
                 "heartbeat_unix_ms": 1_000_000, "expires_unix_ms": 2_000_000, **changes}
        path = self.root / "registry.json"; janitor.write_registry(path, [entry], b"k" * 32); return path

    def test_preview_is_default_and_does_not_delete(self):
        FakeEngine.calls.clear(); report = janitor.run(self.registry(), self.root / "lease", now_ms=self.now, engine_factory=FakeEngine)
        self.assertEqual("preview", report["mode"]); self.assertEqual([], FakeEngine.calls); self.assertEqual(1, len(report["selected"]))

    def test_execute_revalidates_and_holds_exclusive_lease(self):
        FakeEngine.calls.clear(); report = janitor.run(self.registry(), self.root / "lease", execute=True,
                                                        now_ms=self.now, engine_factory=FakeEngine)
        self.assertTrue(report["success"]); self.assertEqual([self.path.resolve()], FakeEngine.calls)

    def test_active_wrong_digest_and_future_heartbeat_are_refused(self):
        cases = ({"expires_unix_ms": self.now}, {"manifest_digest": "0" * 64},
                 {"heartbeat_unix_ms": self.now + 400_000})
        for changes in cases:
            with self.subTest(changes=changes):
                selected, refused = janitor.select_stale(self.registry(**changes), now_ms=self.now)
                self.assertEqual([], selected); self.assertEqual(1, len(refused))

    def test_provider_native_lease_is_rechecked_immediately_before_delete(self):
        current = self.now
        class ActiveEngine(FakeEngine):
            def provider_lease_state(self):
                return {"heartbeat_unix_ms": current, "expires_unix_ms": current + 1_000_000}
        FakeEngine.calls.clear()
        report = janitor.run(self.registry(), self.root / "lease", execute=True,
                             now_ms=self.now, engine_factory=ActiveEngine)
        self.assertFalse(report["success"]); self.assertEqual([], FakeEngine.calls)
        self.assertIn("provider-native lease is active", report["refused"][0]["reason"])

    def test_provider_native_lease_must_match_signed_registry(self):
        class ChangedEngine(FakeEngine):
            def provider_lease_state(self):
                return {"heartbeat_unix_ms": 900_000, "expires_unix_ms": 2_000_000}
        report = janitor.run(self.registry(), self.root / "lease", execute=True,
                             now_ms=self.now, engine_factory=ChangedEngine)
        self.assertFalse(report["success"]); self.assertIn("differs", report["refused"][0]["reason"])

    def test_existing_cleanup_lease_refuses_concurrent_janitor(self):
        lease = self.root / "lease"; lease.mkdir()
        with self.assertRaisesRegex(janitor.JanitorError, "already held"):
            janitor.run(self.registry(), lease, now_ms=self.now)

    def test_registry_tampering_is_detected(self):
        path = self.registry(); envelope = json.loads(path.read_text())
        envelope["payload"]["runs"][0]["expires_unix_ms"] = 0; path.write_text(json.dumps(envelope))
        with self.assertRaisesRegex(janitor.JanitorError, "integrity failure"):
            janitor.load_registry(path)


if __name__ == "__main__": unittest.main()
