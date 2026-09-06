from __future__ import annotations

import importlib.util
import hashlib
import json
import sys
import threading
import sqlite3
from contextlib import closing
import tempfile
from unittest import mock
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from types import SimpleNamespace


ROOT = Path(__file__).resolve().parents[3]
def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path); module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module; spec.loader.exec_module(module); return module


providers = load("test_provider_adapters_module", ROOT / "scripts/e2e/lib/provider-adapters.py")
manifest_api = load("test_provider_manifest_module", ROOT / "scripts/e2e/lib/resource-manifest.py")


class Handler(BaseHTTPRequestHandler):
    state = {}; requests = []
    def log_message(self, *_args): pass
    def _serve(self):
        self.requests.append((self.command, self.path)); item = self.state.get(self.path)
        if item is None: self.send_response(404); self.end_headers(); return
        if self.command == "DELETE": del self.state[self.path]; self.send_response(204); self.end_headers(); return
        body = json.dumps(item).encode(); self.send_response(200); self.send_header("Content-Length", str(len(body)))
        self.end_headers(); self.wfile.write(body)
    do_GET = do_DELETE = _serve


class ProviderAdapterTests(unittest.TestCase):
    def setUp(self):
        Handler.state = {}; Handler.requests = []
        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.thread = threading.Thread(target=self.server.serve_forever); self.thread.start()
        self.adapter = providers.ExactHttpAdapter({"base_url": f"http://127.0.0.1:{self.server.server_port}",
            "resources": {"job": {"get": "/jobs/{identity}", "delete": "/jobs/{identity}"}}, "timeout_seconds": 1})
    def tearDown(self): self.server.shutdown(); self.thread.join(); self.server.server_close()

    def test_exact_url_encoded_identity_marker_delete_and_audit(self):
        resource = SimpleNamespace(resource_type="job", identity="job /? hostile")
        path = "/jobs/job%20%2F%3F%20hostile"; Handler.state[path] = {"ownership": {"run_id": "owned"}}
        self.assertEqual("owned", self.adapter.marker(resource)["run_id"])
        self.assertEqual("removed", self.adapter.delete(resource, float("inf"))); self.assertFalse(self.adapter.exists(resource))
        self.assertEqual([("GET", path), ("DELETE", path), ("GET", path)], Handler.requests)

    def test_missing_marker_and_provider_failure_fail_closed(self):
        resource = SimpleNamespace(resource_type="job", identity="exact")
        Handler.state["/jobs/exact"] = {"not_ownership": {}}
        self.assertIsNone(self.adapter.marker(resource))
        self.server.shutdown(); self.thread.join()
        with self.assertRaisesRegex(providers.ProviderError, "state is unknown"):
            self.adapter.exists(resource)

    def test_configuration_rejects_non_exact_endpoint(self):
        adapter = providers.ExactHttpAdapter({"base_url": "http://127.0.0.1", "resources": {"job": {"get": "/jobs"}}})
        with self.assertRaisesRegex(providers.ProviderError, "exact get"):
            adapter.marker(SimpleNamespace(resource_type="job", identity="job_1"))

    def test_qdrant_uses_standard_alias_api_and_manifest_marker(self):
        calls = []; present = {"value": True}
        adapter = providers.QdrantAdapter({"base_url": "http://127.0.0.1:6333","tenant_enforced":True,"owned_prefix":"axon_e2e_"})
        marker = {"run_id": "axon_e2e_owned"}
        expected = {"run_id": "axon_e2e_owned"}
        alias_state = {"alias_name": "axon_e2e_owned_alias"}
        digest = hashlib.sha256(json.dumps(alias_state, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
        marker = {**expected, "generation": "gen-1", "provider_state_sha256": digest}
        adapter.bind(SimpleNamespace(), SimpleNamespace(provider_marker=lambda _h, _r: expected,
            qdrant_ownership_point=lambda _h, _r: {"id": "marker-1"}))
        def request(url, method, payload=None):
            calls.append((url, method, payload))
            if url.endswith("/collections/aliases"):
                present["value"] = False; return 200, {"result": True}
            if url.endswith("/collections/owned/points"):
                return 200, {"result": [{"payload": {"axon_e2e_ownership": marker}}]}
            if url.endswith("/aliases"):
                return 200, {"result": {"aliases": ([alias_state] if present["value"] else [])}}
            raise AssertionError(url)
        adapter._request = request
        resource = SimpleNamespace(resource_type="qdrant_alias", identity="axon_e2e_owned_alias",
                                   metadata={"collection": "owned", "ownership_generation": "gen-1"})
        self.assertEqual(expected, adapter.marker(resource))
        self.assertEqual("removed", adapter.delete(resource, float("inf")))
        self.assertFalse(adapter.exists(resource))
        self.assertEqual("/aliases", calls[0][0].removeprefix(adapter.base))
        alias_delete = next(call for call in calls if call[0].endswith("/collections/aliases"))
        self.assertEqual({"actions": [{"delete_alias": {"alias_name": resource.identity}}]}, alias_delete[2])
        self.assertFalse(any("axon-e2e" in url for url, _, _ in calls))

    def test_qdrant_batches_aliases_in_one_provider_delete_round_trip(self):
        adapter = providers.QdrantAdapter({"base_url": "http://qdrant","tenant_enforced":True,"owned_prefix":"axon_e2e_"}); present = {"a", "b", "c"}; calls = []
        def request(url, method, payload=None):
            calls.append((url, method, payload))
            if url.endswith("/collections/aliases"):
                for action in payload["actions"]: present.discard(action["delete_alias"]["alias_name"])
                return 200, {"result": True}
            return 200, {"result": {"aliases": [{"alias_name": name} for name in present]}}
        adapter._request = request
        resources = [SimpleNamespace(resource_type="qdrant_alias", identity=name, metadata={}) for name in ("a", "b", "c")]
        result = adapter.delete_batch(resources, float("inf"))
        self.assertEqual(3, len(result)); self.assertEqual(1, sum(url.endswith("/collections/aliases") for url, _, _ in calls))
        self.assertEqual("provider-batch", adapter.batch_capability("qdrant_alias"))
        self.assertEqual("unbatchable-provider-contract", adapter.batch_capability("qdrant_snapshot"))

    def test_qdrant_residual_audit_covers_alias_snapshot_point_and_payload_index(self):
        adapter = providers.QdrantAdapter({"base_url": "http://qdrant","tenant_enforced":True,"owned_prefix":"axon_e2e_"})
        def request(url, method, payload=None):
            if url.endswith("/aliases"): return 200, {"result": {"aliases": [{"alias_name": "alias-owned"}]}}
            if url.endswith("/snapshots"): return 200, {"result": [{"name": "snap-owned"}]}
            if url.endswith("/points"): return 200, {"result": [{"id": "point-owned"}]}
            if url.endswith("/collections/owned"): return 200, {"result": {"payload_schema": {"field-owned": {}}}}
            raise AssertionError((url, method, payload))
        adapter._request = request
        cases = (
            SimpleNamespace(resource_type="qdrant_alias", identity="alias-owned", metadata={}),
            SimpleNamespace(resource_type="qdrant_snapshot", identity="snap-owned", metadata={"collection": "owned"}),
            SimpleNamespace(resource_type="point", identity="point-owned", metadata={"collection": "owned"}),
            SimpleNamespace(resource_type="payload_index", identity="field-owned", metadata={"collection": "owned"}),
        )
        for resource in cases:
            with self.subTest(resource_type=resource.resource_type): self.assertTrue(adapter.exists(resource))

    def test_qdrant_setup_actually_upserts_and_reverifies_generation_marker(self):
        expected = {"run_id": "owned"}; state = {"config": {"params": {"vectors": {"size": 3}}}, "optimizer_status": None}
        digest = hashlib.sha256(json.dumps(state, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
        marker = {**expected, "generation": "generation-1", "provider_state_sha256": digest}; calls = []
        api = SimpleNamespace(qdrant_ownership_point=lambda _h, _r: {"id": "marker-id", "vector": {},
                              "payload": {"axon_e2e_ownership": marker}},
                              provider_marker=lambda _h, _r: expected,
                              verify_marker=lambda _h, _r, value: self.assertEqual(expected, value))
        adapter = providers.QdrantAdapter({"base_url": "http://qdrant","tenant_enforced":True,"owned_prefix":"axon_e2e_"}).bind(SimpleNamespace(), api)
        def request(url, method, payload=None):
            calls.append((url, method, payload))
            if method == "GET": return 200, {"result": {"config": {"params": {"vectors": {"size": 3}}}}}
            if method == "PUT": return 200, {"result": {"status": "completed"}}
            return 200, {"result": [{"payload": {"axon_e2e_ownership": marker}}]}
        adapter._request = request
        resource = SimpleNamespace(resource_type="collection", identity="axon_e2e_owned",
                                   metadata={"ownership_generation": "generation-1"})
        result = adapter.provision_ownership_marker(resource)
        put = next(call for call in calls if call[1] == "PUT")
        self.assertEqual([0.0, 0.0, 0.0], put[2]["points"][0]["vector"])
        self.assertEqual("marker-id", result["point_id"])

    def test_durable_state_adapter_deletes_and_audits_sqlite_and_files(self):
        with tempfile.TemporaryDirectory() as temp:
            data = Path(temp) / "data"; data.mkdir(); db_path = data / "jobs.db"
            with closing(sqlite3.connect(db_path)) as db, db:
                db.execute("CREATE TABLE provider_reservations (reservation_id TEXT PRIMARY KEY, status TEXT, granted_units INTEGER)")
                db.execute("INSERT INTO provider_reservations VALUES ('r1','active',1)")
            header = SimpleNamespace(data_dir=data); api = SimpleNamespace(provider_marker=lambda _h, _r: {"run_id": "owned"})
            adapter = providers.DurableStateAdapter(header, api)
            reservation = SimpleNamespace(resource_type="provider_reservation", identity="r1", metadata={})
            self.assertEqual("owned", adapter.marker(reservation)["run_id"])
            self.assertEqual("removed", adapter.delete(reservation, float("inf"))); self.assertFalse(adapter.exists(reservation))
            evidence = data / "evidence.json"; evidence.write_text("{}")
            resource = SimpleNamespace(resource_type="evidence", identity="ev", metadata={"state_file": str(evidence)})
            self.assertEqual("removed", adapter.delete(resource, float("inf"))); self.assertFalse(adapter.exists(resource))

    def test_durable_state_full_persistent_taxonomy_uses_exact_table_selectors(self):
        with tempfile.TemporaryDirectory() as temp:
            data = Path(temp) / "data"; data.mkdir(); db_path = data / "jobs.db"
            resources = []
            with closing(sqlite3.connect(db_path)) as db, db:
                created = set()
                for index, (kind, (table, columns)) in enumerate(providers.DurableStateAdapter.TABLES.items()):
                    if kind == "provider_reservation": continue
                    if table not in created:
                        db.execute(f"CREATE TABLE {table} ({', '.join(column + ' TEXT' for column in columns)})")
                        created.add(table)
                    values = tuple(f"{kind}-{column}-{index}" for column in columns)
                    db.execute(f"INSERT INTO {table} ({','.join(columns)}) VALUES ({','.join('?' for _ in columns)})", values)
                    metadata = {} if len(columns) == 1 else {"db_key": dict(zip(columns, values))}
                    resources.append(SimpleNamespace(resource_type=kind, identity=values[0], metadata=metadata))
            adapter = providers.DurableStateAdapter(SimpleNamespace(data_dir=data),
                SimpleNamespace(provider_marker=lambda _h, _r: {"run_id": "owned"}))
            for resource in resources:
                with self.subTest(resource_type=resource.resource_type):
                    self.assertTrue(adapter.exists(resource)); self.assertEqual("removed", adapter.delete(resource, float("inf")))
                    self.assertFalse(adapter.exists(resource))

    def test_docker_volume_and_compose_use_exact_owned_argv_only(self):
        resource = SimpleNamespace(resource_type="volume", identity="axon_e2e_owned_volume", metadata={})
        docker = providers.ArgvAdapter({"binary": "docker", "resources": {"volume": {
            "inspect": ["volume", "inspect", "{identity}"], "delete": ["volume", "rm", "{identity}"]}}})
        completed = lambda argv, code=0, out="": __import__("subprocess").CompletedProcess(argv, code, out, "")
        with mock.patch.object(providers.subprocess, "run", side_effect=[
            completed([], out='[{"Config":{"Labels":{"axon.e2e.ownership":"{}"}}}]'), completed([]), completed([], 1)]
        ) as run:
            docker.marker(resource); self.assertEqual("removed", docker.delete(resource, float("inf")))
            self.assertEqual(["docker", "volume", "rm", resource.identity], run.call_args_list[1].args[0])
            self.assertFalse(any("shared-production-volume" in arg for call in run.call_args_list for arg in call.args[0]))
        compose = providers.DockerComposeAdapter({"binary": "docker"}, SimpleNamespace(),
                                                  SimpleNamespace(provider_marker=lambda _h, _r: {}))
        project = SimpleNamespace(identity="axon_e2e_owned_project")
        with mock.patch.object(providers.subprocess, "run", side_effect=[completed([], out='{"Name":"c"}'), completed([])]) as run:
            self.assertEqual("removed", compose.delete(project, float("inf")))
            self.assertEqual(["docker", "compose", "-p", project.identity, "down", "--remove-orphans", "--volumes"],
                             run.call_args_list[1].args[0])

    def test_docker_real_label_shapes_and_creation_generation(self):
        expected = {"run_id": "owned"}; api = SimpleNamespace(provider_marker=lambda _h, _r: expected,
            verify_marker=lambda _h, _r, marker: self.assertEqual(expected, marker))
        config = {"binary": "docker", "resources": {kind: {
            "inspect": [kind, "inspect", "{identity}"], "delete": [kind, "rm", "{identity}"]}
            for kind in ("container", "network", "volume")}}
        adapter = providers.DockerAdapter(config, SimpleNamespace(), api)
        generation = "a" * 64
        for kind in ("container", "network", "volume"):
            metadata = {"ownership_generation": generation, **({"image": "fixture@sha256:abc"} if kind == "container" else {})}
            resource = SimpleNamespace(resource_type=kind, identity=f"axon_e2e_owned_{kind}", metadata=metadata)
            encoded = json.dumps({**expected, "generation": generation})
            body = {"Config": {"Labels": {"axon.e2e.ownership": encoded}}} if kind == "container" \
                else {"Labels": {"axon.e2e.ownership": encoded}}
            done = __import__("subprocess").CompletedProcess([], 0, "created", "")
            inspected = __import__("subprocess").CompletedProcess([], 0, json.dumps([body]), "")
            with mock.patch.object(providers.subprocess, "run", side_effect=[done, inspected]) as run:
                result = adapter.provision_ownership(resource)
                self.assertEqual(generation, result["generation"])
                self.assertTrue(any(arg.startswith("axon.e2e.ownership=") for arg in run.call_args_list[0].args[0]))

    def test_compose_and_watch_ledgers_reject_recycled_provider_identity(self):
        with tempfile.TemporaryDirectory() as temp:
            allocation = manifest_api.isolation.allocate(Path(temp) / "runs", Path(temp) / "manifests")
            manifest = manifest_api.isolation.Manifest.open(Path(allocation["manifest"])); run_id = allocation["run_id"]
            manifest.register("watch", f"{run_id}_watch", {"ownership_generation": "b" * 64})
            manifest.register("upload", f"{run_id}_upload", {"ownership_generation": "c" * 64})
            header, resources = manifest_api.load(Path(allocation["manifest"])); compose = next(r for r in resources if r.resource_type == "compose_project")
            watch = next(r for r in resources if r.resource_type == "watch")
            upload = next(r for r in resources if r.resource_type == "upload")
            good = __import__("subprocess").CompletedProcess([], 0, json.dumps({"ID": "provider-generation-1", "Name": "service", "Project": compose.identity, "Status": "running"}), "")
            changed = __import__("subprocess").CompletedProcess([], 0, json.dumps({"ID": "provider-generation-1", "Name": "service", "Project": compose.identity, "Status": "exited"}), "")
            recycled = __import__("subprocess").CompletedProcess([], 0, json.dumps({"ID": "provider-generation-2", "Name": "service", "Project": compose.identity, "Status": "running"}), "")
            compose_adapter = providers.DockerComposeAdapter({"binary": "docker"}, header, manifest_api)
            compose_adapter._run = lambda _r, _op: good
            compose_adapter.provision_ownership(compose); self.assertIsNotNone(compose_adapter.marker(compose))
            compose_adapter._run = lambda _r, _op: changed
            self.assertIsNotNone(compose_adapter.marker(compose))
            compose_adapter._run = lambda _r, _op: recycled
            with self.assertRaisesRegex(Exception, "recycled or mutated"): compose_adapter.marker(compose)
            watch_good = __import__("subprocess").CompletedProcess([], 0, json.dumps({"id": "provider-generation-1", "status": "active"}), "")
            watch_changed = __import__("subprocess").CompletedProcess([], 0, json.dumps({"id": "provider-generation-1", "status": "paused"}), "")
            watch_recycled = __import__("subprocess").CompletedProcess([], 0, json.dumps({"id": "provider-generation-2", "status": "active"}), "")
            argv = providers.ManifestBoundArgvAdapter({"binary": "axon", "identity_fields": {"watch": ["id"]}, "resources": {"watch": {
                "inspect": ["--json", "watch", "get", "{identity}"], "delete": ["--json", "watch", "delete", "{identity}"]}}},
                header, manifest_api)
            argv._run = lambda _r, _op: watch_good
            argv.provision_ownership(watch); self.assertIsNotNone(argv.marker(watch))
            argv._run = lambda _r, _op: watch_changed
            self.assertIsNotNone(argv.marker(watch))
            argv._run = lambda _r, _op: watch_recycled
            with self.assertRaisesRegex(Exception, "recycled or mutated"): argv.marker(watch)
            upload_argv = providers.ManifestBoundArgvAdapter({"binary": "axon", "identity_fields": {"upload": ["id"]}, "resources": {"upload": {
                "inspect": ["--json", "uploads", "get", "{identity}"], "delete": ["--json", "uploads", "abort", "{identity}"]}}},
                header, manifest_api)
            upload_argv._run = lambda _r, _op: watch_good
            upload_argv.provision_ownership(upload); self.assertIsNotNone(upload_argv.marker(upload))
            upload_argv._run = lambda _r, _op: watch_changed
            self.assertIsNotNone(upload_argv.marker(upload))
            upload_argv._run = lambda _r, _op: watch_recycled
            with self.assertRaisesRegex(Exception, "recycled or mutated"): upload_argv.marker(upload)

    def test_tailscale_logout_is_scoped_to_owned_state_file(self):
        with tempfile.TemporaryDirectory() as temp:
            data = Path(temp) / "data"; data.mkdir(); state = Path(temp) / "tailscaled.state"
            state.write_text(json.dumps({"ownership": {"run_id": "owned"}}))
            adapter = providers.TailscaleAdapter({"binary": "tailscale"}, SimpleNamespace(data_dir=data.resolve()), SimpleNamespace())
            socket = Path(temp) / "tailscaled.sock"
            resource = SimpleNamespace(identity="axon_e2e_owned_node", metadata={"state_file": str(state), "socket": str(socket)})
            done = __import__("subprocess").CompletedProcess([], 0, "", "")
            with mock.patch.object(providers.subprocess, "run", return_value=done) as run:
                self.assertEqual("removed", adapter.delete(resource, float("inf")))
                self.assertEqual(["tailscale", "--socket", str(socket.resolve()), "logout"], run.call_args.args[0])
            self.assertFalse(state.exists())


if __name__ == "__main__": unittest.main()
