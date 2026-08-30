from __future__ import annotations

import importlib.util
import json
import os
import stat
import subprocess
import sys
import tempfile
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
ORCHESTRATOR = ROOT / "tests/e2e/scenarios/source/orchestrator.py"
FAKE = ROOT / "tests/e2e/fixtures/source/fake_axon.py"
SPEC = importlib.util.spec_from_file_location("source_job_orchestrator", ORCHESTRATOR)
orchestrator = importlib.util.module_from_spec(SPEC)
assert SPEC and SPEC.loader
SPEC.loader.exec_module(orchestrator)
CORPUS = ROOT / "tests/e2e/corpus/v1/revisions/atlas"


class SourceJobOrchestratorTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.binary = self.root / "axon"
        self.binary.write_text(f"#!{sys.executable}\nexec(compile(open({str(FAKE)!r}).read(), {str(FAKE)!r}, 'exec'))\n")
        self.binary.chmod(self.binary.stat().st_mode | stat.S_IXUSR)
        self.calls = self.root / "calls.jsonl"
        self.previous = os.environ.get("AXON_E2E_FAKE_CALLS")
        os.environ["AXON_E2E_FAKE_CALLS"] = str(self.calls)
        self.acceptance = orchestrator.SourceJobAcceptance.create(self.binary, self.root / "work")

    def tearDown(self):
        if self.previous is None: os.environ.pop("AXON_E2E_FAKE_CALLS", None)
        else: os.environ["AXON_E2E_FAKE_CALLS"] = self.previous
        self.temp.cleanup()

    def test_drives_real_cli_argv_and_asserts_only_returned_public_evidence(self):
        source = str(CORPUS / "1.0.0.md")
        result = self.acceptance.source(source, "file")
        class Snapshot:
            def snapshot(self, _collection, source_id):
                return {"point_ids": ["point-1"], "generations": ["gen_1"], "fetch_methods": [], "count": 1,
                        "lineage": [{"source_id": source_id, "source_canonical_uri": result["canonical_uri"],
                                     "source_item_key": "item-1", "item_canonical_uri": result["canonical_uri"],
                                     "source_generation": "gen_1", "document_id": "doc-1",
                                     "chunk_text": "Atlas deterministic fixture"}]}
        evidence = self.acceptance.assert_observable(result, source, Snapshot())
        self.assertEqual("completed", evidence["detail"]["status"])
        calls = [json.loads(line) for line in self.calls.read_text().splitlines()]
        self.assertIn(["source", source, "--scope", "file", "--wait", "true", "--collection",
                       self.acceptance.collection, "--json"], calls)
        self.assertTrue(any(call[:2] == ["jobs", "events"] for call in calls))
        self.assertTrue(any(call[:2] == ["jobs", "stream"] for call in calls))
        self.assertTrue(any(call[:2] == ["artifacts", "list"] for call in calls))
        self.assertTrue(any(call[:2] == ["graph", "query"] for call in calls))

    def test_refresh_cancel_race_retry_reconnect_and_registration(self):
        stable = self.root / "stable.md"
        results = self.acceptance.refresh(
            stable, CORPUS / "1.0.0.md", CORPUS / "1.0.1-unchanged.md", CORPUS / "1.1.0-changed.md",
        )
        self.assertEqual(["gen_1", "gen_1", "gen_2"], [item["ledger"]["generation"] for item in results])
        race = self.acceptance.cancel_complete_race("http://127.0.0.1:32123/page.html", "page")
        self.assertIn(race["status"], {"completed", "canceled"})
        retried = self.acceptance.retry_transient("http://127.0.0.1:32123/transient")
        self.assertEqual("completed", retried["status"])
        reconnected = self.acceptance.recover_after_restart(retried["job_id"])
        self.assertEqual("completed", reconnected["job"]["status"])
        calls = [json.loads(line) for line in self.calls.read_text().splitlines()]
        self.assertTrue(any(call[:2] == ["jobs", "recover"] and "--stale-before" in call for call in calls))
        self.acceptance.verify_cleanup_registration()

    def test_preflight_fails_closed_without_binary_or_healthy_providers(self):
        with self.assertRaises(orchestrator.AcceptanceError):
            orchestrator.SourceJobAcceptance.create(self.root / "missing", self.root / "missing-work")
        unhealthy = self.root / "unhealthy"
        unhealthy.write_text("#!/bin/sh\nprintf '{\"all_ok\":false}\\n'\n")
        unhealthy.chmod(unhealthy.stat().st_mode | stat.S_IXUSR)
        with self.assertRaisesRegex(orchestrator.AcceptanceError, "preflight failed"):
            orchestrator.SourceJobAcceptance.create(unhealthy, self.root / "unhealthy-work")

    def test_exact_stage_cancellation_partial_debt_and_chrome_contracts(self):
        class Release(BaseHTTPRequestHandler):
            def log_message(self, *_args): pass
            def do_GET(self): self.send_response(200); self.end_headers(); self.wfile.write(b"released")
        server = ThreadingHTTPServer(("127.0.0.1", 0), Release)
        thread = threading.Thread(target=server.serve_forever, daemon=True); thread.start()
        release = f"http://127.0.0.1:{server.server_port}/release"
        try:
            fetched = self.acceptance.cancel_at_stage("http://fixture/block-fetch", "fetching", release)
            embedded = self.acceptance.cancel_at_stage("http://fixture/block-embed", "embedding", release)
            published = self.acceptance.cancel_after_partial_publication(
                "http://fixture/block-publish", release, release)
            class ChromeSnapshot:
                def snapshot(self, _collection, _source_id):
                    return {"point_ids": ["point-1"], "generations": ["gen_1"],
                            "fetch_methods": ["chrome_render"], "count": 1}
            chrome = self.acceptance.chrome_rendered("http://fixture/chrome-js", ChromeSnapshot())
        finally:
            server.shutdown(); server.server_close(); thread.join(timeout=2)
        self.assertEqual("canceled", fetched["terminal"]["status"])
        self.assertEqual("canceled", embedded["terminal"]["status"])
        self.assertTrue(published["cancel"]["side_effects"])
        self.assertTrue(published["cancel"]["cleanup_debt_ids"])
        self.assertIn("AXON_E2E_JS_ONLY_CONTENT", json.dumps(chrome["retrieve"]))

    def test_mcp_content_decoder_is_recursive_and_fails_closed(self):
        payload = {"job_id": "job_1", "source_id": "src_1", "status": "completed"}
        nested = {"result": {"content": [{"type": "text", "text": json.dumps(payload)}]}}
        self.assertEqual(payload, orchestrator.McpJobsClient.decode_content(nested))
        with self.assertRaises(orchestrator.AcceptanceError):
            orchestrator.McpJobsClient.decode_content({"content": [{"type": "text", "text": "not-json"}]})

    def test_real_worker_process_is_killed_recovered_and_replaced(self):
        class Release(BaseHTTPRequestHandler):
            def log_message(self, *_args): pass
            def do_GET(self): self.send_response(200); self.end_headers()
        server = ThreadingHTTPServer(("127.0.0.1", 0), Release)
        thread = threading.Thread(target=server.serve_forever, daemon=True); thread.start()
        try:
            result = self.acceptance.worker_crash_recover(
                "http://fixture/block-fetch-worker", "fetching",
                f"http://127.0.0.1:{server.server_port}/release")
        finally:
            server.shutdown(); server.server_close(); thread.join(timeout=2)
        self.assertGreaterEqual(result["recovery"]["recovered"], 1)
        self.assertEqual("completed", result["terminal"]["status"])
        self.assertEqual({1, 2}, {item["attempt"] for item in result["events"]["events"]})

    def test_http_and_mcporter_source_creation_clients_execute_real_protocols(self):
        def result(source_id, job_id):
            return {"job_id": job_id, "source_id": source_id, "canonical_uri": "http://fixture/page",
                    "source_kind": "web", "adapter": {"id": "web"}, "scope": "page",
                    "status": "completed", "ledger": {"source_id": source_id, "generation": "gen_1",
                    "committed_generation": "gen_1", "status": "completed", "counts": {}},
                    "graph": {"nodes_upserted": 0, "edges_upserted": 0, "evidence_records": 0, "degraded": False},
                    "counts": {"documents_total": 1}, "warnings": [], "artifacts": [], "errors": []}
        http_result = result("src_http", "job_http")
        class Api(BaseHTTPRequestHandler):
            def log_message(self, *_args): pass
            def do_POST(self):
                request = json.loads(self.rfile.read(int(self.headers.get("Content-Length", "0"))))
                if request.get("source") != "http://fixture/page":
                    body = json.dumps({"code": "route.validation.missing_field", "error": "source required"}).encode()
                    self.send_response(400); self.send_header("Content-Length", str(len(body))); self.end_headers(); self.wfile.write(body); return
                body = json.dumps(http_result).encode(); self.send_response(200); self.send_header("Content-Length", str(len(body))); self.end_headers(); self.wfile.write(body)
            def do_GET(self):
                if self.path == "/v1/jobs":
                    body = json.dumps({"items": [], "total": 0}).encode(); self.send_response(200); self.send_header("Content-Length", str(len(body))); self.end_headers(); self.wfile.write(body); return
                source_id = self.path.rsplit("/", 1)[-1]
                job_id = "job_http" if source_id == "src_http" else "job_mcp"
                body = json.dumps({"summary": {"source_id": source_id, "last_job_id": job_id},
                    "committed_generation": "gen_1", "manifest": {"generation": "gen_1",
                    "status": "completed", "item_count": 1, "items": [{"source_item_key": "item-1",
                    "canonical_uri": "http://fixture/page", "content_hash": "abc"}]},
                    "documents": [{"document_id": "doc-1", "source_item_key": "item-1",
                    "generation": "gen_1", "status": "published", "chunk_count": 1,
                    "vector_point_count": 1, "updated_at": "2026-01-01T00:00:00Z"}]}).encode()
                self.send_response(200); self.send_header("Content-Length", str(len(body))); self.end_headers(); self.wfile.write(body)
        server = ThreadingHTTPServer(("127.0.0.1", 0), Api)
        thread = threading.Thread(target=server.serve_forever, daemon=True); thread.start()
        mcporter = self.root / "mcporter"
        mcp_result = result("src_http", "job_mcp")
        mcporter.write_text(f"#!{sys.executable}\nimport json,sys\na=json.loads(sys.argv[sys.argv.index('--args')+1])\nif a.get('source') != 'http://fixture/page':\n print(json.dumps({{'code':'invalid_params','error':'source required or unsafe'}}),file=sys.stderr);sys.exit(2)\np={{'data':{{'response_mode':'inline','inline':{mcp_result!r}}}}}\nprint(json.dumps({{'content':[{{'type':'text','text':json.dumps(p)}}]}}))\n")
        mcporter.chmod(mcporter.stat().st_mode | stat.S_IXUSR)
        try:
            evidence = self.acceptance.assert_transport_source_creation(
                "http://fixture/page", orchestrator.HttpJobsClient(f"http://127.0.0.1:{server.server_port}"),
                orchestrator.McpJobsClient(mcporter, "axon.axon"))
        finally:
            server.shutdown(); server.server_close(); thread.join(timeout=2)
        self.assertEqual("src_http", evidence["http"]["source_id"])
        self.assertEqual("src_http", evidence["mcp"]["source_id"])

    def test_transport_lifecycle_catalog_uses_production_rest_and_mcp_shapes(self):
        class Http:
            def __init__(self): self.calls = []
            def rejected(self, method, path, body=None):
                self.calls.append((method, path, body)); return {"code": "job.not_found"}
        class Mcp:
            def __init__(self): self.calls = []
            def rejected(self, body): self.calls.append(body); return {"code": "invalid_params"}
        http, mcp = Http(), Mcp()
        self.acceptance.assert_transport_lifecycle_negatives(http, mcp)
        self.assertEqual(["/v1/jobs/00000000-0000-0000-0000-000000000000/cancel",
                          "/v1/jobs/00000000-0000-0000-0000-000000000000/retry",
                          "/v1/jobs/recover"], [item[1] for item in http.calls])
        self.assertEqual(["cancel", "retry", "recover"], [item["subaction"] for item in mcp.calls])


