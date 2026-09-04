from __future__ import annotations

import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]
SCRIPT = ROOT / "scripts/e2e/cleanup-owned-runs.py"


def load():
    spec = importlib.util.spec_from_file_location("test_outer_cleanup_module", SCRIPT)
    module = importlib.util.module_from_spec(spec); sys.modules[spec.name] = module; spec.loader.exec_module(module)
    return module


outer = load(); isolation = outer.teardown.manifest_api.isolation


class OuterCleanupTests(unittest.TestCase):
    def test_all_mode_removes_only_signed_manifest_owned_paths_and_is_idempotent(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); run_id = isolation.new_run_id(); run_root = root / "target/e2e/runs" / run_id
            data = run_root / "data"; data.mkdir(parents=True)
            manifest = isolation.Manifest.create(root / "target/e2e/manifests", run_id, data)
            manifest.register("data_dir", str(data)); foreign = root / "foreign"; foreign.write_text("keep")
            receipt = outer.cleanup(root / "target/e2e", stale_seconds=None, live_gateways=False)
            self.assertTrue(receipt["success"], receipt); self.assertTrue(foreign.exists())
            self.assertFalse(data.exists())
            again = outer.cleanup(root / "target/e2e", stale_seconds=None, live_gateways=False)
            self.assertTrue(again["success"], again)

    def test_stale_mode_preserves_recent_run(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); allocation = isolation.allocate(root / "runs", root / "manifests")
            header, _ = outer.teardown.manifest_api.load(Path(allocation["manifest"]))
            receipt = outer.cleanup(root, stale_seconds=3600, live_gateways=False, now_ms=header.created_unix_ms + 1000)
            self.assertTrue(receipt["success"]); self.assertEqual("active-age-guard", receipt["skipped"][0]["reason"])
            self.assertTrue(Path(allocation["run_root"]).exists())

    def test_tampered_manifest_fails_closed_without_deleting(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); allocation = isolation.allocate(root / "runs", root / "manifests")
            manifest = Path(allocation["manifest"]); manifest.write_text(manifest.read_text() + json.dumps({"bad": True}) + "\n")
            receipt = outer.cleanup(root, stale_seconds=None, live_gateways=False)
            self.assertFalse(receipt["success"]); self.assertTrue(Path(allocation["run_root"]).exists())

    def test_absent_run_root_is_canonical_completion_marker(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); run_id = isolation.new_run_id(); data = root / "runs" / run_id / "data"
            data.mkdir(parents=True); manifest = isolation.Manifest.create(root / "manifests", run_id, data)
            manifest.register("collection", f"{run_id}_collection", {"ownership_generation": "a" * 64})
            data.rmdir(); data.parent.rmdir()
            receipt = outer.cleanup(root, stale_seconds=None, live_gateways=False)
            self.assertTrue(receipt["success"], receipt)
            self.assertEqual("retired-completed-authority", receipt["cleanups"][0]["outcome"])
            self.assertFalse(manifest.path.parent.exists())

    def test_registry_discovers_owned_run_outside_scan_root_after_launcher_death(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); scan = root / "checkout/target/e2e"; scan.mkdir(parents=True)
            run_id = isolation.new_run_id(); data = root / "external-runs" / run_id / "data"; data.mkdir(parents=True)
            manifest = isolation.Manifest.create(root / "external-manifests", run_id, data)
            registry = root / "stable/registry.json"
            registered = outer.register(registry, manifest.path); self.assertTrue(registered["success"])
            # Registration precedes provisioning; valid append-only growth must
            # retain the immutable registry binding.
            manifest.register("data_dir", str(data))
            receipt = outer.cleanup(scan, stale_seconds=None, live_gateways=False, registry=registry)
            self.assertTrue(receipt["success"], receipt); self.assertFalse(data.exists())
            self.assertEqual([], outer._registry_payload(registry)["runs"])

    def test_dangling_and_tampered_registry_entries_fail_closed(self):
        for mutation in ("dangling", "tampered"):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as directory:
                root = Path(directory); run_id = isolation.new_run_id(); data = root / "runs" / run_id / "data"; data.mkdir(parents=True)
                manifest = isolation.Manifest.create(root / "manifests", run_id, data); registry = root / "registry.json"
                outer.register(registry, manifest.path)
                if mutation == "dangling": manifest.path.unlink()
                else:
                    envelope = json.loads(registry.read_text()); envelope["payload"]["runs"][0]["run_id"] = "axon_e2e_tampered"
                    registry.write_text(json.dumps(envelope))
                receipt = outer.cleanup(root / "empty", stale_seconds=None, live_gateways=False, registry=registry)
                self.assertFalse(receipt["success"], receipt); self.assertTrue(data.exists())

    def test_completed_authority_with_unexpected_file_or_symlink_is_refused_untouched(self):
        for kind in ("file", "symlink"):
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as directory:
                root = Path(directory); run_id = isolation.new_run_id(); data = root / "runs" / run_id / "data"
                data.mkdir(parents=True); manifest = isolation.Manifest.create(root / "manifests", run_id, data)
                data.rmdir(); data.parent.rmdir(); unexpected = manifest.path.parent / "unexpected"
                if kind == "file": unexpected.write_text("foreign")
                else: unexpected.symlink_to(root / "outside")
                receipt = outer.cleanup(root, stale_seconds=None, live_gateways=False)
                self.assertFalse(receipt["success"], receipt)
                self.assertTrue(manifest.path.exists()); self.assertTrue(unexpected.exists() or unexpected.is_symlink())


if __name__ == "__main__": unittest.main()
