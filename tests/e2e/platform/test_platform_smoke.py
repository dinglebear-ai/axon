import importlib.util
import json
import os
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
RUNNER = ROOT / "scripts/e2e/run-platform-smoke.py"
OUTER_CLEANUP = ROOT / "scripts/e2e/cleanup-owned-runs.py"


class PlatformSmokeTests(unittest.TestCase):
    def _wait_for_record(self, record: Path) -> Path:
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            if record.exists() and record.read_text().strip(): return Path(record.read_text().strip())
            time.sleep(0.02)
        self.fail("runner did not publish its owned root")

    def _wait_for_manifest(self, root: Path) -> None:
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            manifests = list(root.rglob("resources.jsonl"))
            if manifests and len(manifests[0].read_text().splitlines()) >= 4: return
            time.sleep(0.02)
        self.fail("runner did not publish its signed manifest")

    def test_runner_emits_release_qualification_input_and_cleans_up(self):
        with tempfile.TemporaryDirectory() as directory:
            report = Path(directory) / "platform.json"
            result = subprocess.run([sys.executable, str(RUNNER), "--report", str(report), "--tested-sha", "a" * 40],
                                    cwd=ROOT, capture_output=True, text=True, timeout=30)
            self.assertEqual(0, result.returncode, result.stderr + result.stdout)
            payload = json.loads(report.read_text())
            self.assertEqual("passed", payload["summary"]["status"])
            self.assertEqual("platform_smoke", payload["policy"]["catalog_tag"])
            selected = [item["id"] for item in json.loads((ROOT / "tests/e2e/catalog/catalog.json").read_text())["scenarios"]
                        if "platform_smoke" in item["tags"]]
            emitted = [item["scenario_id"] for item in payload["scenarios"] if item["scenario_id"] != "platform.portable.contract"]
            self.assertEqual(sorted(selected), emitted)
            self.assertTrue(all(item["cleanup"]["success"] for item in payload["scenarios"]))
            self.assertTrue(all(item["cleanup"]["residuals"] == [] for item in payload["scenarios"]))

    def test_selection_is_derived_from_catalog_tags(self):
        catalog = json.loads((ROOT / "tests/e2e/catalog/catalog.json").read_text())
        selected = [item["id"] for item in catalog["scenarios"] if "platform_smoke" in item["tags"]]
        self.assertTrue(selected)
        self.assertEqual(len(selected), len(set(selected)))

    def test_setup_failure_removes_entire_owned_root(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory); report = base / "failure.json"; record = base / "root.txt"
            result = subprocess.run([sys.executable, str(RUNNER), "--binary", sys.executable,
                "--report", str(report), "--root-base", str(base), "--run-root-record", str(record),
                "--failure-at", "after_setup"], cwd=ROOT, capture_output=True, text=True, timeout=15)
            self.assertEqual(2, result.returncode, result.stderr + result.stdout)
            owned = Path(record.read_text())
            self.assertFalse(owned.exists())
            self.assertFalse(any(base.rglob("jobs.db*")))

    @unittest.skipIf(os.name == "nt", "POSIX signal contract")
    def test_sigterm_runs_teardown_before_exit(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory); report = base / "signal.json"; record = base / "root.txt"
            proc = subprocess.Popen([sys.executable, str(RUNNER), "--binary", sys.executable,
                "--report", str(report), "--root-base", str(base), "--run-root-record", str(record),
                "--test-hold-seconds", "60"], cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
            owned = self._wait_for_record(record); os.kill(proc.pid, signal.SIGTERM)
            stdout, stderr = proc.communicate(timeout=15)
            self.assertEqual(2, proc.returncode, stderr + stdout)
            self.assertFalse(owned.exists())
            self.assertFalse(any(base.rglob("jobs.db*")))

    @unittest.skipIf(os.name == "nt", "POSIX hard-kill contract")
    def test_outer_cleanup_discovers_hard_killed_run(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory); report = base / "signal.json"; record = base / "root.txt"; registry = base / "registry/owned.json"
            env = {**os.environ, "AXON_E2E_CLEANUP_REGISTRY": str(registry)}
            proc = subprocess.Popen([sys.executable, str(RUNNER), "--binary", sys.executable,
                "--report", str(report), "--root-base", str(base), "--run-root-record", str(record),
                "--test-hold-seconds", "60"], cwd=ROOT, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
            owned = self._wait_for_record(record); self._wait_for_manifest(base)
            os.kill(proc.pid, signal.SIGKILL); proc.communicate(timeout=10)
            self.assertTrue(owned.exists(), "hard-kill fixture did not abandon state")
            cleanup_report = base / "outer.json"
            cleaned = subprocess.run([sys.executable, str(OUTER_CLEANUP), "--manifest-root", str(base),
                "--registry", str(registry), "--report", str(cleanup_report)], cwd=ROOT,
                capture_output=True, text=True, timeout=20)
            self.assertEqual(0, cleaned.returncode, cleaned.stderr + cleaned.stdout)
            self.assertFalse(owned.exists())
            self.assertTrue(json.loads(cleanup_report.read_text())["success"])


if __name__ == "__main__": unittest.main()
