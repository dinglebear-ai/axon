from __future__ import annotations

import importlib.util
import os
import subprocess
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
SCRIPT = ROOT / "scripts/e2e/validate-live-config.py"
REQUIRED_NAMES = {
    "TS_WIF_CLIENT_ID",
    "TS_WIF_AUDIENCE",
    "AXON_E2E_EXPECTED_PEERS",
    "AXON_E2E_QDRANT_GATEWAY_URL",
    "AXON_E2E_QDRANT_PEER",
    "AXON_E2E_QDRANT_TOKEN",
    "AXON_E2E_TEI_GATEWAY_URL",
    "AXON_E2E_TEI_PEER",
    "AXON_E2E_TEI_TOKEN",
    "AXON_E2E_CHROME_GATEWAY_URL",
    "AXON_E2E_CHROME_PEER",
    "AXON_E2E_CHROME_TOKEN",
    "AXON_E2E_LLM_GATEWAY_URL",
    "AXON_E2E_LLM_PEER",
    "AXON_E2E_LLM_TOKEN",
}


def load_validator():
    spec = importlib.util.spec_from_file_location("axon_live_config_validator", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class LiveConfigTests(unittest.TestCase):
    def test_required_names_follow_provider_contract(self):
        validator = load_validator()
        self.assertEqual(REQUIRED_NAMES, validator.required_names())

    def test_missing_configuration_reports_names_only(self):
        validator = load_validator()
        env = {name: "configured-value" for name in REQUIRED_NAMES}
        env["AXON_E2E_LLM_TOKEN"] = ""
        env["TS_WIF_AUDIENCE"] = "  "

        self.assertEqual(
            ["AXON_E2E_LLM_TOKEN", "TS_WIF_AUDIENCE"],
            validator.missing_names(env),
        )

    def test_entrypoint_fails_closed_without_printing_values(self):
        validator = load_validator()
        env = os.environ.copy()
        for name in REQUIRED_NAMES:
            env.pop(name, None)
        marker = "must-not-appear-in-output"
        env["AXON_E2E_QDRANT_TOKEN"] = marker

        completed = subprocess.run(
            [sys.executable, str(SCRIPT)],
            env=env,
            capture_output=True,
            text=True,
            check=False,
        )

        self.assertNotEqual(0, completed.returncode)
        self.assertIn("missing live E2E configuration:", completed.stderr)
        self.assertIn("TS_WIF_AUDIENCE", completed.stderr)
        self.assertNotIn(marker, completed.stdout + completed.stderr)

    def test_entrypoint_accepts_complete_configuration(self):
        validator = load_validator()
        env = os.environ.copy()
        env.update({name: "configured-value" for name in REQUIRED_NAMES})

        completed = subprocess.run(
            [sys.executable, str(SCRIPT)],
            env=env,
            capture_output=True,
            text=True,
            check=False,
        )

        self.assertEqual(0, completed.returncode, completed.stderr)
        self.assertEqual("", completed.stdout)
        self.assertEqual("", completed.stderr)


if __name__ == "__main__":
    unittest.main()
