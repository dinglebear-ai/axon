import importlib.util
import http.server
import json
import sys
import threading
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
SPEC = importlib.util.spec_from_file_location("axon_http_adapter", ROOT / "scripts/e2e/adapters/http_adapter.py")
adapter = importlib.util.module_from_spec(SPEC)
assert SPEC.loader
sys.modules[SPEC.name] = adapter
SPEC.loader.exec_module(adapter)


class HttpAdapterTests(unittest.TestCase):
    def test_projects_every_catalog_http_scenario_without_shell(self):
        selected = adapter.scenarios()
        self.assertEqual(6, len(selected))
        for scenario in selected:
            spec = adapter.project(scenario, adapter.fixture_for(scenario), "e2e_owned", "owned_job")
            self.assertIn(spec.method, {"GET", "POST"})
            self.assertTrue(spec.path.startswith("/v1/"))
            if spec.body:
                self.assertIsInstance(json.loads(spec.body), dict)

    def test_openapi_inventory_is_authoritative_and_includes_lifecycles(self):
        routes = adapter.inventory()
        self.assertIn("POST /v1/sources", routes)
        self.assertIn("GET /v1/jobs", routes)
        self.assertIn("PUT /v1/uploads/{upload_id}/content", routes)
        self.assertIn("GET /v1/artifacts/{artifact_id}/content", routes)
        self.assertIn("POST /v1/ask/stream", routes)

    def test_inventory_reconciliation_classifies_every_operation(self):
        groups = adapter.reconcile_inventory()
        classified = {route for routes in groups.values() for route in routes}
        self.assertEqual(set(adapter.inventory()), classified)
        for required in ("jobs", "uploads", "artifacts", "behavioral_operations", "contract_only"):
            self.assertTrue(groups[required], required)

    def test_cross_origin_redirect_is_refused_before_credentials_forward(self):
        handler = adapter.SameOriginRedirects()
        request = adapter.Request("https://axon.test/v1/status",
                                  headers={"Authorization": "Bearer secret"})
        with self.assertRaisesRegex(adapter.HttpAdapterError, "cross-origin"):
            handler.redirect_request(request, None, 302, "Found", {}, "https://evil.test/grab")

    def test_live_cross_origin_redirect_never_reaches_credential_sink(self):
        seen = []
        class Sink(http.server.BaseHTTPRequestHandler):
            def do_GET(self):
                seen.append(self.headers.get("Authorization")); self.send_response(200); self.end_headers()
            def log_message(self, *_args): pass
        sink = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Sink)
        class Redirect(http.server.BaseHTTPRequestHandler):
            def do_GET(self):
                self.send_response(302); self.send_header("Location", f"http://127.0.0.1:{sink.server_port}/sink"); self.end_headers()
            def log_message(self, *_args): pass
        redirect = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Redirect)
        threads = [threading.Thread(target=server.serve_forever) for server in (sink, redirect)]
        for thread in threads: thread.start()
        try:
            with self.assertRaisesRegex(adapter.HttpAdapterError, "cross-origin"):
                adapter.request(f"http://127.0.0.1:{redirect.server_port}", "secret",
                                adapter.HttpRequest("GET", "/redirect"), 2)
            self.assertEqual([], seen)
        finally:
            redirect.shutdown(); sink.shutdown(); redirect.server_close(); sink.server_close()
            for thread in threads: thread.join()

    def test_hostile_fixture_remains_json_data(self):
        hostile = {"source": "$(touch /tmp/nope); '`\n../secret", "scope": "page", "wait": True}
        scenario = adapter.scenarios()[0]
        spec = adapter.project(scenario, hostile, "e2e_owned")
        self.assertEqual(hostile, json.loads(spec.body))

    def test_normalization_preserves_errors_and_redacts_headers(self):
        scenario = next(item for item in adapter.scenarios() if item["id"] == "jobs.cancel.negative")
        response = adapter.HttpResponse(404, {
            "Content-Type": "application/json", "Authorization": "Bearer secret"
        }, b'{"error":{"code":"not_found"}}')
        record = adapter.normalize(scenario, response, 4)
        self.assertEqual("pass", record["result"])
        self.assertEqual(404, record["status"])
        self.assertEqual({"error": {"code": "not_found"}}, record["body"])
        self.assertEqual("[REDACTED]", record["headers"]["Authorization"])

    def test_stream_parser_keeps_progress_and_final_events(self):
        events = adapter.sse_events(b'data: {"kind":"progress"}\n\ndata: {"kind":"final"}\n')
        self.assertEqual(["progress", "final"], [event["kind"] for event in events])

    def test_stream_projection_requires_an_owned_job(self):
        scenario = next(item for item in adapter.scenarios() if item["id"] == "jobs.stream.happy")
        with self.assertRaisesRegex(adapter.HttpAdapterError, "harness-owned job"):
            adapter.project(scenario, adapter.fixture_for(scenario), "e2e_owned")
        self.assertEqual("/v1/jobs/job_123/stream",
                         adapter.project(scenario, {}, "e2e_owned", "job_123").path)

    def test_job_id_extraction_rejects_path_values(self):
        response = adapter.HttpResponse(202, {"Content-Type": "application/json"},
                                        b'{"job_id":"../../foreign"}')
        self.assertIsNone(adapter.response_job_id(response))

    def test_semantic_oracles_require_lifecycle_and_digest_evidence(self):
        source = next(item for item in adapter.scenarios() if item["id"] == "source.inline.happy")
        self.assertTrue(adapter.evaluate_oracle("source.accepted", source, 202, {"job_id": "job_1"}, []))
        self.assertFalse(adapter.evaluate_oracle("source.accepted", source, 202, {}, []))
        prune = next(item for item in adapter.scenarios() if item["id"] == "prune.plan.happy")
        self.assertTrue(adapter.evaluate_oracle("prune.plan_digest_bound", prune, 200,
                                                {"plan_digest": "a" * 64}, []))
        self.assertFalse(adapter.evaluate_oracle("prune.plan_digest_bound", prune, 200, {}, []))

    def test_probe_contract_contains_auth_and_hostile_cases(self):
        probes = adapter.probe_specs()
        coverage = json.loads((ROOT / "tests/e2e/http/coverage.json").read_text())
        executable = set(probes) | {"redirect.cross_origin", "stream.progress_final",
                                    "stream.disconnect_reconnect", "auth.non_loopback_bind",
                                    *adapter.compatibility_specs()}
        self.assertEqual(set(coverage["required_probes"]), executable)
        with self.assertRaisesRegex(adapter.HttpAdapterError, "hostile header"):
            adapter.probe_headers("bad\r\nInjected: yes", "secret")

    def test_resource_registration_uses_chained_isolation_manifest(self):
        isolation = adapter.load_isolation()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            allocation = isolation.allocate(root / "runs", root / "manifests")
            identity = f"{isolation.RUN_PREFIX}http_upload"
            adapter.register_resource(Path(allocation["manifest"]), "upload", identity,
                                      {"owner": "http-adapter"})
            records = isolation.Manifest.open(Path(allocation["manifest"])).verify()
            self.assertEqual(identity, records[-1]["payload"]["identity"])

    def test_response_resources_are_registered_and_foreign_ids_rejected(self):
        isolation = adapter.load_isolation()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); allocation = isolation.allocate(root / "runs", root / "manifests")
            path = Path(allocation["manifest"])
            response = adapter.HttpResponse(201, {"Content-Type": "application/json"},
                                            b'{"upload_id":"upl_550e8400-e29b-41d4-a716-446655440000"}')
            operation = f'{allocation["namespace"]}_http_test'
            adapter.register_resource(path, "operation", operation, {"run_id": allocation["run_id"]})
            binding = {"run_id": allocation["run_id"], "attempt": 1, "scenario_id": "http.test",
                       "request_id": "request-1", "origin": "server_response",
                       "parent_resource_type": "operation", "parent_identity": operation}
            self.assertEqual([("upload", "upl_550e8400-e29b-41d4-a716-446655440000")],
                             adapter.register_response_resources(path, response, binding))
            foreign = adapter.HttpResponse(201, {}, b'{"artifact_id":"art_550e8400-e29b-41d4-a716-446655440001"}')
            with self.assertRaisesRegex(adapter.HttpAdapterError, "resource registration rejected"):
                adapter.register_response_resources(path, foreign)

    def test_disconnect_reconnect_requires_progress_continuation_and_terminal(self):
        self.assertTrue(adapter.validate_disconnect_reconnect(
            [{"kind": "progress"}], {"status": "running"}, [{"kind": "final"}]))
        self.assertFalse(adapter.validate_disconnect_reconnect(
            [], {"status": "completed"}, [{"kind": "final"}]))

    def test_rejection_and_failure_taxonomy_oracles_require_structured_code(self):
        scenario = next(item for item in adapter.scenarios() if item["id"] == "jobs.cancel.negative")
        envelope = {"error": {"code": "job.not_found", "message": "missing"}}
        self.assertTrue(adapter.evaluate_oracle("rejection.job_missing", scenario, 404, envelope, []))
        self.assertTrue(adapter.evaluate_oracle("failure.taxonomy", scenario, 404, envelope, []))
        self.assertFalse(adapter.evaluate_oracle("failure.taxonomy", scenario, 404, {"error": "missing"}, []))

    def test_non_loopback_bind_probe_executes_binary_and_requires_auth_failure(self):
        with tempfile.TemporaryDirectory() as directory:
            binary = Path(directory) / "fake-axon"
            binary.write_text("#!/bin/sh\necho 'authentication token required' >&2\nexit 2\n")
            binary.chmod(0o700)
            self.assertTrue(adapter.non_loopback_bind_probe(binary, 2)["passed"])

    def test_upload_artifact_lifecycle_issues_real_crud_sequence(self):
        from unittest import mock
        isolation = adapter.load_isolation()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); allocation = isolation.allocate(root / "runs", root / "manifests")
            responses = iter([
                adapter.HttpResponse(200, {}, b'{"upload_id":"upl_550e8400-e29b-41d4-a716-446655440000"}'),
                adapter.HttpResponse(200, {}, b'{}'), adapter.HttpResponse(200, {}, b'{}'),
                adapter.HttpResponse(200, {}, b'{}'),
                adapter.HttpResponse(200, {}, b'{"artifact_id":"art_550e8400-e29b-41d4-a716-446655440001"}'),
                adapter.HttpResponse(200, {}, b'{}'), adapter.HttpResponse(200, {}, b'x'),
                adapter.HttpResponse(200, {}, b'{}'),
                adapter.HttpResponse(200, {}, b'{"upload_id":"upl_550e8400-e29b-41d4-a716-446655440002"}'),
                adapter.HttpResponse(200, {}, b'{}'),
            ])
            calls = []
            def fake_request(_base, _token, spec, _timeout, _headers=None):
                calls.append((spec.method, spec.path)); return next(responses)
            with mock.patch.object(adapter, "request", side_effect=fake_request):
                records = adapter.upload_artifact_lifecycle(
                    "http://127.0.0.1:1", "token", Path(allocation["manifest"]),
                    allocation["namespace"], 2)
            self.assertTrue(all(record["passed"] for record in records))
            self.assertIn(("PUT", "/v1/uploads/upl_550e8400-e29b-41d4-a716-446655440000/content"), calls)
            self.assertIn(("DELETE", "/v1/uploads/upl_550e8400-e29b-41d4-a716-446655440002"), calls)
            self.assertIn(("GET", "/v1/artifacts/art_550e8400-e29b-41d4-a716-446655440001/content"), calls)

    def test_fixture_escape_is_rejected(self):
        catalog = adapter.load_catalog()
        catalog["scenarios"][0]["requests"]["http"] = "../outside.json"
        with tempfile.NamedTemporaryFile("w", suffix=".json", dir=ROOT / "tests/e2e/http") as output:
            json.dump(catalog, output)
            output.flush()
            scenario = adapter.scenarios(Path(output.name))[0]
            with self.assertRaisesRegex(adapter.HttpAdapterError, "escapes tests"):
                adapter.fixture_for(scenario)


if __name__ == "__main__":
    unittest.main()
