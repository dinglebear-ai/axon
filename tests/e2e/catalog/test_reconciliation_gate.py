from __future__ import annotations

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
CATALOG = json.loads((ROOT / "tests/e2e/catalog/catalog.json").read_text())
SPEC = importlib.util.spec_from_file_location(
    "reconcile_gate", ROOT / "scripts/e2e/reconcile-surfaces.py"
)
reconcile = importlib.util.module_from_spec(SPEC)
assert SPEC.loader
SPEC.loader.exec_module(reconcile)


class RequiredReconciliationGateTests(unittest.TestCase):
    def test_behavioral_claim_without_executable_evidence_fails_closed(self):
        operation = next(
            item for item in CATALOG["operations"]
            if item["classification"] == "behavioral_e2e"
        )
        scenario = CATALOG["scenarios"][0]
        with tempfile.TemporaryDirectory() as temporary:
            bundle_path = Path(temporary) / "bundle.json"
            bundle = {
                "schema_version": 1,
                "executions": [],
                "coverage": [{
                    "operation_id": operation["id"],
                    "scenario_id": scenario["id"],
                    "surface": scenario["surfaces"][0],
                    "kind": "behavioral",
                    "result": "pass",
                    "evidence_path": "missing.json",
                    "lifecycle": scenario["lifecycle"],
                    "polarity": scenario["polarity"],
                    "execution_id": "missing-execution",
                }],
            }
            bundle_path.write_text(json.dumps(bundle))
            result = reconcile.reconcile(CATALOG, bundle, bundle_path)
        self.assertFalse(result["passed"])
        self.assertIn("coverage.execution", {item["invariant"] for item in result["failures"]})


if __name__ == "__main__":
    unittest.main()
