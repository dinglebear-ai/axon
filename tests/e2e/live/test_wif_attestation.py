import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parents[3]
spec = importlib.util.spec_from_file_location("wif_attestation", ROOT / "scripts/e2e/attest-wif.py")
attestation = importlib.util.module_from_spec(spec)
spec.loader.exec_module(attestation)


class WifAttestationTests(unittest.TestCase):
    def test_missing_audience_fails_before_requesting_an_identity_token(self):
        for audience in ("", "   "):
            with self.subTest(audience=audience), tempfile.TemporaryDirectory() as directory:
                output = Path(directory) / "attestation.json"
                args = ["attest-wif", "--audience", audience, "--out", str(output)]
                with mock.patch.object(sys, "argv", args), mock.patch.dict(
                    attestation.os.environ, {}, clear=True
                ):
                    with self.assertRaisesRegex(SystemExit, "TS_WIF_AUDIENCE must be configured"):
                        attestation.main()
                self.assertFalse(output.exists())
