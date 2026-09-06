import copy
import importlib.util
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
SPEC = importlib.util.spec_from_file_location("validate_catalog", ROOT / "scripts/e2e/validate-catalog.py")
validator = importlib.util.module_from_spec(SPEC)
assert SPEC.loader
SPEC.loader.exec_module(validator)


class CatalogValidationTests(unittest.TestCase):
    def setUp(self):
        self.catalog = validator.load(validator.DEFAULT_CATALOG)

    def assert_rejected(self, mutate, fragment):
        candidate = copy.deepcopy(self.catalog)
        mutate(candidate)
        self.assertTrue(any(fragment in error for error in validator.validate(candidate)))

    def test_current_catalog_reconciles_all_inventories(self):
        self.assertEqual([], validator.validate(self.catalog))

    def test_rejects_missing_and_duplicate_ids(self):
        self.assert_rejected(lambda value: value["operations"].pop(), "unclassified advertised")
        self.assert_rejected(lambda value: value["scenarios"][0].pop("id"), "missing required property")
        self.assert_rejected(lambda value: value["scenarios"].append(copy.deepcopy(value["scenarios"][0])), "duplicate scenario IDs")

    def test_rejects_unknown_surface_and_missing_cleanup(self):
        self.assert_rejected(lambda value: value["scenarios"][0]["surfaces"].append("grpc"), "is not in")
        self.assert_rejected(lambda value: value["scenarios"][0].update(cleanup_contract=None), "requires cleanup_contract")

    def test_rejects_executable_dsl_and_weakened_envelope(self):
        self.assert_rejected(lambda value: value["scenarios"][0].update(command="sh -c anything"), "forbidden executable key")
        self.assert_rejected(lambda value: value["scenarios"][0]["envelope_oracles"].pop("cli"), "every surface needs request and envelope assertions")

    def test_rejects_schema_and_fixture_violations(self):
        self.assert_rejected(lambda value: value["scenarios"][0].update(fixture="../../etc/passwd"), "required pattern")
        self.assert_rejected(lambda value: value["scenarios"][0].update(fixture="tests/e2e/catalog/fixtures/missing.json"), "does not exist")
        self.assert_rejected(lambda value: value["scenarios"][0]["weights"].update(cpu="heavy"), "expected type")
        self.assert_rejected(lambda value: value["scenarios"][0].update(failure_taxonomy=[]), "too few items")
        self.assert_rejected(lambda value: value["operations"].append({"id": "cli:invented", "inventory": "cli", "classification": "contract_only", "reason": "invented"}), "absent from authoritative")

    def test_rejects_setup_retry_and_ownership_contract_violations(self):
        self.assert_rejected(lambda value: value["scenarios"][0].update(setup_dependencies=["valid", 7]), "expected type")
        self.assert_rejected(lambda value: value["scenarios"][0].update(retry_class="unbounded"), "is not in")
        self.assert_rejected(lambda value: value["scenarios"][0]["resource_ownership"].update(strategy="ambient"), "is not in")
        self.assert_rejected(lambda value: value["scenarios"][0].update(resource_ownership={"strategy": "none", "namespace_prefix": None, "lease_required": False}), "requires run_manifest ownership")

    def test_rejects_coverage_and_critical_lifecycle_regressions(self):
        self.assert_rejected(lambda value: value["operations"][0].update(classification="contract_only"), "below")
        self.assert_rejected(lambda value: value["scenarios"].__setitem__(slice(None), [item for item in value["scenarios"] if not (item["lifecycle"] == "jobs" and item["polarity"] == "negative")]), "requires happy and negative")

    def test_catalog_cannot_be_empty(self):
        self.assert_rejected(lambda value: value["operations"].clear(), "too few items")
        self.assert_rejected(lambda value: value["scenarios"].clear(), "too few items")

    def test_failure_taxonomy_matches_canonical_reporting_schema(self):
        reporting_schema = validator.load(validator.ROOT / "tests/e2e/reporting/report.schema.json")
        attempt = reporting_schema["properties"]["scenarios"]["items"]["properties"]["attempts"]["items"]
        report_values = set(attempt["properties"]["classification"]["enum"]) - {None}
        self.assertEqual({"product", "fixture", "provider", "auth_network", "cleanup", "harness"}, report_values)
        self.assertTrue(all(set(item["failure_taxonomy"]) <= report_values for item in self.catalog["scenarios"]))


if __name__ == "__main__":
    unittest.main()
