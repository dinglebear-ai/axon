from __future__ import annotations

import hashlib
import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from copy import deepcopy
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
SCRIPT = ROOT / "scripts/e2e/reconcile-surfaces.py"
CATALOG = json.loads((ROOT / "tests/e2e/catalog/catalog.json").read_text())


def module():
    spec = importlib.util.spec_from_file_location("reconcile_surfaces", SCRIPT)
    assert spec and spec.loader
    loaded = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(loaded)
    return loaded


def evidence(tmp_path: Path, name: str, value: dict) -> tuple[str, str]:
    path = tmp_path / name
    path.write_text(json.dumps(value, sort_keys=True), encoding="utf-8")
    return path.name, hashlib.sha256(path.read_bytes()).hexdigest()


def semantic(*, state="completed", error=None, citations=None, identity="fixture:atlas",
             lineage="atlas:v1", value=None, effects=None):
    return {
        "semantic_value": value if value is not None else {"accepted": True},
        "terminal_state": state,
        "error_code": error,
        "citations": citations if citations is not None else ["fixture://atlas#amber"],
        "resource_identity": identity,
        "lineage": lineage,
        "effects": effects if effects is not None else {"documents": 1},
    }


def passing_bundle(tmp_path: Path) -> tuple[dict, Path]:
    saved_path, digest = evidence(tmp_path, "saved.json", {"token": "[REDACTED]", "result": "pass"})
    executions = []
    for surface, envelope in [
        ("cli", {"exit_code": 0, "assertions": [{"id": "cli.json_object", "passed": True}]}),
        ("mcp", {"jsonrpc": "2.0", "content_count": 1, "assertions": [{"id": "mcp.content", "passed": True}]}),
        ("mcp_task_wire", {"task_id": "task-17", "assertions": [{"id": "mcp.task_wire", "passed": True}]}),
        ("http", {"status": 202, "assertions": [{"id": "http.success_status", "passed": True}]}),
    ]:
        executions.append({
            "parent_scenario_id": "source.inline.happy", "execution_id": f"source-{surface}",
            "capability": "source", "surface": surface, "comparison_mode": "independent",
            "evidence_path": saved_path, "evidence_sha256": digest,
            "semantics": semantic(), "envelope": envelope,
        })
    executions.extend([
        {"parent_scenario_id": "jobs.stream.happy", "execution_id": "jobs-cli", "capability": "jobs",
         "surface": "cli", "comparison_mode": "multi_observer", "observed_operation_id": "job-42",
         "evidence_path": saved_path, "evidence_sha256": digest, "semantics": semantic(identity="job-42"),
         "envelope": {"exit_code": 0, "assertions": [{"id": "cli.json_object", "passed": True}]}},
        {"parent_scenario_id": "jobs.stream.happy", "execution_id": "jobs-http", "capability": "jobs",
         "surface": "http", "comparison_mode": "multi_observer", "observed_operation_id": "job-42",
         "evidence_path": saved_path, "evidence_sha256": digest, "semantics": semantic(identity="job-42"),
         "envelope": {"status": 200, "assertions": [{"id": "http.success_status", "passed": True}]}},
    ])
    for scenario_id, execution_id, capability in [
        ("source.detached.negative", "source-negative-cli", "source"),
        ("jobs.cancel.negative", "jobs-negative-cli", "jobs"),
        ("prune.plan.happy", "prune-happy-cli", "prune"),
        ("prune.execute.negative", "prune-negative-cli", "prune"),
    ]:
        executions.append({
            "parent_scenario_id": scenario_id, "execution_id": execution_id,
            "capability": capability, "surface": "cli", "comparison_mode": "independent",
            "evidence_path": saved_path, "evidence_sha256": digest, "semantics": semantic(),
            "envelope": {"exit_code": 0, "assertions": [{"id": "cli.json_object", "passed": True}]},
        })
    behavioral = [row for row in CATALOG["operations"] if row["classification"] == "behavioral_e2e"]
    scenario_for = {
        (scenario["lifecycle"], scenario["polarity"]): scenario
        for scenario in CATALOG["scenarios"]
    }
    lifecycle_pairs = list(scenario_for)
    execution_for = {
        (item["parent_scenario_id"], item["surface"]): item["execution_id"]
        for item in executions
    }
    coverage = []
    for index, row in enumerate(behavioral):
        lifecycle, polarity = lifecycle_pairs[index] if index < len(lifecycle_pairs) else ("inventory", "happy")
        scenario = scenario_for[(lifecycle, polarity)] if index < len(lifecycle_pairs) else CATALOG["scenarios"][0]
        lifecycle, polarity = scenario["lifecycle"], scenario["polarity"]
        scenario_id = scenario["id"]
        execution_id = execution_for[(scenario_id, "cli")]
        coverage_path, _ = evidence(tmp_path, f"coverage-{index}.json", {
            "operation_id": row["id"], "scenario_id": scenario_id, "kind": "behavioral", "result": "pass",
            "surface": "cli", "lifecycle": lifecycle, "polarity": polarity,
            "execution_id": execution_id,
        })
        coverage.append({
            "operation_id": row["id"], "scenario_id": scenario_id, "surface": "cli",
            "kind": "behavioral", "result": "pass", "evidence_path": coverage_path,
            "lifecycle": lifecycle, "polarity": polarity, "execution_id": execution_id,
        })
    bundle = {"schema_version": 1, "executions": executions, "coverage": coverage}
    bundle_path = tmp_path / "bundle.json"
    bundle_path.write_text(json.dumps(bundle), encoding="utf-8")
    return bundle, bundle_path


class ReconcileSurfaceTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.tmp_path = Path(self.temporary.name)

    def tearDown(self):
        self.temporary.cleanup()

    def test_equivalent_semantics_pass_despite_transport_envelopes(self):
        bundle, path = passing_bundle(self.tmp_path)
        result = module().reconcile(CATALOG, bundle, path)
        self.assertTrue(result["passed"])
        self.assertEqual(result["parity_comparisons"], 4)
        self.assertGreaterEqual(result["coverage"]["percent"], 91)

    def test_each_semantic_divergence_is_actionable(self):
        for field, changed in [
            ("semantic_value", {"accepted": False}), ("terminal_state", "failed"),
            ("error_code", "auth.forbidden"), ("citations", ["fixture://wrong"]),
            ("resource_identity", "fixture:wrong"), ("lineage", "atlas:v2"),
        ]:
            with self.subTest(field=field):
                bundle, path = passing_bundle(self.tmp_path)
                bundle["executions"][1]["semantics"][field] = changed
                result = module().reconcile(CATALOG, bundle, path)
                failures = [item for item in result["failures"] if item["invariant"] == field]
                self.assertTrue(failures)
                self.assertEqual(failures[0]["scenario"], "source.inline.happy")
                self.assertEqual(failures[0]["surface"], "mcp")
                self.assertEqual(failures[0]["evidence_path"], "saved.json")

    def test_forbidden_and_not_found_are_not_normalized_together(self):
        bundle, path = passing_bundle(self.tmp_path)
        for item in bundle["executions"][:3]:
            item["semantics"] = semantic(state="failed", error="resource.not_found", citations=[], effects={})
        bundle["executions"][3]["semantics"]["error_code"] = "auth.forbidden"
        result = module().reconcile(CATALOG, bundle, path)
        self.assertTrue(any(item["invariant"] == "error_code" for item in result["failures"]))

    def test_multi_observer_requires_literal_operation_identity(self):
        bundle, path = passing_bundle(self.tmp_path)
        next(item for item in bundle["executions"] if item["execution_id"] == "jobs-http")["observed_operation_id"] = "job-99"
        result = module().reconcile(CATALOG, bundle, path)
        self.assertTrue(any(item["invariant"] == "observed_operation_id" for item in result["failures"]))

    def test_parent_scenario_cannot_join_different_capabilities(self):
        bundle, path = passing_bundle(self.tmp_path)
        bundle["executions"][1]["capability"] = "unrelated-capability"
        result = module().reconcile(CATALOG, bundle, path)
        self.assertTrue(any(item["invariant"] == "capability.consistent" for item in result["failures"]))

    def test_transport_envelope_assertions_are_surface_specific(self):
        bundle, path = passing_bundle(self.tmp_path)
        bundle["executions"][0]["envelope"]["assertions"] = [{"id": "http.success_status", "passed": True}]
        result = module().reconcile(CATALOG, bundle, path)
        self.assertTrue(any(item["invariant"] == "envelope.surface_specific" for item in result["failures"]))

    def test_inventory_added_removed_and_unsupported_owner_drift_fail(self):
        bundle, path = passing_bundle(self.tmp_path)
        catalog = deepcopy(CATALOG)
        catalog["operations"].pop()
        catalog["operations"].append({"id": "cli:invented", "inventory": "cli",
                                      "classification": "unsupported", "reason": "not mapped"})
        result = module().reconcile(catalog, bundle, path)
        invariants = {item["invariant"] for item in result["failures"]}
        self.assertTrue({"inventory.classified", "inventory.current", "unsupported.rationale_owner"} <= invariants)

    def test_coverage_requires_saved_behavior_and_critical_polarities(self):
        bundle, path = passing_bundle(self.tmp_path)
        bundle["coverage"] = bundle["coverage"][:1]
        result = module().reconcile(CATALOG, bundle, path)
        invariants = {item["invariant"] for item in result["failures"]}
        self.assertIn("coverage.threshold", invariants)
        self.assertIn("coverage.critical_lifecycle", invariants)

    def test_coverage_evidence_is_bound_to_operation_and_scenario(self):
        bundle, path = passing_bundle(self.tmp_path)
        record = bundle["coverage"][0]
        (self.tmp_path / record["evidence_path"]).write_text(json.dumps({
            "operation_id": "cli:wrong", "scenario_id": record["scenario_id"],
            "kind": "behavioral", "result": "pass", "surface": record["surface"],
            "lifecycle": record["lifecycle"], "polarity": record["polarity"],
            "execution_id": record["execution_id"],
        }), encoding="utf-8")
        result = module().reconcile(CATALOG, bundle, path)
        self.assertTrue(any(item["invariant"] == "coverage.evidence_binding" for item in result["failures"]))

    def test_critical_lifecycle_labels_cannot_be_reassigned(self):
        bundle, path = passing_bundle(self.tmp_path)
        target = bundle["coverage"][6]
        target.update({"lifecycle": "source", "polarity": "negative"})
        result = module().reconcile(CATALOG, bundle, path)
        self.assertTrue(any(item["invariant"] == "coverage.scenario_binding" for item in result["failures"]))

    def test_coverage_requires_matching_executable_scenario_evidence(self):
        bundle, path = passing_bundle(self.tmp_path)
        bundle["coverage"][0]["execution_id"] = "invented-execution"
        result = module().reconcile(CATALOG, bundle, path)
        self.assertTrue(any(item["invariant"] == "coverage.execution_binding" for item in result["failures"]))

    def test_reconciler_cli_is_deterministic_offline(self):
        _, path = passing_bundle(self.tmp_path)
        command = [sys.executable, str(SCRIPT), str(path)]
        first = subprocess.run(command, cwd=ROOT, check=True, capture_output=True, text=True).stdout
        second = subprocess.run(command, cwd=ROOT, check=True, capture_output=True, text=True).stdout
        self.assertEqual(first, second)
        self.assertTrue(json.loads(first)["passed"])


if __name__ == "__main__":
    unittest.main()
