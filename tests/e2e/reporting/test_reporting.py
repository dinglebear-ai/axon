from __future__ import annotations

import importlib.util
import json
import sys
import tempfile
import unittest
import xml.etree.ElementTree as ET
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, ROOT / "scripts/e2e/lib" / filename)
    module = importlib.util.module_from_spec(spec); sys.modules[name] = module; spec.loader.exec_module(module); return module


reporting = load("e2e_reporting", "reporting.py")


class ReportingTests(unittest.TestCase):
    def setUp(self): self.temp = tempfile.TemporaryDirectory(); self.root = Path(self.temp.name)
    def tearDown(self): self.temp.cleanup()

    def scenario(self, *, cleanup=True):
        source = self.root / "scenario.json"; source.write_text('{"safe":true}\n')
        value = reporting.Scenario("source.page", "hermetic", "source", "cli")
        value.attempt("failed", 9, classification="provider", summary="provider unavailable")
        value.attempt("passed", 3)
        value.invariants.append({"id": "job.terminal", "passed": True})
        value.evidence.append(reporting.evidence_ref(source, self.root))
        value.cleanup = {"success": cleanup, "manifest_digest": "a" * 64, "classes": {}, "phases": [],
                         "residual": [] if cleanup else [{"class": "collection", "identity": "private-host/customer"}]}
        return value

    def test_json_is_deterministic_and_preserves_first_attempt(self):
        first = reporting.suite_report([self.scenario()], tested_sha="a" * 40,
                                       provider_versions={"tei": "1.9", "qdrant": "1.18.2"}, policy={"tier": "hermetic"})
        second = reporting.suite_report([self.scenario()], tested_sha="a" * 40,
                                        provider_versions={"qdrant": "1.18.2", "tei": "1.9"}, policy={"tier": "hermetic"})
        self.assertEqual(first, second); record = first["scenarios"][0]
        self.assertEqual("provider", record["first_attempt_failure"]["classification"])
        self.assertEqual([1, 2], [item["attempt"] for item in record["attempts"]])
        self.assertEqual("passed", first["summary"]["status"])
        reporting.validate_report(first)

    def test_cleanup_failure_is_terminal_and_residual_is_opaque(self):
        report = reporting.suite_report([self.scenario(cleanup=False)], tested_sha="b" * 40,
                                        provider_versions={}, policy={})
        record = report["scenarios"][0]; self.assertEqual("failed", record["status"])
        encoded = json.dumps(record); self.assertNotIn("private-host", encoded)
        self.assertRegex(record["cleanup"]["residuals"][0]["opaque_id"], r"^[0-9a-f]{16}$")

    def test_success_failure_timeout_and_cancellation_are_representable(self):
        scenarios = []
        for status in ("passed", "failed", "timed_out", "canceled"):
            scenario = reporting.Scenario(status, "hermetic", "jobs", "http")
            scenario.attempt(status, 1, classification=None if status == "passed" else "product", summary=status)
            scenario.cleanup = {"success": True}; scenarios.append(scenario)
        report = reporting.suite_report(scenarios, tested_sha="c" * 40, provider_versions={}, policy={})
        self.assertEqual({"passed", "failed", "timed_out", "canceled"},
                         {item["attempts"][0]["status"] for item in report["scenarios"]})

    def test_junit_preserves_attempts_classification_cleanup_and_evidence(self):
        report = reporting.suite_report([self.scenario(cleanup=False)], tested_sha="d" * 40,
                                        provider_versions={}, policy={})
        path = self.root / "junit.xml"; reporting.write_junit(report, path)
        tree = ET.parse(path); failure = tree.find(".//failure")
        self.assertEqual("cleanup", failure.attrib["type"]); self.assertIn('"attempt": 1', failure.text)
        self.assertIn('"sha256"', tree.find(".//system-out").text)
        self.assertIn('"classification": "provider"', tree.find(".//system-out").text)

    def test_upload_failure_does_not_hide_status_or_local_path(self):
        report = reporting.suite_report([self.scenario()], tested_sha="e" * 40, provider_versions={}, policy={},
                                        upload={"status": "failed", "local_evidence_path": "target/e2e/evidence"})
        self.assertEqual("passed", report["summary"]["status"])
        self.assertEqual("target/e2e/evidence", report["upload"]["local_evidence_path"])

    def test_writers_fail_closed_on_dynamic_canary_or_credential_shape(self):
        scenario = self.scenario(); scenario.attempts[0]["summary"] = "provider echoed DynamicCanary123"
        report = reporting.suite_report([scenario], tested_sha="f" * 40, provider_versions={}, policy={})
        with self.assertRaises(reporting.redaction.RedactionError):
            reporting.write_json(report, self.root / "unsafe.json", secrets=("DynamicCanary123",))
        self.assertFalse((self.root / "unsafe.json").exists())

    def test_long_first_failure_keeps_head_tail_and_attributable_digest(self):
        scenario = reporting.Scenario("large", "hermetic", "source", "cli")
        scenario.attempt("failed", 1, classification="product", summary="HEAD" + "x" * 6000 + "TAIL")
        scenario.attempt("passed", 1); scenario.cleanup = {"success": True}
        failure = scenario.record()["first_attempt_failure"]["summary"]
        self.assertIn("HEAD", failure); self.assertIn("TAIL", failure); self.assertRegex(failure, r"sha256=[0-9a-f]{64}")

    def test_digest_and_attempt_history_tampering_fail_validation(self):
        report = reporting.suite_report([self.scenario()], tested_sha="1" * 40, provider_versions={}, policy={})
        report["scenarios"][0]["attempts"][1]["attempt"] = 7
        with self.assertRaises(reporting.ReportingError): reporting.validate_report(report)
        invalid = reporting.suite_report([self.scenario()], tested_sha="2" * 40, provider_versions={}, policy={})
        invalid["tested_sha"] = "g" * 40
        with self.assertRaises(reporting.ReportingError): reporting.validate_report(invalid)


if __name__ == "__main__": unittest.main()
