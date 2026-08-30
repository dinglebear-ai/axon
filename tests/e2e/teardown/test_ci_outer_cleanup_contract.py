from __future__ import annotations

import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]


class CiOuterCleanupContractTests(unittest.TestCase):
    def test_mutable_workflows_have_independent_always_cleanup_and_no_cancellation(self):
        for name in ("e2e-live.yml", "e2e-performance.yml", "e2e-platform-smoke.yml", "e2e-hermetic.yml"):
            with self.subTest(name=name):
                text = (ROOT / ".github/workflows" / name).read_text()
                self.assertIn("cancel-in-progress: false", text)
                self.assertNotIn("cancel-in-progress: true", text)
                self.assertIn("Outer ownership-checked teardown", text)
                self.assertIn("if: always()", text)
                self.assertIn("scripts/e2e/cleanup-owned-runs.py", text)
                self.assertIn("AXON_E2E_CLEANUP_REGISTRY", text)
                self.assertIn("runner.tool_cache", text)

    def test_persistent_and_live_lanes_recover_stale_runs_before_mutation(self):
        for name in ("e2e-live.yml", "e2e-performance.yml"):
            text = (ROOT / ".github/workflows" / name).read_text()
            self.assertIn("Recover stale owned runs before mutation", text)
            self.assertIn("--stale-seconds 21600", text)
        live = (ROOT / ".github/workflows/e2e-live.yml").read_text()
        self.assertGreaterEqual(live.count("--live-gateways"), 2)
        performance = (ROOT / ".github/workflows/e2e-performance.yml").read_text()
        self.assertNotIn("clean: false", performance)


if __name__ == "__main__": unittest.main()
