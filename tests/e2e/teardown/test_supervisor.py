from __future__ import annotations

import importlib.util
import sys
import signal
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path); module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module; spec.loader.exec_module(module); return module

supervisor = load("test_e2e_supervisor", ROOT / "scripts/e2e/lib/run-with-teardown.py")
isolation = supervisor.teardown.isolation


class SupervisorTests(unittest.TestCase):
    def run_case(self, script: str, timeout: float = 2):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp); allocation = isolation.allocate(root / "runs", root / "manifests")
            header, resources = supervisor.teardown.manifest_api.load(Path(allocation["manifest"]))
            fake_path = ROOT / "tests/e2e/fixtures/teardown/fake_provider.py"
            fake_module = load(f"fake_provider_{id(allocation)}", fake_path)
            fake = fake_module.FakeProvider(supervisor.teardown.manifest_api, header, resources)
            old_engine = supervisor.teardown.Engine
            class Engine(old_engine):
                def __init__(self, manifest, _adapters=None):
                    super().__init__(manifest, {kind: fake for kind in supervisor.teardown.PROVIDER_TYPES})
            supervisor.teardown.Engine = Engine
            try: report = supervisor.supervise(Path(allocation["manifest"]), [sys.executable, "-c", script], timeout=timeout)
            finally: supervisor.teardown.Engine = old_engine
            self.assertTrue(report["cleanup"]["success"], report); self.assertFalse(Path(allocation["data_dir"]).exists())
            return report

    def test_child_and_chrome_style_crash_still_teardown(self):
        report = self.run_case("raise SystemExit(17)")
        self.assertEqual(17, report["child_returncode"]); self.assertFalse(report["success"])

    def test_assertion_death_before_provider_persistence_still_teardown(self):
        report = self.run_case("assert False, 'injected before persistence'")
        self.assertNotEqual(0, report["child_returncode"]); self.assertFalse(report["success"])

    def test_timeout_still_teardown(self):
        report = self.run_case("import time; time.sleep(30)", timeout=.05)
        self.assertTrue(report["timed_out"]); self.assertFalse(report["success"])

    @unittest.skipIf(sys.platform == "win32", "POSIX signal contract")
    def test_sigint_workflow_cancellation_still_teardown(self):
        report = self.run_case("import os,signal; os.kill(os.getppid(), signal.SIGINT)")
        self.assertEqual(signal.SIGINT, report["signal"]); self.assertFalse(report["success"])

    @unittest.skipIf(sys.platform == "win32", "POSIX signal contract")
    def test_sigterm_still_teardown(self):
        report = self.run_case("import os,signal; os.kill(os.getppid(), signal.SIGTERM)")
        self.assertEqual(signal.SIGTERM, report["signal"]); self.assertFalse(report["success"])


if __name__ == "__main__": unittest.main()
