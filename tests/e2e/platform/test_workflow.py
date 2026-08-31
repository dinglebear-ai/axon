import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]


class PlatformWorkflowTests(unittest.TestCase):
    def test_workflow_is_bounded_secretless_and_pinned(self):
        text = (ROOT / ".github/workflows/e2e-platform-smoke.yml").read_text()
        for runner in ("ubuntu-latest", "macos-latest", "windows-latest"): self.assertIn(runner, text)
        self.assertIn("timeout-minutes: 15", text)
        self.assertIn("permissions:\n  contents: read", text)
        self.assertNotIn("tailscale", text.lower())
        self.assertNotIn("secrets.", text)
        self.assertIn("actions/checkout@34e114876b0b11c390a56381ad16ebd13914f8d5", text)
        self.assertIn("actions/upload-artifact@65462800fd760344b1a7b4382951275a0abb4808", text)
        self.assertIn("python scripts/e2e/run-platform-smoke.py", text)
        self.assertIn("--attestations-out target/e2e/prior-history-attestations.json",text)
        self.assertIn("--evidence-artifact-template 'e2e-platform-smoke-${{ runner.os }}-{run_id}-{run_attempt}'",text)


if __name__ == "__main__": unittest.main()
