from __future__ import annotations

import subprocess
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
ENTRY = ROOT / "tests/e2e/scenarios/observability/live_entry.py"


class LiveObservabilityEntryTests(unittest.TestCase):
    def test_live_entry_fails_closed_without_trust_and_descriptor(self):
        completed = subprocess.run([sys.executable, str(ENTRY), "--launcher-descriptor", "missing.json"],
                                   cwd=ROOT, capture_output=True, text=True, check=False)
        self.assertNotEqual(0, completed.returncode)
        self.assertIn("AXON_E2E_TRUSTED_LIVE", completed.stderr)


if __name__ == "__main__": unittest.main()
