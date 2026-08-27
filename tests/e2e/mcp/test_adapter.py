import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
SPEC = importlib.util.spec_from_file_location("mcp_adapter", ROOT / "scripts/e2e/adapters/mcp.py")
adapter = importlib.util.module_from_spec(SPEC)
assert SPEC.loader
SPEC.loader.exec_module(adapter)


class McpAdapterTests(unittest.TestCase):
    def test_projects_all_catalog_mcp_scenarios_as_structured_argv(self):
        selected = adapter.scenarios()
        self.assertEqual(6, len(selected))
        for item in selected:
            arguments = adapter.tool_arguments(item)
            argv = adapter.mcporter_argv("axon.axon", arguments)
            self.assertEqual("--args", argv[2])
            self.assertEqual(arguments, json.loads(argv[3]))
            self.assertNotIn("bash", argv)
            self.assertNotIn("-c", argv)

    def test_hostile_values_remain_one_json_argument(self):
        hostile = {"action": "source", "source": "$(touch /tmp/nope); '`\n--flag"}
        argv = adapter.mcporter_argv("axon.axon", hostile)
        self.assertEqual(6, len(argv))
        self.assertEqual(hostile, json.loads(argv[3]))

    def test_normalized_evidence_redacts_secrets_and_rejects_provider_error(self):
        item = adapter.scenarios()[0] | {"provider": "tei"}
        result = adapter.normalize(item, "http", {
            "ok": True, "error": "TEI provider unavailable", "authorization": "Bearer secret",
        })
        self.assertFalse(result["success"])
        self.assertNotIn("Bearer secret", json.dumps(result))
        self.assertEqual("Bearer [REDACTED]", adapter.redact("Bearer secret-value"))

    def test_missing_mcp_fixture_is_rejected(self):
        catalog = adapter.load_catalog()
        catalog["scenarios"][0]["requests"].pop("mcp")
        path = self.id().replace(".", "_") + ".json"
        target = ROOT / "tests/e2e/mcp" / path
        try:
            target.write_text(json.dumps(catalog), encoding="utf-8")
            with self.assertRaisesRegex(adapter.McpAdapterError, "fixture is missing"):
                adapter.scenarios(target)
        finally:
            target.unlink(missing_ok=True)

    def test_jobs_projection_contains_only_runtime_mcp_arguments(self):
        item = next(value for value in adapter.scenarios() if value["id"] == "jobs.stream.happy")
        arguments = adapter.tool_arguments(item)
        self.assertEqual("${E2E_JOB_ID}", arguments["job_id"])
        self.assertNotIn("catalog_fixture", arguments)

    def test_oracle_evaluation_rejects_false_success(self):
        item = adapter.scenarios()[0]
        evidence = adapter.normalize(item, "stdio", {"ok": False, "error": "failed"})
        self.assertIn("expected successful MCP content or task envelope", adapter.evaluate(item, evidence))

    def test_success_envelope_without_semantic_facts_is_rejected(self):
        item = next(value for value in adapter.scenarios() if value["id"] == "source.inline.happy")
        evidence = adapter.normalize(item, "stdio", {"ok": True, "action": "source", "data": {}})
        failures = adapter.evaluate(item, evidence)
        self.assertIn("semantic oracle failed: source.accepted", failures)
        self.assertIn("semantic oracle failed: job.terminal_success", failures)

    def test_unknown_oracles_and_cleanup_contract_fail_closed(self):
        item = adapter.scenarios()[0] | {"semantic_oracles": ["unknown.oracle"], "cleanup_contract": "cleanup.unknown"}
        evidence = adapter.normalize(item, "stdio", {"ok": True, "action": "source"})
        failures = adapter.evaluate(item, evidence)
        self.assertIn("unknown semantic oracle: unknown.oracle", failures)
        self.assertIn("unknown or missing cleanup contract", failures)

    def test_generic_error_cannot_satisfy_specific_rejection_oracle(self):
        item = next(value for value in adapter.scenarios() if value["id"] == "jobs.cancel.negative")
        evidence = adapter.normalize(item, "stdio", {"error":"something failed"})
        self.assertIn("semantic oracle failed: rejection.job_missing", adapter.evaluate(item, evidence))

    def test_negative_projections_are_genuinely_invalid_or_missing(self):
        selected = {item["id"]: adapter.tool_arguments(item) for item in adapter.scenarios()}
        self.assertEqual("", selected["source.detached.negative"]["source"])
        self.assertEqual("00000000-0000-0000-0000-000000000000", selected["jobs.cancel.negative"]["job_id"])
        self.assertIn("E2E_FOREIGN_COLLECTION", selected["prune.execute.negative"]["target"])

    def test_registration_records_actual_evidence_and_returned_ids_without_cleanup(self):
        isolation_spec = importlib.util.spec_from_file_location("run_isolation_test", ROOT / "scripts/e2e/lib/run-isolation.py")
        isolation = importlib.util.module_from_spec(isolation_spec)
        assert isolation_spec.loader
        isolation_spec.loader.exec_module(isolation)
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            allocation = isolation.allocate(base / "runs", base / "manifests")
            evidence_path = base / "evidence.json"
            envelope_path = base / "envelope.json"
            evidence_path.write_text("{}", encoding="utf-8")
            envelope_path.write_text(json.dumps({"job_id":"job-123", "source_id":"source-456", "collection":"axon_e2e_returned"}), encoding="utf-8")
            scenario = adapter.scenarios()[0]
            result = adapter.register_evidence(Path(allocation["manifest"]), scenario, evidence_path, envelope_path, "axon_e2e_requested")
            records = isolation.Manifest.open(Path(allocation["manifest"])).verify()
            metadata = [record["payload"].get("metadata", {}) for record in records]
            self.assertTrue(any(value.get("path") == str(evidence_path.resolve()) for value in metadata))
            self.assertTrue(any(value.get("external_id") == "job-123" for value in metadata))
            self.assertTrue(any(value.get("external_id") == "source-456" for value in metadata))
            self.assertEqual({"registered":True,"cleanup_state":"registered_only","cleanup_executed":False}, result)


if __name__ == "__main__":
    unittest.main()
