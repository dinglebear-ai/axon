import importlib.util
import json
import http.server
import os
from pathlib import Path
import subprocess
import tempfile
import threading
import unittest


ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("e2e_cli_adapter", ROOT / "scripts/e2e/adapters/cli.py")
adapter = importlib.util.module_from_spec(SPEC)
assert SPEC.loader
SPEC.loader.exec_module(adapter)


class CliAdapterTests(unittest.TestCase):
    def fixture_endpoint(self):
        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), http.server.SimpleHTTPRequestHandler)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        self.addCleanup(server.server_close)
        self.addCleanup(server.shutdown)
        return f"http://127.0.0.1:{server.server_port}"

    def test_hostile_values_remain_single_argv_elements(self):
        hostile = "space newline\n* ' \" ` $(touch nope); --confusable-\N{CYRILLIC SMALL LETTER A} \x01"
        scenario = {"id": "source.inline.happy"}
        argv = adapter.scenario_argv(scenario, {"source": hostile, "scope": "page"})
        self.assertEqual(hostile, argv[0])
        self.assertEqual([hostile, "--scope", "page", "--wait", "true", "--json"], argv)
        negative = adapter.scenario_argv(
            {"id": "source.detached.negative"}, {"source": hostile, "scope": "page"}
        )
        self.assertEqual("", negative[0])
        self.assertEqual("false", negative[4])
        missing_job = adapter.scenario_argv(
            {"id": "jobs.cancel.negative"}, {}, {"fixture.job": "must_not_be_used"}
        )
        self.assertEqual(["jobs", "cancel", "e2e_missing_job", "--json"], missing_job)

    def test_selection_is_deterministic_and_rejects_unknown_ids(self):
        catalog = json.loads((ROOT / "tests/e2e/catalog/catalog.json").read_text())
        first = adapter.selected_scenarios(catalog, set(), "source", 0, 1)
        second = adapter.selected_scenarios(catalog, set(), "source", 0, 1)
        self.assertEqual([item["id"] for item in first], [item["id"] for item in second])
        with self.assertRaisesRegex(ValueError, "unknown CLI scenario"):
            adapter.selected_scenarios(catalog, {"missing"}, None, 0, 1)
        with self.assertRaisesRegex(ValueError, "unknown or empty CLI scenario group"):
            adapter.selected_scenarios(catalog, set(), "missing", 0, 1)

    def test_hermetic_http_fixture_is_required_and_loopback_only(self):
        scenario = {"id": "source.inline.happy", "setup_dependencies": ["fixture.http"]}
        fixture = {"source": "https://example.invalid"}
        previous = os.environ.pop("AXON_E2E_FIXTURE_BASE_URL", None)
        try:
            with self.assertRaisesRegex(ValueError, "requires AXON_E2E_FIXTURE_BASE_URL"):
                adapter.apply_hermetic_projection(scenario, fixture)
            os.environ["AXON_E2E_FIXTURE_BASE_URL"] = "https://public.example.com"
            with self.assertRaisesRegex(ValueError, "must be loopback"):
                adapter.apply_hermetic_projection(scenario, fixture)
        finally:
            if previous is None:
                os.environ.pop("AXON_E2E_FIXTURE_BASE_URL", None)
            else:
                os.environ["AXON_E2E_FIXTURE_BASE_URL"] = previous

    def test_direct_entry_point_emits_normalized_evidence(self):
        with tempfile.TemporaryDirectory() as directory:
            temp = Path(directory)
            calls = temp / "calls"
            fake = temp / "axon"
            fake.write_text(
                "#!/usr/bin/env python3\n"
                "import json, os, sys\n"
                "open(os.environ['CALLS'], 'w').write(json.dumps(sys.argv[1:]))\n"
                "print(json.dumps({'status':'completed','job_id':'job_1'}))\n"
            )
            fake.chmod(0o755)
            output = temp / "out"
            completed = subprocess.run(
                [str(ROOT / "scripts/e2e/adapters/cli.sh"), "--axon-bin", str(fake),
                 "--outdir", str(output), "--scenario", "source.inline.happy"],
                cwd=ROOT, env={**os.environ, "CALLS": str(calls), "AXON_E2E_FIXTURE_BASE_URL": self.fixture_endpoint()},
                capture_output=True, text=True,
            )
            self.assertEqual(0, completed.returncode, completed.stderr)
            argv = json.loads(calls.read_text())
            self.assertTrue(argv[0].startswith("http://127.0.0.1:"))
            self.assertEqual(["--scope", "page", "--wait", "true", "--json"], argv[1:])
            records = [json.loads(line) for line in (output / "cli-evidence.jsonl").read_text().splitlines()]
            self.assertEqual(1, len(records))
            record = records[0]
            self.assertEqual("pass", record["result"])
            self.assertEqual("cli", record["surface"])
            self.assertEqual(40, len(record["tested_sha"]))
            self.assertEqual(1, record["attempts"])
            self.assertTrue(record["cleanup"]["registered"])
            self.assertTrue(Path(record["cleanup"]["manifest"]).is_file())
            self.assertEqual(
                [{"type": "job", "identity": "job_1"}],
                record["cleanup"]["command_created_resources"],
            )
            self.assertTrue(all(item["passed"] for item in record["assertions"]))

    def test_jobs_and_destructive_oracles_inspect_semantics(self):
        jobs = {
            "polarity": "happy", "semantic_oracles": ["job.visible", "job.transition_valid"]
        }
        result, failure, assertions = adapter.classify(
            0, b'{"items":[{"job_id":"fixture_1","status":"running"}]}', b"", jobs, "fixture_1"
        )
        self.assertEqual(("pass", None), (result, failure))
        self.assertTrue(all(item["passed"] for item in assertions))

        destructive = {
            "polarity": "happy", "semantic_oracles": ["prune.plan_digest_bound", "resource.ownership_checked"]
        }
        result, _, assertions = adapter.classify(
            0, b'{"ok":true,"plan":{"digest":"sha256:abc","ownership_checked":true}}', b"", destructive
        )
        self.assertEqual("pass", result)
        self.assertTrue(all(item["passed"] for item in assertions))

    def test_provider_error_envelope_never_counts_as_success(self):
        source = {
            "polarity": "happy", "semantic_oracles": ["source.accepted", "job.terminal_success"]
        }
        result, failure, assertions = adapter.classify(
            0, b'{"code":"provider.unavailable","status":"completed","job_id":"job_1"}', b"", source
        )
        self.assertEqual(("fail", "provider"), (result, failure))
        self.assertFalse(next(item["passed"] for item in assertions if item["id"] == "cli.no_provider_error_envelope"))

    def test_negative_rejection_oracles_require_specific_error_families(self):
        scenario = {
            "polarity": "negative",
            "semantic_oracles": ["rejection.job_missing", "failure.taxonomy"],
        }
        result, failure, assertions = adapter.classify(
            2, b'{"error":{"code":"jobs.not_found"}}', b"", scenario
        )
        self.assertEqual(("pass", None), (result, failure))
        self.assertTrue(all(item["passed"] for item in assertions))
        result, _, _ = adapter.classify(2, b'{"error":{"code":"validation.bad_argument"}}', b"", scenario)
        self.assertEqual("fail", result)

    def test_text_only_invalid_source_rejection_is_normalized_but_raw_stderr_is_preserved(self):
        with tempfile.TemporaryDirectory() as directory:
            temp = Path(directory)
            fake = temp / "axon"
            fake.write_text(
                "#!/usr/bin/env python3\n"
                "import sys\n"
                "assert sys.argv[1] == ''\n"
                "print(\"error: invalid value '' for '<SOURCE>': source cannot be empty\", file=sys.stderr)\n"
                "raise SystemExit(2)\n"
            )
            fake.chmod(0o755)
            output = temp / "out"
            completed = subprocess.run(
                [str(ROOT / "scripts/e2e/adapters/cli.sh"), "--axon-bin", str(fake),
                 "--outdir", str(output), "--scenario", "source.detached.negative"],
                cwd=ROOT, env={**os.environ, "AXON_E2E_FIXTURE_BASE_URL": self.fixture_endpoint()},
                capture_output=True, text=True,
            )
            self.assertEqual(0, completed.returncode, completed.stderr)
            record = json.loads((output / "cli-evidence.jsonl").read_text().strip())
            self.assertEqual("pass", record["result"])
            self.assertEqual(1, record["attempts"])
            attempt = record["attempt_history"][0]
            self.assertEqual("stderr", attempt["normalized_envelope"]["normalized_from"])
            self.assertEqual("validation.source_invalid", attempt["normalized_envelope"]["error"]["code"])
            self.assertEqual(b"", Path(attempt["stdout"]).read_bytes())
            self.assertIn("source cannot be empty", Path(attempt["stderr"]).read_text())

    def test_missing_job_and_fake_provider_fail_before_execution(self):
        with tempfile.TemporaryDirectory() as directory:
            temp = Path(directory)
            marker = temp / "called"
            fake = temp / "axon"
            fake.write_text(f"#!/bin/sh\ntouch '{marker}'\n")
            fake.chmod(0o755)
            completed = subprocess.run(
                [str(ROOT / "scripts/e2e/adapters/cli.sh"), "--axon-bin", str(fake),
                 "--outdir", str(temp / "out"), "--scenario", "jobs.cancel.negative"],
                cwd=ROOT, capture_output=True, text=True,
            )
            self.assertEqual(2, completed.returncode)
            self.assertFalse(marker.exists())
            self.assertIn("AXON_E2E_FAKE_PROVIDER_URL", completed.stderr)

    def test_timeout_retry_is_bounded_and_preserves_first_attempt(self):
        with tempfile.TemporaryDirectory() as directory:
            temp = Path(directory)
            fake = temp / "axon"
            fake.write_text("#!/bin/sh\nsleep 2\n")
            fake.chmod(0o755)
            output = temp / "out"
            completed = subprocess.run(
                [str(ROOT / "scripts/e2e/adapters/cli.sh"), "--axon-bin", str(fake),
                 "--outdir", str(output), "--scenario", "source.inline.happy", "--timeout-secs", "0.02"],
                cwd=ROOT, env={**os.environ, "AXON_E2E_FIXTURE_BASE_URL": self.fixture_endpoint()},
                capture_output=True, text=True,
            )
            self.assertEqual(1, completed.returncode, completed.stderr)
            record = json.loads((output / "cli-evidence.jsonl").read_text().strip())
            self.assertEqual(2, record["attempts"])
            self.assertEqual(["timeout", "timeout"], [item["result"] for item in record["attempt_history"]])
            self.assertNotEqual(record["attempt_history"][0]["namespace"], record["attempt_history"][1]["namespace"])


if __name__ == "__main__":
    unittest.main()
