from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
import secrets
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]


def load(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    assert spec and spec.loader
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


isolation = load("stop_descriptor_isolation", ROOT / "scripts/e2e/lib/run-isolation.py")


class StopDescriptorTests(unittest.TestCase):
    def allocation(self, root: Path):
        run_id=f"axon_e2e_{secrets.token_hex(12)}";run_root=root/"runs"/run_id;data_dir=run_root/"data"
        data_dir.mkdir(parents=True);manifest=isolation.Manifest.create(root/"manifests",run_id,data_dir)
        manifest.register("data_dir",str(data_dir))
        allocation={"run_id":run_id,"run_root":str(run_root),"data_dir":str(data_dir),"manifest":str(manifest.path)}
        reservation = isolation.allocate_port(root / "leases", run_id, manifest)
        reservation.close()
        descriptor = {
            "schema": 1,
            "status": "running",
            "run_id": allocation["run_id"],
            "run_root": allocation["run_root"],
            "ownership_manifest": allocation["manifest"],
            "cleanup_report": str(Path(allocation["manifest"]).parent / "cleanup-report.json"),
            "ports": [reservation.port],
            "process_ids": {},
        }
        path = Path(allocation["run_root"]) / "descriptor.json"
        path.write_text(json.dumps(descriptor))
        return allocation, descriptor, path

    def test_tampered_raw_pid_is_ignored_in_favor_of_signed_manifest(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); allocation, descriptor, path = self.allocation(root)
            innocent = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(30)"], start_new_session=True)
            try:
                descriptor["process_ids"] = {"tampered": innocent.pid}; path.write_text(json.dumps(descriptor))
                completed = subprocess.run([sys.executable, str(ROOT / "scripts/e2e/stop-hermetic-stack.py"), str(path)],
                                           cwd=ROOT, capture_output=True, text=True, timeout=15)
                self.assertEqual(0, completed.returncode, completed.stderr + completed.stdout)
                self.assertIsNone(innocent.poll(), "untrusted descriptor PID was signaled")
                self.assertFalse(Path(allocation["data_dir"]).exists())
            finally:
                innocent.terminate(); innocent.wait(timeout=5)

    def test_manifest_binding_tamper_fails_before_cleanup(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); allocation, descriptor, path = self.allocation(root)
            descriptor["run_id"] += "_tampered"; path.write_text(json.dumps(descriptor))
            completed = subprocess.run([sys.executable, str(ROOT / "scripts/e2e/stop-hermetic-stack.py"), str(path)],
                                       cwd=ROOT, capture_output=True, text=True, timeout=15)
            self.assertNotEqual(0, completed.returncode)
            self.assertTrue(Path(allocation["data_dir"]).exists())


if __name__ == "__main__":
    unittest.main()
