from __future__ import annotations
import importlib.util,json,os,re,signal,subprocess,tempfile,threading,time,unittest
from pathlib import Path
from unittest import mock

ROOT=Path(__file__).resolve().parents[3]
WORKFLOW=ROOT/".github/workflows/e2e-hermetic.yml"
RUNNER=ROOT/"scripts/e2e/run-hermetic.py"
def load_runner():
    spec=importlib.util.spec_from_file_location("axon_e2e_hermetic_runner",RUNNER)
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module

class HermeticWorkflowTests(unittest.TestCase):
    def test_macos_profile_keeps_network_deny_and_process_metadata_contract(self):
        profile=(ROOT/"scripts/e2e/hermetic.sb").read_text()
        self.assertIn('(deny network-outbound)',profile)
        self.assertIn('(allow process-exec (literal "/bin/ps"))',profile)
        self.assertIn('(allow file-read* (literal "/bin/ps"))',profile)
        self.assertIn('/usr/lib/libproc.dylib',profile)
        isolation=(ROOT/"scripts/e2e/lib/run-isolation.py").read_text()
        self.assertIn('libproc.proc_pidinfo(pid, 3',isolation)
    def test_workflow_is_required_least_privilege_and_sha_pinned(self):
        text=WORKFLOW.read_text();self.assertIn("contents: read",text);self.assertNotIn("id-token:",text)
        self.assertNotIn("secrets.",text);self.assertNotIn("tailscale",text.lower());self.assertNotIn("pull_request_target",text)
        uses=re.findall(r"uses:\s*([^\s]+)",text);self.assertTrue(uses)
        for action in uses:self.assertRegex(action,r"^[^@]+@[0-9a-f]{40}$")
        self.assertIn("E2E Hermetic Required",text);self.assertIn("persist-credentials: false",text)
        self.assertIn("node-version: 24.8.0",text)
        self.assertIn("--version 1.42.4 just",text)
        self.assertIn("npm ci --prefix scripts/e2e/tooling",text)
        lock=json.loads((ROOT/"scripts/e2e/tooling/package-lock.json").read_text())
        self.assertEqual("0.13.0",lock["packages"]["node_modules/mcporter"]["version"])
        self.assertTrue(all("integrity" in package for name,package in lock["packages"].items() if name))
        self.assertNotIn("curl",text);self.assertNotIn("mise-action",text)
        self.assertRegex(text,r"Retain sanitized measured evidence\n\s+#.*\n\s+#.*\n\s+if: always\(\)")

    def test_workflow_does_not_cancel_teardown_and_has_mandatory_hermetic_flags(self):
        text=WORKFLOW.read_text();self.assertIn("cancel-in-progress: false",text);self.assertNotIn("cancel-in-progress: true",text);self.assertIn("timeout-minutes:",text)
        self.assertIn("--history target/e2e/prior-history.json",text)
        self.assertIn("--attestations target/e2e/prior-history-attestations.json",text)
        for key,value in load_runner().REQUIRED_ENV.items():
            self.assertIn(key,text);self.assertIn(str(value),text)
        planned=load_runner().commands()
        self.assertGreater(load_runner().DEFAULT_BUDGET_SECONDS,sum(stage[2] for stage in planned))

    def test_local_launcher_has_no_transient_openssl_path(self):
        just=(ROOT/"Justfile").read_text();launcher=(ROOT/"scripts/e2e/run-hermetic-local.sh").read_text()
        self.assertNotIn("/tmp/axon-openssl",just+launcher)
        self.assertIn("brew --prefix openssl@3",launcher)

    def test_operator_policy_documents_name_promotion_and_bypass(self):
        text=(ROOT/"docs/guides/e2e-hermetic-ci.md").read_text()
        self.assertIn("`E2E Hermetic Required`",text);self.assertIn("axon_rust-nnzde.24",text)
        self.assertIn("bypass",text.lower());self.assertIn("rollback",text.lower())

    def test_runner_discovers_domain_entrypoints_and_cleanup_stages(self):
        names=[name for name,_argv,_budget in load_runner().commands()]
        self.assertIn("scenario-source",names);self.assertIn("scenario-retrieval",names)
        self.assertIn("teardown",names);self.assertIn("isolation",names)
        self.assertLess(names.index("scenario-source"),names.index("teardown"))

    def test_runner_fails_closed_without_flags_or_with_public_route(self):
        runner=load_runner();saved=dict(os.environ)
        try:
            for key in runner.REQUIRED_ENV:os.environ.pop(key,None)
            with self.assertRaisesRegex(RuntimeError,"mandatory hermetic"):runner.validate_environment()
            os.environ.update(runner.REQUIRED_ENV);os.environ["QDRANT_URL"]="https://operator.example"
            with self.assertRaisesRegex(RuntimeError,"public provider route"):runner.validate_environment()
        finally:os.environ.clear();os.environ.update(saved)

    def test_report_verifier_rejects_failed_cleanup(self):
        verifier=(ROOT/"scripts/e2e/verify-hermetic-report.py").read_text()
        self.assertIn('if not report["success"]',verifier);self.assertIn("stage budget exceeded",verifier)

    def test_promoted_runner_report_is_accepted_by_exact_workflow_verifier(self):
        runner=load_runner();saved=dict(os.environ)
        try:
            os.environ.update(runner.REQUIRED_ENV)
            commands=[("work",[os.sys.executable,"-c","pass"],5),("teardown",[os.sys.executable,"-c","pass"],5),("isolation",[os.sys.executable,"-c","pass"],5)]
            with tempfile.TemporaryDirectory() as directory,mock.patch.object(runner,"commands",return_value=commands),mock.patch.object(runner,"verify_native_isolation"):
                report=Path(directory)/"report.json";self.assertEqual(0,runner.run(report,20))
                result=subprocess.run([os.sys.executable,str(ROOT/"scripts/e2e/verify-hermetic-report.py"),"--expected-required","true",str(report)],capture_output=True,text=True)
                self.assertEqual(0,result.returncode,result.stderr)
        finally:os.environ.clear();os.environ.update(saved)

    def test_runner_installs_executable_dns_and_connect_denial(self):
        text=RUNNER.read_text();self.assertIn("socket.socket.connect=_guard",text)
        self.assertIn("socket.getaddrinfo=_guard_gai",text);self.assertIn("hermetic public network denied",text)

    def test_failure_still_emits_machine_readable_cleanup_report(self):
        runner=load_runner();saved=dict(os.environ)
        try:
            os.environ.update(runner.REQUIRED_ENV)
            with tempfile.TemporaryDirectory() as directory:
                report=Path(directory)/"report.json"
                commands=[("injected-failure",[os.sys.executable,"-c","raise SystemExit(7)"],5),
                          ("teardown",[os.sys.executable,"-c","pass"],5),
                          ("isolation",[os.sys.executable,"-c","pass"],5)]
                with mock.patch.object(runner,"commands",return_value=commands), \
                     mock.patch.object(runner,"verify_native_isolation"):
                    self.assertEqual(1,runner.run(report,11))
                body=json.loads(report.read_text());self.assertFalse(body["success"])
                self.assertEqual("failed",body["stages"][0]["status"])
                self.assertEqual("teardown-stages-plus-run-wide-residual-audit",body["cleanup_contract"])
                self.assertTrue(body["residual_audit"]["success"])
                self.assertEqual("passed",body["cleanup"]["teardown"]["status"])
                self.assertEqual("passed",body["cleanup"]["isolation"]["status"])
        finally:os.environ.clear();os.environ.update(saved)

    def test_environment_failure_is_reported_then_propagated_after_cleanup(self):
        runner=load_runner();saved=dict(os.environ)
        try:
            for key in runner.REQUIRED_ENV:os.environ.pop(key,None)
            commands=[("teardown",[os.sys.executable,"-c","pass"],5),
                      ("isolation",[os.sys.executable,"-c","pass"],5)]
            with tempfile.TemporaryDirectory() as directory,mock.patch.object(runner,"commands",return_value=commands):
                report=Path(directory)/"report.json"
                with self.assertRaisesRegex(RuntimeError,"mandatory hermetic"):
                    runner.run(report,10)
                body=json.loads(report.read_text())
                self.assertEqual("environment",body["stages"][0]["name"])
                self.assertTrue(body["stages"][0]["sanitized"])
                self.assertEqual("passed",body["cleanup"]["teardown"]["status"])
                self.assertEqual("passed",body["cleanup"]["isolation"]["status"])
        finally:os.environ.clear();os.environ.update(saved)

    def test_total_budget_is_enforced_without_synthetic_stage(self):
        runner=load_runner();saved=dict(os.environ)
        try:
            os.environ.update(runner.REQUIRED_ENV)
            commands=[("slow",[os.sys.executable,"-c","import time;time.sleep(2)"],5),
                      ("teardown",[os.sys.executable,"-c","pass"],1),
                      ("isolation",[os.sys.executable,"-c","pass"],1)]
            with tempfile.TemporaryDirectory() as directory, \
                 mock.patch.object(runner,"commands",return_value=commands), \
                 mock.patch.object(runner,"verify_native_isolation"):
                report=Path(directory)/"report.json";self.assertEqual(1,runner.run(report,3))
                body=json.loads(report.read_text())
                self.assertTrue(body["budget_exhausted"])
                self.assertNotIn("total-budget",[stage["name"] for stage in body["stages"]])
                self.assertEqual("passed",body["cleanup"]["teardown"]["status"])
                self.assertEqual("passed",body["cleanup"]["isolation"]["status"])
        finally:os.environ.clear();os.environ.update(saved)

    def test_timeout_stops_stage_resource_monitor(self):
        runner=load_runner();saved=dict(os.environ)
        try:
            os.environ.update(runner.REQUIRED_ENV)
            commands=[("slow",[os.sys.executable,"-c","import time;time.sleep(2)"],1),
                      ("teardown",[os.sys.executable,"-c","pass"],1),
                      ("isolation",[os.sys.executable,"-c","pass"],1)]
            with tempfile.TemporaryDirectory() as directory, \
                 mock.patch.object(runner,"commands",return_value=commands), \
                 mock.patch.object(runner,"verify_native_isolation"):
                before={thread.ident for thread in threading.enumerate()}
                report=Path(directory)/"report.json";self.assertEqual(1,runner.run(report,4))
                leaked=[thread for thread in threading.enumerate() if thread.ident not in before and thread.name.startswith("Thread-")]
                self.assertEqual([],leaked)
                self.assertTrue(json.loads(report.read_text())["residual_audit"]["success"])
        finally:os.environ.clear();os.environ.update(saved)

    def test_sigterm_still_runs_cleanup_and_writes_report(self):
        runner=load_runner();saved=dict(os.environ)
        try:
            os.environ.update(runner.REQUIRED_ENV)
            commands=[("work",[os.sys.executable,"-c","import time;time.sleep(10)"],15),
                      ("teardown",[os.sys.executable,"-c","pass"],5),
                      ("isolation",[os.sys.executable,"-c","pass"],5)]
            with tempfile.TemporaryDirectory() as directory, \
                 mock.patch.object(runner,"commands",return_value=commands), \
                 mock.patch.object(runner,"verify_native_isolation"):
                timer=threading.Timer(.1,lambda:os.kill(os.getpid(),signal.SIGTERM));timer.start()
                report=Path(directory)/"report.json";self.assertEqual(1,runner.run(report,30));timer.join()
                body=json.loads(report.read_text());self.assertTrue(body["canceled"])
                self.assertEqual("passed",body["cleanup"]["teardown"]["status"])
                self.assertEqual("passed",body["cleanup"]["isolation"]["status"])
        finally:os.environ.clear();os.environ.update(saved)

    @unittest.skipIf(os.name=="nt","POSIX process-group cancellation contract")
    def test_sigterm_terminates_stage_descendants(self):
        runner=load_runner();saved=dict(os.environ)
        try:
            os.environ.update(runner.REQUIRED_ENV)
            with tempfile.TemporaryDirectory() as directory:
                child_pid=Path(directory)/"child.pid"
                program=("import pathlib,subprocess,sys,time;"
                         "p=subprocess.Popen([sys.executable,'-c','import time;time.sleep(30)'],"
                         "stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL);"
                         f"pathlib.Path({str(child_pid)!r}).write_text(str(p.pid));time.sleep(30)")
                commands=[("work",[os.sys.executable,"-c",program],15),
                          ("teardown",[os.sys.executable,"-c","pass"],5),
                          ("isolation",[os.sys.executable,"-c","pass"],5)]
                with mock.patch.object(runner,"commands",return_value=commands),mock.patch.object(runner,"verify_native_isolation"):
                    def cancel_after_child():
                        deadline=time.monotonic()+2
                        while time.monotonic()<deadline:
                            if child_pid.exists() and child_pid.read_text().strip():break
                            time.sleep(.01)
                        os.kill(os.getpid(),signal.SIGTERM)
                    timer=threading.Thread(target=cancel_after_child,daemon=True);timer.start()
                    report=Path(directory)/"report.json";self.assertEqual(1,runner.run(report,30));timer.join(timeout=3)
                pid=int(child_pid.read_text());time.sleep(.1)
                with self.assertRaises(ProcessLookupError):os.kill(pid,0)
        finally:os.environ.clear();os.environ.update(saved)

if __name__=="__main__":unittest.main()
