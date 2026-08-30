from __future__ import annotations

import copy
import importlib.util
import json
import sqlite3
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
MODULE = ROOT / "scripts/e2e/lib/observability-assertions.py"
SPEC = importlib.util.spec_from_file_location("axon_e2e_observability", MODULE)
assert SPEC and SPEC.loader
observe = importlib.util.module_from_spec(SPEC); sys.modules[SPEC.name] = observe; SPEC.loader.exec_module(observe)
REPORT_SPEC = importlib.util.spec_from_file_location("axon_e2e_observe_reporting", ROOT / "scripts/e2e/lib/reporting.py")
assert REPORT_SPEC and REPORT_SPEC.loader
reporting = importlib.util.module_from_spec(REPORT_SPEC); sys.modules[REPORT_SPEC.name] = reporting; REPORT_SPEC.loader.exec_module(reporting)
SUPERVISOR_SPEC = importlib.util.spec_from_file_location("axon_e2e_observe_supervisor", ROOT / "scripts/e2e/lib/run-with-teardown.py")
assert SUPERVISOR_SPEC and SUPERVISOR_SPEC.loader
supervisor = importlib.util.module_from_spec(SUPERVISOR_SPEC); sys.modules[SUPERVISOR_SPEC.name] = supervisor; SUPERVISOR_SPEC.loader.exec_module(supervisor)


