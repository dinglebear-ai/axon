from __future__ import annotations

import importlib.util
import contextlib
import hashlib
import io
import os
import sys
import signal
import subprocess
import tempfile
import time
import unittest
from unittest import mock
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

    def test_spawn_failure_still_returns_authoritative_cleanup(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp); allocation = isolation.allocate(root / "runs", root / "manifests")
            with mock.patch.object(supervisor.subprocess, "Popen", side_effect=OSError("injected spawn failure")):
                report = supervisor.supervise(Path(allocation["manifest"]), [sys.executable, "-c", "pass"], timeout=1)
            self.assertFalse(report["success"]); self.assertIn("injected spawn failure", report["fatal"])
            self.assertIn("run_id", report["cleanup"])
            self.assertTrue(report["cleanup"]["refused"], "missing adapters must fail closed")

    def test_supervised_result_wires_authoritative_cleanup_into_canonical_report(self):
        result = self.run_case("raise SystemExit(7)")
        scenario = supervisor.canonical_scenario(result, scenario_id="source.fail", tier="hermetic",
                                                 capability="source", surface="cli")
        report = supervisor.reporting.suite_report([scenario], tested_sha="a" * 40,
                                                   provider_versions={"qdrant": "1.18.2"}, policy={})
        supervisor.reporting.validate_report(report)
        self.assertEqual("product", report["scenarios"][0]["first_attempt_failure"]["classification"])
        self.assertTrue(report["scenarios"][0]["cleanup"]["success"])

    def test_supervisor_enforces_pre_log_command_policy(self):
        with self.assertRaises(supervisor.reporting.redaction.RedactionError):
            supervisor.supervise(Path("unused"), ["bash", "-c", "set -x; env"], timeout=1)

    def test_suite_aggregates_setup_exception_and_later_result(self):
        old = supervisor.supervise; calls = []
        def fake(manifest, command, **kwargs):
            calls.append(command[0])
            if command[0] == "first": raise RuntimeError("fixture setup")
            return {"success": True, "duration_ms": 1, "cleanup": {"success": True}}
        supervisor.supervise = fake
        try:
            entries = [{"scenario_id": name, "tier": "hermetic", "capability": "source", "surface": "cli",
                        "manifest": "unused", "command": [name], "setup_failure_class": "fixture"}
                       for name in ("first", "second")]
            report = supervisor.supervise_suite(entries, tested_sha="b" * 40, provider_versions={}, policy={})
        finally: supervisor.supervise = old
        self.assertEqual(["first", "second"], calls); self.assertEqual(2, len(report["scenarios"]))
        self.assertEqual("fixture", report["scenarios"][0]["first_attempt_failure"]["classification"])

    def test_live_suite_uses_governed_real_supervisor_retry_path(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);seed="live-run";scenario_id="source.page"
            token=hashlib.sha256(f"{seed}:{scenario_id}".encode()).hexdigest()[:20]
            paths=[]
            for attempt in (1,2):
                run_id=f"axon_e2e_{token}_attempt_{attempt}";data=root/run_id;data.mkdir()
                paths.append(isolation.Manifest.create(root/"manifests",run_id,data).path)
            old=supervisor.supervise;calls=[]
            def fake(manifest,command,**kwargs):
                calls.append((manifest,command));return {"success":len(calls)==2,"child_returncode":7 if len(calls)==1 else 0,"duration_ms":1,"cleanup":{"success":True}}
            supervisor.supervise=fake
            try:
                entry={"scenario_id":scenario_id,"tier":"live","capability":"source","surface":"cli","lifecycle":"source","retry_class":"provider_transient","failure_class":"provider","manifest":str(paths[0]),"command":["first"],"diagnostic_retry":{"manifest":str(paths[1]),"command":["second"]}}
                with mock.patch("time.sleep"):
                    report=supervisor.supervise_suite([entry],tested_sha="c"*40,provider_versions={},policy={"suite_retry_budget":1,"retry_seed":seed})
            finally:supervisor.supervise=old
            self.assertEqual(["first","second"],[item[1][0] for item in calls]);self.assertEqual("passed",report["summary"]["status"])
            retry=report["scenarios"][0]["invariants"][0]["retry_policy"];self.assertEqual((1,0),(retry["budget_before"],retry["suite_budget_remaining"]))

    def test_supervisor_redacts_dynamic_environment_secret_before_log_sink(self):
        secret = "supervisor-Dynamic-Canary-123"; old = os.environ.get("AXON_HTTP_TOKEN"); os.environ["AXON_HTTP_TOKEN"] = secret
        try:
            output = io.StringIO()
            with contextlib.redirect_stdout(output): self.run_case("import os; print(os.environ['AXON_HTTP_TOKEN'])")
        finally:
            if old is None: os.environ.pop("AXON_HTTP_TOKEN", None)
            else: os.environ["AXON_HTTP_TOKEN"] = old
        self.assertNotIn(secret, output.getvalue()); self.assertIn("[REDACTED]", output.getvalue())

    def test_retained_cli_evidence_migrates_as_checksummed_reference(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "cli-evidence.sanitized"; path.write_text("safe")
            digest = hashlib.sha256(path.read_bytes()).hexdigest()
            result = {"success": True, "duration_ms": 1, "cleanup": {"success": True, "retained": [
                {"path": str(path), "sha256": digest, "class": "evidence"}]}}
            scenario = supervisor.canonical_scenario(result, scenario_id="cli.old", tier="hermetic", capability="cli", surface="cli")
            self.assertEqual({"path": path.name, "sha256": digest, "bytes": 4, "kind": "evidence"}, scenario.evidence[0])
            result["cleanup"]["retained"][0]["sha256"] = "c" * 64
            with self.assertRaises(supervisor.reporting.ReportingError):
                supervisor.canonical_scenario(result, scenario_id="cli.bad", tier="hermetic", capability="cli", surface="cli")

    def test_invalid_utf8_child_log_fails_closed_without_releasing_output(self):
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            result = self.run_case("import os; os.write(1, b'\\xff\\xfe')")
        self.assertFalse(result["success"]); self.assertEqual("non-UTF-8 child log is forbidden", result["log_redaction_error"])
        self.assertNotIn("�", output.getvalue())

    @unittest.skipIf(sys.platform == "win32", "POSIX signal contract")
    def test_sigint_workflow_cancellation_still_teardown(self):
        report = self.run_case("import os,signal; os.kill(os.getppid(), signal.SIGINT)")
        self.assertEqual(signal.SIGINT, report["signal"]); self.assertFalse(report["success"])

    @unittest.skipIf(sys.platform == "win32", "POSIX signal contract")
    def test_signal_process_group_exit_race_is_recorded_and_reaped(self):
        real_killpg = supervisor.os.killpg
        raced = False
        def race_once(pid, signum):
            nonlocal raced
            if signum == signal.SIGTERM and not raced:
                raced = True
                raise PermissionError(1, "injected process-group exit race")
            return real_killpg(pid, signum)
        with mock.patch.object(supervisor.os, "killpg", side_effect=race_once):
            report = self.run_case("import os,signal; os.kill(os.getppid(), signal.SIGINT)")
        self.assertTrue(raced)
        self.assertEqual(signal.SIGINT, report["signal"])
        self.assertNotIn("fatal", report)

    @unittest.skipIf(sys.platform == "win32", "POSIX process group contract")
    def test_cancellation_reaps_spawned_descendant_tree(self):
        with tempfile.TemporaryDirectory() as directory:
            pid_file = Path(directory) / "descendant.pid"
            script = ("import os,pathlib,signal,subprocess,sys,time; "
                      "p=subprocess.Popen([sys.executable,'-c','import time; time.sleep(60)']); "
                      f"pathlib.Path({str(pid_file)!r}).write_text(str(p.pid)); "
                      "os.kill(os.getppid(),signal.SIGINT); time.sleep(60)")
            report = self.run_case(script)
            self.assertEqual(signal.SIGINT, report["signal"])
            descendant = int(pid_file.read_text())
            for _ in range(40):
                state = subprocess.run(["ps", "-o", "stat=", "-p", str(descendant)], capture_output=True,
                                       text=True, check=False).stdout.strip()
                if not state or state.startswith("Z"):
                    break
                time.sleep(.025)
            else:
                self.fail("canceled supervisor left a running descendant")

    @unittest.skipIf(sys.platform == "win32", "POSIX signal contract")
    def test_sigterm_still_teardown(self):
        report = self.run_case("import os,signal; os.kill(os.getppid(), signal.SIGTERM)")
        self.assertEqual(signal.SIGTERM, report["signal"]); self.assertFalse(report["success"])

    @unittest.skipIf(sys.platform == "win32", "POSIX signal contract")
    def test_repeated_sigterm_during_cleanup_cannot_interrupt_teardown(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp); allocation = isolation.allocate(root / "runs", root / "manifests")
            header, resources = supervisor.teardown.manifest_api.load(Path(allocation["manifest"]))
            fake_module = load(f"fake_provider_signal_{id(allocation)}", ROOT / "tests/e2e/fixtures/teardown/fake_provider.py")
            fake = fake_module.FakeProvider(supervisor.teardown.manifest_api, header, resources)
            old_engine = supervisor.teardown.Engine
            calls = []
            class Engine(old_engine):
                def __init__(self, manifest, _adapters=None):
                    super().__init__(manifest, {kind: fake for kind in supervisor.teardown.PROVIDER_TYPES})
                def run(self):
                    calls.append("entered")
                    os.kill(os.getpid(), signal.SIGTERM)
                    os.kill(os.getpid(), signal.SIGTERM)
                    return super().run()
            supervisor.teardown.Engine = Engine
            try:
                report = supervisor.supervise(Path(allocation["manifest"]), [sys.executable, "-c", "pass"], timeout=2)
            finally:
                supervisor.teardown.Engine = old_engine
            self.assertEqual(["entered"], calls)
            self.assertTrue(report["cleanup"]["success"], report)
            self.assertFalse(Path(allocation["data_dir"]).exists())
            self.assertEqual(signal.SIGTERM, report["signal"])


if __name__ == "__main__": unittest.main()
