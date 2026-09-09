import importlib.util
import json
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
SPEC = importlib.util.spec_from_file_location("mcp_adapter", ROOT / "scripts/e2e/adapters/mcp.py")
adapter = importlib.util.module_from_spec(SPEC)
assert SPEC.loader
SPEC.loader.exec_module(adapter)


class McpAdapterTests(unittest.TestCase):
    def run_adapter(self, command, *envelopes):
        with tempfile.TemporaryDirectory() as directory:
            paths = []
            for index, envelope in enumerate(envelopes):
                path = Path(directory) / f"{index}.json"
                path.write_text(json.dumps(envelope), encoding="utf-8")
                paths.append(str(path))
            return subprocess.run(
                ["python3", str(ROOT / "scripts/e2e/adapters/mcp.py"), command, *paths],
                capture_output=True, text=True, check=False,
            )

    def test_job_id_extraction_uses_source_envelope_not_artifact_id(self):
        job_id = "49a32677-4005-4bd8-83b8-a41bfc093e72"
        result = self.run_adapter("job-id", {
            "ok": True, "action": "source", "job": {"id": job_id},
            "data": {"response_mode": "path", "artifact": {"id": "not-a-job"}},
        })
        self.assertEqual(0, result.returncode, result.stderr)
        self.assertEqual(job_id, result.stdout.strip())

    def test_job_id_extraction_rejects_missing_job_instead_of_artifact(self):
        result = self.run_adapter("job-id", {
            "ok": True, "action": "source", "artifacts": [{"id": "not-a-job"}],
        })
        self.assertNotEqual(0, result.returncode)
        self.assertEqual("", result.stdout)

    def test_prune_verification_binds_saved_checksum_to_original_plan(self):
        plan = {"job_id": "f0662dd7-ee80-4f72-8eff-5480b5b43c04",
                "selector": {"kind": "collection", "collection": "axon_e2e_test"},
                "estimated": {"vector_points": 0}, "steps": []}
        original = {"ok": True, "action": "prune", "subaction": "plan",
                    "data": {"response_mode": "auto-inline", "data": {"plan": plan, "result": None}}}
        saved = {"ok": True, "action": "prune", "subaction": "get",
                 "data": {"response_mode": "auto-inline", "data": {
                     "plan": plan, "inventory_checksum": "a" * 64,
                     "expires_at_utc": "2099-01-01T00:00:00Z"}}}
        result = self.run_adapter("verify-plan", original, saved)
        self.assertEqual(0, result.returncode, result.stderr)
        scenario = next(item for item in adapter.scenarios() if item["id"] == "prune.plan.happy")
        evidence = adapter.normalize(scenario, "http", json.loads(result.stdout))
        self.assertEqual([], adapter.evaluate(scenario, evidence))
        for invalid in (
            saved | {"ok": False},
            saved | {"data": {"data": {"plan": plan, "inventory_checksum": ""}}},
            saved | {"data": {"data": {"plan": plan | {"steps": ["different"]}, "inventory_checksum": "a" * 64}}},
        ):
            with self.subTest(invalid=invalid):
                rejected = self.run_adapter("verify-plan", original, invalid)
                self.assertNotEqual(0, rejected.returncode)
                self.assertEqual("", rejected.stdout)

    def test_plan_id_alone_cannot_satisfy_digest_oracle(self):
        scenario = next(item for item in adapter.scenarios() if item["id"] == "prune.plan.happy")
        evidence = adapter.normalize(scenario, "http", {
            "ok": True, "action": "prune", "data": {"plan_id": "some-id"},
        })
        self.assertIn("semantic oracle failed: prune.plan_digest_bound", adapter.evaluate(scenario, evidence))

    def test_source_projection_drops_cli_wait_and_preserves_inline_completion(self):
        item = next(value for value in adapter.scenarios() if value["id"] == "source.inline.happy")
        args = adapter.tool_arguments(item)
        self.assertNotIn("wait", args)
        self.assertFalse(args["detached"])

    def test_prune_plan_uses_plan_subaction_without_cli_dry_run(self):
        item = next(value for value in adapter.scenarios() if value["id"] == "prune.plan.happy")
        args = adapter.tool_arguments(item)
        self.assertNotIn("dry_run", args)
        self.assertEqual("plan", args["subaction"])

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
            envelope_path.write_text(json.dumps({"job_id":"job-123", "source_id":"source-456",
                                                 "collection":f"{allocation['run_id']}_returned"}), encoding="utf-8")
            scenario = adapter.scenarios()[0]
            result = adapter.register_evidence(Path(allocation["manifest"]), scenario, evidence_path, envelope_path,
                                               f"{allocation['run_id']}_requested")
            records = isolation.Manifest.open(Path(allocation["manifest"])).verify()
            metadata = [record["payload"].get("metadata", {}) for record in records]
            self.assertTrue(any(value.get("path") == str(evidence_path.resolve()) for value in metadata))
            self.assertTrue(any(value.get("external_id") == "job-123" for value in metadata))
            self.assertTrue(any(value.get("external_id") == "source-456" for value in metadata))
            self.assertEqual({"registered":True,"cleanup_state":"registered_only","cleanup_executed":False}, result)


if __name__ == "__main__":
    unittest.main()