class ObservabilityContractTests(unittest.TestCase):
    def setUp(self):
        self.capture = json.loads((ROOT / "tests/e2e/fixtures/observability/source-success.json").read_text())
        self.runtime = {
            "events": [
                {"job_id": "job-observe-1", "sequence": 1, "phase": "resolving", "status": "running", "attempt": 0, "counts": {}, "timestamp": "2026-08-30T10:00:00.000Z"},
                {"job_id": "job-observe-1", "sequence": 2, "phase": "routing", "status": "running", "attempt": 0, "counts": {}, "timestamp": "2026-08-30T10:00:00.020Z"},
                {"job_id": "job-observe-1", "sequence": 3, "phase": "authorizing", "status": "running", "attempt": 0, "counts": {}, "timestamp": "2026-08-30T10:00:00.040Z"},
                {"job_id": "job-observe-1", "sequence": 4, "phase": "leasing", "status": "running", "attempt": 0, "counts": {}, "timestamp": "2026-08-30T10:00:00.060Z"},
                {"job_id": "job-observe-1", "sequence": 5, "phase": "discovering", "status": "running", "attempt": 0, "counts": {"items_total": 2, "items_done": 0}, "timestamp": "2026-08-30T10:00:00.080Z"},
                {"job_id": "job-observe-1", "sequence": 6, "phase": "discovering", "status": "completed", "attempt": 0, "counts": {"items_total": 2, "items_done": 2}, "timestamp": "2026-08-30T10:00:00.120Z"},
                {"job_id": "job-observe-1", "sequence": 7, "phase": "fetching", "status": "running", "attempt": 0, "counts": {"items_total": 2, "items_done": 2, "bytes_total": 20, "bytes_done": 0}, "current": {"source_item_key": "item-1"}, "timestamp": "2026-08-30T10:00:00.150Z"},
                {"job_id": "job-observe-1", "sequence": 8, "phase": "fetching", "status": "completed", "attempt": 0, "counts": {"items_total": 2, "items_done": 2, "bytes_total": 20, "bytes_done": 20}, "timestamp": "2026-08-30T10:00:00.250Z"},
                {"job_id": "job-observe-1", "sequence": 9, "phase": "cleaning", "status": "running", "attempt": 0, "counts": {}, "timestamp": "2026-08-30T10:00:00.300Z"},
                {"job_id": "job-observe-1", "sequence": 10, "phase": "complete", "status": "completed", "attempt": 0, "counts": {}, "timestamp": "2026-08-30T10:00:00.350Z"}
            ],
            "heartbeat": {"job_id": "job-observe-1", "attempt": 0, "last_event_sequence": 10, "status": "completed", "phase": "complete"},
            "provider_health": []
        }

    def assert_rejected(self, mutation, phrase):
        capture, runtime = copy.deepcopy(self.capture), copy.deepcopy(self.runtime)
        mutation(capture, runtime)
        with self.assertRaisesRegex(observe.ObservabilityFailure, phrase): observe.evaluate(capture, runtime)

    def test_success_contract_emits_small_stable_oracle_set_for_canonical_report(self):
        outcomes = observe.evaluate(self.capture, self.runtime)
        self.assertEqual(list(observe.ORACLE_IDS), [item["id"] for item in outcomes])
        self.assertTrue(all(item == {**item, "passed": True} for item in outcomes))
        json.dumps(outcomes)

    def test_hermetic_and_live_tiers_serialize_through_canonical_reporting(self):
        for tier in ("hermetic", "live"):
            scenario = reporting.Scenario("source.observe", tier, "source", "multi-observer")
            scenario.invariants.extend(observe.evaluate(self.capture, self.runtime))
            scenario.attempt("passed", 400)
            scenario.cleanup = {"success": True, "residual": [], "refused": [], "manifest_digest": "a" * 64}
            report = reporting.suite_report([scenario], tested_sha="b" * 40,
                                            provider_versions={"axon": "7.2.2"}, policy={"tier": tier})
            reporting.validate_report(report)
            self.assertEqual(list(observe.ORACLE_IDS), [item["id"] for item in report["scenarios"][0]["invariants"]])

    def test_missing_duplicate_out_of_order_and_contradictory_events_fail(self):
        self.assert_rejected(lambda _, runtime: runtime["events"].pop(2), "cardinality gap")
        self.assert_rejected(lambda _, runtime: runtime["events"].insert(2, copy.deepcopy(runtime["events"][1])), "duplicate")
        self.assert_rejected(lambda _, runtime: runtime["events"].__setitem__(slice(1, 3), reversed(runtime["events"][1:3])), "causal order")
        self.assert_rejected(lambda capture, _: capture["executions"][1].update(terminal_status="failed"), "contradict")
        self.assert_rejected(lambda capture, _: capture["executions"][1].update(progress_sequence=10), "before terminal")

    def test_progress_must_be_monotonic_bounded_and_preterminal(self):
        self.assert_rejected(lambda _, runtime: runtime["events"][6]["counts"].update(bytes_done=-1), "unbounded")
        self.assert_rejected(lambda _, runtime: (runtime["events"][6]["counts"].update(bytes_done=10),
                                                  runtime["events"][7]["counts"].update(bytes_done=5)), "regressed")
        self.assert_rejected(lambda _, runtime: runtime["heartbeat"].update(last_event_sequence=99), "heartbeat contradicts")

    def test_logs_metrics_and_retained_evidence_keep_bounded_correlation(self):
        self.assert_rejected(lambda capture, _: capture["logs"][0].pop("job_id"), "log lost")
        self.assert_rejected(lambda capture, _: capture["metrics"][0]["labels"].update(job_id="job-observe-1"), "high-cardinality")
        self.assert_rejected(lambda capture, _: capture["metrics"][0]["labels"].update(phase="unknown"), "not attributable")
        self.assert_rejected(lambda capture, _: capture["evidence"][0].update(job_id="foreign"), "evidence lost")

    def test_provider_auth_and_product_failures_agree_across_channels(self):
        for classification in ("provider", "auth/network", "product"):
            capture, runtime = copy.deepcopy(self.capture), copy.deepcopy(self.runtime)
            capture["expected_failure"] = {"classification": classification}
            for execution in capture["executions"]:
                execution["failure_classification"] = classification; execution["terminal_status"] = "failed"
            code = {"provider": "provider.timeout", "auth/network": "auth.denied", "product": "source.invalid"}[classification]
            runtime["events"][-1].update(status="failed", error={"code": code, "message": "sanitized"})
            if classification == "provider":
                capture["owned_provider_ids"] = ["tei"]
                runtime["provider_health"] = [{"provider_id": "tei", "provider_kind": "embedding", "status": "degraded", "cooldown_until": None, "last_error_code": "provider.timeout"}]
            self.assertEqual(classification, observe.evaluate(capture, runtime)[3]["detail"]["failure_classification"])
        self.assert_rejected(lambda capture, runtime: (capture.update(expected_failure={"classification": "provider"}),
            [item.update(failure_classification="provider", terminal_status="failed") for item in capture["executions"]],
            runtime["events"][-1].update(status="failed", error={"code": "source.invalid"})), "runtime failure")

    def test_retry_requires_a_causal_failed_attempt(self):
        self.runtime["events"][7]["retry"] = {"attempt": 1, "max_attempts": 3, "reason": "timeout", "next_retry_at": None}
        self.assert_rejected(lambda _capture, _runtime: None, "retry has no preceding")
        self.runtime["events"][6]["error"] = {"code": "provider.timeout", "message": "sanitized"}
        self.assertEqual(1, observe.evaluate(self.capture, self.runtime)[2]["detail"]["retry_count"])

    def test_redaction_scans_every_channel_and_transformed_canaries(self):
        for field, target in (("message", self.capture["logs"][0]), ("path", self.capture["evidence"][0])):
            capture, runtime = copy.deepcopy(self.capture), copy.deepcopy(self.runtime)
            target = capture["logs"][0] if field == "message" else capture["evidence"][0]
            target[field] = "ObserveSecret-DO-NOT-LEAK"
            with self.assertRaisesRegex(observe.ObservabilityFailure, "protected data leaked"):
                observe.evaluate(capture, runtime)
        self.assert_rejected(lambda capture, _: capture["logs"][0].update(message="%2FUsers%2Fprivate%2Fcustomer%2Facme-contract.pdf"), "protected data leaked")

    def test_timing_uses_monotonic_elapsed_and_declared_tolerance(self):
        self.assert_rejected(lambda capture, _: capture["timing"].update(reported_duration_ms=900), "tolerance")
        self.runtime["events"][-1]["timestamp"] = "2026-08-30T10:00:20.000Z"
        self.assert_rejected(lambda _capture, _runtime: None, "cannot reconcile")

    def test_parity_runs_are_distinguished_from_multi_observer_identity(self):
        capture = copy.deepcopy(self.capture); capture["observation_mode"] = "parity"
        for number, execution in enumerate(capture["executions"]):
            execution["job_id"] = f"parity-{number}"; execution["equivalence_group"] = "source-success"
        capture["logs"][0]["job_id"] = "parity-0"
        capture["evidence"][0]["job_id"] = "parity-0"
        self.assertEqual("parity", observe.evaluate(capture, self.runtime)[0]["detail"]["mode"])

    def test_authoritative_sqlite_rows_feed_the_same_oracles(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "jobs.db"; connection = sqlite3.connect(path)
            migration = ROOT / "crates/axon-observe/src/migrations/0001_create_observability_tables.sql"
            connection.executescript(migration.read_text())
            for event in self.runtime["events"]:
                connection.execute("INSERT INTO axon_observe_events VALUES(?,?,?,?,?,?,?,?,?,?,?)",
                    (f'e-{event["sequence"]}', event["job_id"], event["sequence"], event["phase"], event["status"], "info", "public", "sanitized", event["timestamp"], json.dumps(event), 10000 + event["sequence"] * 20))
            connection.execute("INSERT INTO axon_observe_heartbeats VALUES(?,?,?,?,?,?,?,?,?)",
                ("job-observe-1", 0, None, "complete", "completed", "2026-08-30T10:00:00.350Z", 10, json.dumps(self.runtime["heartbeat"]), 10350))
            connection.commit(); connection.close()
            runtime = observe.load_runtime(path, "job-observe-1")
            self.assertEqual(list(observe.ORACLE_IDS), [item["id"] for item in observe.evaluate(self.capture, runtime)])

    def test_canonical_supervisor_evaluates_before_authoritative_teardown(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory); allocation = supervisor.teardown.isolation.allocate(base / "runs", base / "manifests")
            data_dir = Path(allocation["data_dir"]); capture_path = data_dir / "observe-capture.json"
            db_path = data_dir / "jobs.db"; capture_path.write_text(json.dumps(self.capture))
            connection = sqlite3.connect(db_path)
            connection.executescript((ROOT / "crates/axon-observe/src/migrations/0001_create_observability_tables.sql").read_text())
            for event in self.runtime["events"]:
                connection.execute("INSERT INTO axon_observe_events VALUES(?,?,?,?,?,?,?,?,?,?,?)",
                    (f'e-{event["sequence"]}', event["job_id"], event["sequence"], event["phase"], event["status"], "info", "public", "sanitized", event["timestamp"], json.dumps(event), 10000 + event["sequence"] * 20))
            connection.execute("INSERT INTO axon_observe_heartbeats VALUES(?,?,?,?,?,?,?,?,?)",
                ("job-observe-1", 0, None, "complete", "completed", "2026-08-30T10:00:00.350Z", 10, json.dumps(self.runtime["heartbeat"]), 10350))
            connection.commit(); connection.close()
            header, resources = supervisor.teardown.manifest_api.load(Path(allocation["manifest"]))
            fake_spec = importlib.util.spec_from_file_location("observe_fake_provider", ROOT / "tests/e2e/fixtures/teardown/fake_provider.py")
            assert fake_spec and fake_spec.loader
            fake_module = importlib.util.module_from_spec(fake_spec); sys.modules[fake_spec.name] = fake_module; fake_spec.loader.exec_module(fake_module)
            fake = fake_module.FakeProvider(supervisor.teardown.manifest_api, header, resources)
            original = supervisor.teardown.Engine
            class Engine(original):
                def __init__(self, manifest, _adapters=None):
                    super().__init__(manifest, {kind: fake for kind in supervisor.teardown.PROVIDER_TYPES})
            supervisor.teardown.Engine = Engine
            try:
                result = supervisor.supervise(Path(allocation["manifest"]), [sys.executable, "-c", "pass"], timeout=2,
                                              observability_capture=capture_path, observability_db=db_path)
            finally: supervisor.teardown.Engine = original
            self.assertTrue(result["success"], result)
            self.assertEqual(list(observe.ORACLE_IDS), [item["id"] for item in result["observability"]])
            self.assertFalse(data_dir.exists(), "authoritative teardown must remove raw capture and SQLite evidence")
            for tier in ("hermetic", "live"):
                scenario = supervisor.canonical_scenario(result, scenario_id=f"source.observe.{tier}", tier=tier,
                                                         capability="source", surface="multi-observer")
                self.assertEqual(list(observe.ORACLE_IDS), [item["id"] for item in scenario.invariants])


if __name__ == "__main__": unittest.main()
