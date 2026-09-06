from __future__ import annotations

import base64
import importlib.util
import io
import json
import os
import tempfile
import unittest
from pathlib import Path
from urllib.parse import quote

ROOT = Path(__file__).resolve().parents[3]
spec = importlib.util.spec_from_file_location("e2e_redaction", ROOT / "scripts/e2e/lib/redaction.py")
redaction = importlib.util.module_from_spec(spec); spec.loader.exec_module(redaction)


class RedactionTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.root = Path(self.temp.name); self.secret = "Canary-S3cret/value"
    def tearDown(self): self.temp.cleanup()

    def test_stream_masks_environment_argv_headers_urls_nested_json_and_multiline(self):
        forms = [self.secret, self.secret.lower(), self.secret.upper(), quote(self.secret, safe=""),
                 base64.b64encode(self.secret.encode()).decode(), self.secret.encode().hex(),
                 "\n".join(self.secret[index:index + 3] for index in range(0, len(self.secret), 3))]
        source = json.dumps({"environment": forms[0], "argv": forms[1], "nested": {"value": forms[2]}})
        source += "\nAuthorization: Bearer " + forms[3] + "\nhttps://user:" + forms[4] + "@example.invalid/x?q=" + forms[5]
        sink = io.StringIO(); stream = redaction.RedactingStream(sink, [self.secret]); stream.write(source); stream.close()
        redaction.scan_bytes(sink.getvalue().encode(), [self.secret]); self.assertIn("[REDACTED]", sink.getvalue())

    def test_scanner_detects_every_transformed_canary_and_line_boundary(self):
        for value in redaction._variants(self.secret):
            with self.subTest(value=value), self.assertRaises(redaction.RedactionError):
                redaction.scan_bytes(f"provider echoed {value} in crash".encode(), [self.secret])

    def test_allowed_files_package_with_attributable_hash(self):
        source = self.root / "scenario" / "summary.json"; source.parent.mkdir(); source.write_text('{"result":"pass"}\n')
        destination = self.root / "package"
        manifest = redaction.package(self.root, [source], destination, [self.secret])
        self.assertEqual(1, len(manifest["files"])); self.assertEqual(64, len(manifest["files"][0]["sha256"]))
        self.assertEqual(manifest, json.loads((destination / "evidence-manifest.json").read_text()))

    def test_forbidden_env_database_profile_private_content_and_token_fixture_fail(self):
        cases = [self.root / ".env", self.root / "jobs.db", self.root / "chrome-profile" / "log.txt",
                 self.root / "private-content" / "raw.txt", self.root / "token.bin"]
        for index, source in enumerate(cases):
            source.parent.mkdir(parents=True, exist_ok=True); source.write_text("safe" if index < 4 else self.secret)
            with self.subTest(path=source), self.assertRaises(redaction.RedactionError):
                redaction.package(self.root, [source], self.root / f"out-{index}", [self.secret])

    def test_rejects_symlink_hardlink_fifo_traversal_and_out_of_root(self):
        source = self.root / "safe.txt"; source.write_text("safe")
        outside = Path(self.temp.name).parent / f"outside-{os.getpid()}.txt"; outside.write_text("safe")
        try:
            link = self.root / "link.txt"; link.symlink_to(source)
            hard = self.root / "hard.txt"; os.link(source, hard)
            fifo = self.root / "pipe.txt"; os.mkfifo(fifo)
            for index, value in enumerate((link, hard, fifo, outside)):
                with self.subTest(path=value), self.assertRaises(redaction.RedactionError):
                    redaction.package(self.root, [value], self.root / f"reject-{index}", [self.secret])
        finally: outside.unlink(missing_ok=True)

    def test_binary_and_credential_shaped_content_fail_closed(self):
        for data in (b"\xff\xfe", b"api_key=hunter2", b"Authorization: Bearer surprise",
                     b'token=bare-secret', b'{"token":"nested-secret"}',
                     b'{"auth_token":"nested-auth"}', b'session_cookie=secret-cookie'):
            with self.assertRaises(redaction.RedactionError): redaction.scan_bytes(data, [])

    def test_stream_secret_split_across_writes_never_reaches_sink(self):
        sink = io.StringIO(); stream = redaction.RedactingStream(sink, [self.secret])
        stream.write(self.secret[:8]); stream.flush(); self.assertEqual("", sink.getvalue())
        stream.write(self.secret[8:]); stream.close(); self.assertEqual("[REDACTED]", sink.getvalue())

    def test_dynamic_credential_is_masked_immediately_and_used_by_stream(self):
        control = io.StringIO(); masker = redaction.CredentialMasker(control); token = masker.acquire(self.secret)
        self.assertEqual(self.secret, token); self.assertTrue(control.getvalue().startswith("::add-mask::"))
        output = io.StringIO(); stream = masker.stream(output); stream.write("error echoed " + token); stream.close()
        self.assertNotIn(token, output.getvalue())

    def test_command_policy_forbids_debug_env_trace_and_secret_argv(self):
        cases = [("bash", "-c", "set -x"), ("curl", "-v", "https://localhost"), ("printenv",),
                 ("curl", "--trace", "-", "https://localhost"), ("tool", "--token", self.secret)]
        for argv in cases:
            with self.subTest(argv=argv), self.assertRaises(redaction.RedactionError):
                redaction.validate_command(argv, [self.secret])

    def test_name_variants_archives_and_byte_ceilings_fail_closed(self):
        for index, relative in enumerate(("chrome-profiles/log.txt", "private_content/raw.txt", "bundle.zip")):
            source = self.root / relative; source.parent.mkdir(parents=True, exist_ok=True); source.write_text("safe")
            with self.subTest(relative=relative), self.assertRaises(redaction.RedactionError):
                redaction.package(self.root, [source], self.root / f"variant-{index}", [])
        source = self.root / "large.log"; source.write_text("x" * 32)
        old = redaction.MAX_FILE_BYTES; redaction.MAX_FILE_BYTES = 16
        try:
            with self.assertRaises(redaction.RedactionError): redaction.package(self.root, [source], self.root / "large-out", [])
        finally: redaction.MAX_FILE_BYTES = old


if __name__ == "__main__": unittest.main()