class RealAxonSourceJobTests(unittest.TestCase):
    def test_opt_in_real_axon_runner(self):
        binary = os.environ.get("AXON_E2E_REAL_AXON_BIN")
        required = os.environ.get("AXON_E2E_REQUIRE_REAL_SOURCE_JOBS") == "1"
        if not binary:
            if required:
                self.fail("AXON_E2E_REQUIRE_REAL_SOURCE_JOBS=1 requires AXON_E2E_REAL_AXON_BIN")
            self.skipTest("set AXON_E2E_REAL_AXON_BIN to run real Axon source/jobs acceptance")
        if os.environ.get("AXON_E2E_HERMETIC") == "1":
            with tempfile.TemporaryDirectory() as directory:
                env={**os.environ,"AXON_DATA_DIR":directory,"AXON_SQLITE_PATH":str(Path(directory)/"jobs.db"),
                     "QDRANT_URL":"http://127.0.0.1:9","TEI_URL":"http://127.0.0.1:9"}
                version=subprocess.run([binary,"--version"],env=env,capture_output=True,text=True,check=False)
                self.assertEqual(0,version.returncode,version.stderr)
                source=Path(directory)/"source.md";source.write_text("hermetic real source stage")
                attempted=subprocess.run([binary,str(source),"--wait","true","--json"],env=env,capture_output=True,text=True,check=False,timeout=30)
                self.assertNotEqual(0,attempted.returncode,"unavailable provider double gate unexpectedly succeeded")
                jobs=subprocess.run([binary,"jobs","list","--json"],env=env,capture_output=True,text=True,check=False,timeout=20)
                self.assertEqual(0,jobs.returncode,jobs.stderr)
                self.assertIsNotNone(json.loads(jobs.stdout))
            return
        fixture = os.environ.get("AXON_E2E_FIXTURE_BASE_URL")
        transient = os.environ.get("AXON_E2E_TRANSIENT_SOURCE_URL")
        required_env = {
            "fixture": fixture, "transient": transient,
            "tei_failure": os.environ.get("AXON_E2E_TEI_FAILURE_SOURCE_URL"),
            "qdrant_failure": os.environ.get("AXON_E2E_QDRANT_FAILURE_SOURCE_URL"),
            "chrome": os.environ.get("AXON_E2E_CHROME_SOURCE_URL"),
            "acquire_block": os.environ.get("AXON_E2E_ACQUIRE_BLOCK_SOURCE_URL"),
            "acquire_release": os.environ.get("AXON_E2E_ACQUIRE_RELEASE_URL"),
            "embed_block": os.environ.get("AXON_E2E_EMBED_BLOCK_SOURCE_URL"),
            "embed_release": os.environ.get("AXON_E2E_EMBED_RELEASE_URL"),
            "publish_block": os.environ.get("AXON_E2E_PUBLISH_BLOCK_SOURCE_URL"),
            "publish_release": os.environ.get("AXON_E2E_PUBLISH_RELEASE_URL"),
            "publish_cleanup_failure": os.environ.get("AXON_E2E_PUBLISH_CLEANUP_FAILURE_URL"),
            "worker_crash": os.environ.get("AXON_E2E_WORKER_CRASH_SOURCE_URL"),
            "worker_crash_release": os.environ.get("AXON_E2E_WORKER_CRASH_RELEASE_URL"),
            "transport_http_block": os.environ.get("AXON_E2E_TRANSPORT_HTTP_BLOCK_SOURCE_URL"),
            "transport_mcp_block": os.environ.get("AXON_E2E_TRANSPORT_MCP_BLOCK_SOURCE_URL"),
            "transport_release": os.environ.get("AXON_E2E_TRANSPORT_RELEASE_URL"),
            "http": os.environ.get("AXON_E2E_HTTP_BASE_URL"),
            "mcporter": os.environ.get("AXON_E2E_MCPORTER_BIN"),
            "selector": os.environ.get("AXON_E2E_MCP_SELECTOR"),
            "qdrant": os.environ.get("AXON_E2E_QDRANT_URL"),
            "ssrf_redirect": os.environ.get("AXON_E2E_SSRF_REDIRECT_URL"),
            "ssrf_rebinding": os.environ.get("AXON_E2E_SSRF_REBINDING_URL"),
        }
        missing = [name for name, value in required_env.items() if not value]
        if missing:
            self.fail(f"real Axon acceptance missing required endpoints/tools: {missing}")
        completed = subprocess.run([
            sys.executable, str(ORCHESTRATOR), "--axon-bin", binary,
            "--fixture-base-url", fixture, "--transient-source-url", transient,
            "--tei-failure-source-url", required_env["tei_failure"],
            "--qdrant-failure-source-url", required_env["qdrant_failure"],
            "--chrome-source-url", required_env["chrome"],
            "--acquire-block-source-url", required_env["acquire_block"],
            "--acquire-release-url", required_env["acquire_release"],
            "--embed-block-source-url", required_env["embed_block"],
            "--embed-release-url", required_env["embed_release"],
            "--publish-block-source-url", required_env["publish_block"],
            "--publish-release-url", required_env["publish_release"],
            "--publish-cleanup-failure-url", required_env["publish_cleanup_failure"],
            "--worker-crash-source-url", required_env["worker_crash"],
            "--worker-crash-release-url", required_env["worker_crash_release"],
            "--transport-http-block-source-url", required_env["transport_http_block"],
            "--transport-mcp-block-source-url", required_env["transport_mcp_block"],
            "--transport-release-url", required_env["transport_release"],
            "--http-base-url", required_env["http"], "--mcporter", required_env["mcporter"],
            "--mcp-selector", required_env["selector"],
            "--qdrant-url", required_env["qdrant"],
            "--ssrf-redirect-url", required_env["ssrf_redirect"],
            "--ssrf-rebinding-url", required_env["ssrf_rebinding"],
            *(["--http-token", os.environ["AXON_E2E_HTTP_TOKEN"]]
              if os.environ.get("AXON_E2E_HTTP_TOKEN") else []),
        ], cwd=ROOT, capture_output=True, text=True, check=False)
        self.assertEqual(0, completed.returncode, completed.stderr)


if __name__ == "__main__": unittest.main()
