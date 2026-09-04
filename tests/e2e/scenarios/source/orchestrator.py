#!/usr/bin/env python3
"""Real-Axon source/job E2E orchestrator.

Acceptance always comes from a supplied Axon executable's public JSON. This
module has no in-memory product model and never substitutes a fake provider or
fabricated ledger/vector state when preflight fails.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import datetime as dt
import importlib.util
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[4]
ISOLATION_PATH = ROOT / "scripts/e2e/lib/run-isolation.py"
SPEC = importlib.util.spec_from_file_location("axon_e2e_source_isolation", ISOLATION_PATH)
isolation = importlib.util.module_from_spec(SPEC)
assert SPEC and SPEC.loader
SPEC.loader.exec_module(isolation)
TERMINAL = {"completed", "completed_degraded", "failed", "canceled", "cancelled", "expired", "skipped"}


class AcceptanceError(RuntimeError):
    pass


class AxonProcess:
    def __init__(self, binary: Path, allocation: dict[str, str], timeout: int = 60):
        self.binary = binary.resolve()
        self.allocation = allocation
        self.timeout = timeout
        self.env = {**os.environ, "AXON_DATA_DIR": allocation["data_dir"]}
        self.calls: list[list[str]] = []

    def call(self, *args: str, ok: bool = True) -> dict[str, Any]:
        argv = [str(self.binary), *map(str, args)]
        self.calls.append(argv[1:])
        completed = subprocess.run(
            argv, cwd=ROOT, env=self.env, capture_output=True, text=True,
            timeout=self.timeout, check=False,
        )
        if ok and completed.returncode:
            raise AcceptanceError(f"Axon command failed ({completed.returncode}): {' '.join(argv[1:])}\n{completed.stderr}")
        try:
            value = json.loads(completed.stdout)
        except json.JSONDecodeError as error:
            raise AcceptanceError(f"Axon command did not return JSON: {' '.join(argv[1:])}") from error
        if not isinstance(value, dict):
            raise AcceptanceError("Axon public response must be a JSON object")
        value["_returncode"] = completed.returncode
        return value

    def call_nullable(self, *args: str) -> dict[str, Any] | None:
        argv = [str(self.binary), *map(str, args)]; self.calls.append(argv[1:])
        completed = subprocess.run(argv, cwd=ROOT, env=self.env, capture_output=True, text=True,
                                   timeout=self.timeout, check=False)
        if completed.returncode: raise AcceptanceError(f"nullable Axon command failed: {completed.stderr}")
        value = json.loads(completed.stdout)
        if value is not None and not isinstance(value, dict):
            raise AcceptanceError("nullable Axon response was neither object nor null")
        return value

    def call_negative(self, *args: str) -> dict[str, Any]:
        """Require failure and recover its structured JSON from either stream."""
        argv = [str(self.binary), *map(str, args)]; self.calls.append(argv[1:])
        completed = subprocess.run(argv, cwd=ROOT, env=self.env, capture_output=True, text=True,
                                   timeout=self.timeout, check=False)
        if completed.returncode == 0:
            raise AcceptanceError("negative Axon command unexpectedly succeeded")
        for stream in (completed.stdout, completed.stderr):
            for line in reversed(stream.splitlines()):
                try: value = json.loads(line)
                except json.JSONDecodeError: continue
                if isinstance(value, dict) and any(key in value for key in ("error", "code", "message")):
                    value["_returncode"] = completed.returncode
                    return value
        raise AcceptanceError("negative Axon command lacked structured JSON on stdout/stderr")

    def preflight(self) -> dict[str, Any]:
        if not self.binary.is_file() or not os.access(self.binary, os.X_OK):
            raise AcceptanceError(f"--axon-bin is not executable: {self.binary}")
        doctor = self.call("doctor", "--json")
        if doctor.get("all_ok") is not True:
            raise AcceptanceError(f"Axon provider preflight failed: {doctor}")
        return doctor

    def restarted(self) -> "AxonProcess":
        """Return a fresh OS-process client over the same durable data directory."""
        return AxonProcess(self.binary, self.allocation, self.timeout)


class HttpJobsClient:
    def __init__(self, base_url: str, token: str | None = None, timeout: int = 30):
        self.base_url = base_url.rstrip("/")
        self.token = token
        self.timeout = timeout

    def request(self, method: str, path: str, body: dict[str, Any] | None = None) -> dict[str, Any]:
        headers = {"Accept": "application/json"}
        if self.token:
            headers["Authorization"] = f"Bearer {self.token}"
        payload = None
        if body is not None:
            payload = json.dumps(body).encode()
            headers["Content-Type"] = "application/json"
        request = urllib.request.Request(self.base_url + path, data=payload, headers=headers, method=method)
        try:
            with urllib.request.urlopen(request, timeout=self.timeout) as response:
                result = json.load(response)
        except (urllib.error.URLError, json.JSONDecodeError) as error:
            raise AcceptanceError(f"HTTP {method} {path} failed: {error}") from error
        if not isinstance(result, dict):
            raise AcceptanceError(f"HTTP {method} {path} did not return an object")
        return result

    def rejected(self, method: str, path: str, body: dict[str, Any] | None = None) -> dict[str, Any]:
        headers = {"Accept": "application/json", "Content-Type": "application/json"}
        if self.token: headers["Authorization"] = f"Bearer {self.token}"
        data = json.dumps(body).encode() if body is not None else None
        request = urllib.request.Request(self.base_url + path, data=data,
                                         headers=headers, method=method)
        try: urllib.request.urlopen(request, timeout=self.timeout)
        except urllib.error.HTTPError as error:
            try:
                value = json.loads(error.read())
                if error.code < 400 or not isinstance(value, dict):
                    raise AcceptanceError("HTTP negative lacked structured error")
                return value
            finally:error.close()
        raise AcceptanceError("HTTP negative unexpectedly succeeded")


class QdrantEvidenceClient(HttpJobsClient):
    def snapshot(self, collection: str, source_id: str) -> dict[str, Any]:
        points: list[dict[str, Any]] = []; offset: Any = None
        while True:
            body: dict[str, Any] = {"limit": 256, "with_payload": True, "with_vector": False,
                                    "filter": {"must": [{"key": "source_id", "match": {"value": source_id}}]}}
            if offset is not None: body["offset"] = offset
            envelope = self.request("POST", f"/collections/{collection}/points/scroll", body)
            result = envelope.get("result")
            if not isinstance(result, dict) or not isinstance(result.get("points"), list):
                raise AcceptanceError("Qdrant scroll omitted result.points")
            points.extend(result["points"]); offset = result.get("next_page_offset")
            if offset is None: break
        ids = [str(point.get("id")) for point in points]
        if len(ids) != len(set(ids)): raise AcceptanceError("Qdrant returned duplicate point IDs")
        lineage = []
        for point in points:
            payload = point.get("payload") if isinstance(point, dict) else None
            if not isinstance(payload, dict): raise AcceptanceError("Qdrant point omitted payload")
            required = ("source_id", "source_canonical_uri", "source_item_key",
                        "item_canonical_uri", "source_generation", "document_id", "chunk_text")
            if any(payload.get(key) in (None, "") for key in required):
                raise AcceptanceError(f"Qdrant point omitted lineage fields: {required}")
            if payload["source_id"] != source_id:
                raise AcceptanceError("filtered Qdrant result carried a foreign source_id")
            lineage.append({key: payload[key] for key in required})
        generations = {str(point.get("payload", {}).get("source_generation")) for point in points
                       if isinstance(point, dict) and point.get("payload", {}).get("source_generation") is not None}
        fetch_methods = {str(point.get("payload", {}).get("web_fetch_method")) for point in points
                         if isinstance(point, dict) and point.get("payload", {}).get("web_fetch_method") is not None}
        return {"point_ids": ids, "generations": sorted(generations), "fetch_methods": sorted(fetch_methods),
                "lineage": sorted(lineage, key=lambda item: (str(item["document_id"]), str(item["source_item_key"]))),
                "count": len(points)}


class McpJobsClient:
    """Invoke the real MCP server through mcporter without shell interpolation."""
    def __init__(self, mcporter: Path, selector: str, timeout: int = 30):
        self.mcporter = mcporter.resolve(); self.selector = selector; self.timeout = timeout

    def call(self, arguments: dict[str, Any]) -> dict[str, Any]:
        argv = [str(self.mcporter), "call", self.selector, "--args",
                json.dumps(arguments, separators=(",", ":")), "--output", "json"]
        completed = subprocess.run(argv, cwd=ROOT, capture_output=True, text=True,
                                   timeout=self.timeout, check=False)
        if completed.returncode:
            raise AcceptanceError(f"MCP call failed ({completed.returncode}): {completed.stderr}")
        try: envelope = json.loads(completed.stdout)
        except json.JSONDecodeError as error: raise AcceptanceError("MCP response was not JSON") from error
        if not isinstance(envelope, dict): raise AcceptanceError("MCP response was not an object")
        return self.decode_content(envelope)

    def rejected(self, arguments: dict[str, Any]) -> dict[str, Any]:
        argv = [str(self.mcporter), "call", self.selector, "--args",
                json.dumps(arguments, separators=(",", ":")), "--output", "json"]
        completed = subprocess.run(argv, cwd=ROOT, capture_output=True, text=True,
                                   timeout=self.timeout, check=False)
        if completed.returncode == 0: raise AcceptanceError("MCP negative unexpectedly succeeded")
        for text in (completed.stdout, completed.stderr):
            try: value = json.loads(text)
            except json.JSONDecodeError: continue
            if isinstance(value, dict): return value
        raise AcceptanceError("MCP negative lacked structured JSON error")

    @classmethod
    def decode_content(cls, envelope: dict[str, Any]) -> dict[str, Any]:
        """Decode rmcp content[].text JSON until the production DTO is reached."""
        value: Any = envelope
        for _ in range(8):
            if isinstance(value, dict) and isinstance(value.get("content"), list):
                texts = [item.get("text") for item in value["content"]
                         if isinstance(item, dict) and isinstance(item.get("text"), str)]
                if len(texts) != 1: raise AcceptanceError("MCP response did not contain one JSON text payload")
                try: value = json.loads(texts[0])
                except json.JSONDecodeError as error: raise AcceptanceError("MCP content text was not JSON") from error
                continue
            if isinstance(value, dict) and isinstance(value.get("result"), dict):
                value = value["result"]; continue
            if isinstance(value, dict) and isinstance(value.get("data"), dict):
                value = value["data"]; continue
            if isinstance(value, dict) and isinstance(value.get("inline"), dict):
                value = value["inline"]; continue
            break
        if not isinstance(value, dict): raise AcceptanceError("decoded MCP payload was not an object")
        return value


class SourceJobAcceptance:
    TRANSPORT_APPLICABILITY = {
        "get": {"rest": "/v1/jobs/{id}", "mcp": "jobs.get"},
        "events": {"rest": "/v1/jobs/{id}/events", "mcp": "jobs.events"},
        "stream": {"rest": "/v1/jobs/{id}/stream", "mcp": "jobs.stream"},
        "cancel": {"rest": "/v1/jobs/{id}/cancel", "mcp": "jobs.cancel"},
        "retry": {"rest": "/v1/jobs/{id}/retry", "mcp": "jobs.retry"},
        "recover": {"rest": "/v1/jobs/recover", "mcp": "jobs.recover"},
        # REST exposes artifacts directly. The generated MCP jobs subaction
        # enum intentionally does not; MCP callers use the protocol task result.
        "artifacts": {"rest": "/v1/jobs/{id}/artifacts", "mcp": "tasks/result"},
    }
    def __init__(self, client: AxonProcess):
        self.client = client
        self.manifest = isolation.Manifest.open(Path(client.allocation["manifest"]))
        self.run_id = client.allocation["run_id"]
        self.collection = client.allocation["namespace"]
        self.returned_resources: set[tuple[str, str]] = set()

    @classmethod
    def create(cls, binary: Path, base: Path, timeout: int = 60) -> "SourceJobAcceptance":
        allocation = isolation.allocate(base / "runs", base / "manifests")
        instance = cls(AxonProcess(binary, allocation, timeout))
        instance.client.preflight()
        return instance

    def _operation(self, scenario: str) -> str:
        identity = f"{self.run_id}_{scenario.replace('.', '_')}"
        self.manifest.register("operation", identity, {"run_id": self.run_id, "scenario_id": scenario})
        return identity

    def _register(self, envelope: dict[str, Any], scenario: str, operation: str) -> None:
        metadata = {"run_id": self.run_id, "scenario_id": scenario}
        for kind, key in (("job", "job_id"), ("source", "source_id")):
            identity = envelope.get(key)
            if isinstance(identity, str) and identity:
                self.manifest.register(kind, identity, metadata)
        collection = envelope.get("collection")
        if collection and collection != self.collection:
            raise AcceptanceError(f"Axon returned foreign collection {collection!r}")
        artifacts: list[Any] = []
        for key in ("items", "artifacts"):
            if isinstance(envelope.get(key), list):
                artifacts.extend(envelope[key])
        for artifact in artifacts:
            artifact_id = artifact.get("artifact_id") if isinstance(artifact, dict) else None
            if artifact_id:
                self.manifest.register("artifact", artifact_id, {
                    **metadata, "attempt": 1, "request_id": scenario, "origin": "server_response",
                    "parent_resource_type": "operation", "parent_identity": operation,
                })
        # Public transports may nest IDs below result/data. Recursively capture
        # every returned resource identity; cleanup remains centralized in .15.
        def walk(value: Any) -> None:
            if isinstance(value, dict):
                for key, item in value.items():
                    kind = {"job_id": "job", "source_id": "source", "artifact_id": "artifact",
                            "taskId": "mcp_session", "task_id": "mcp_session",
                            "provider_reservation_id": "provider_reservation",
                            "reservation_id": "provider_reservation", "debt_id": "cleanup_debt"}.get(key)
                    if kind and isinstance(item, str) and item:
                        self.returned_resources.add((kind, item))
                        extra = metadata
                        if kind in {"artifact", "mcp_session"}:
                            extra = {**metadata, "attempt": 1, "request_id": scenario,
                                     "origin": "server_response", "parent_resource_type": "operation",
                                     "parent_identity": operation}
                        self.manifest.register(kind, item, extra)
                    walk(item)
                    if key == "cleanup_debt_ids" and isinstance(item, list):
                        for debt_id in item:
                            if isinstance(debt_id, str) and debt_id:
                                self.returned_resources.add(("cleanup_debt", debt_id))
                                self.manifest.register("cleanup_debt", debt_id, metadata)
            elif isinstance(value, list):
                for item in value: walk(item)
        walk(envelope)

    @staticmethod
    def _require_source_success(result: dict[str, Any]) -> None:
        if result.get("status") not in {"completed", "completed_degraded"}:
            raise AcceptanceError(f"source did not complete: {result}")
        for key in ("job_id", "source_id"):
            if not isinstance(result.get(key), str) or not result[key]:
                raise AcceptanceError(f"source response omitted {key}")
        counts = result.get("counts", {})
        if int(counts.get("documents_total", 0)) < 1:
            raise AcceptanceError("source response did not prove document preparation")

    def source(self, source: str, scope: str, *, wait: bool = True) -> dict[str, Any]:
        scenario = f"source.{scope}.{'inline' if wait else 'detached'}"
        operation = self._operation(scenario)
        result = self.client.call(
            "source", source, "--scope", scope, "--wait", str(wait).lower(),
            "--collection", self.collection, "--json",
        )
        self._register(result, scenario, operation)
        if wait:
            self._require_source_success(result)
        elif result.get("status") not in {"accepted", "queued", "pending", "running"}:
            raise AcceptanceError(f"detached source was not accepted: {result}")
        return result

    def get(self, job_id: str) -> dict[str, Any]:
        result = self.client.call("jobs", "get", job_id, "--json")
        if result.get("job_id") != job_id or not isinstance(result.get("status"), str):
            raise AcceptanceError("jobs get did not preserve identity/status")
        return result

    def wait(self, job_id: str, timeout: float = 30) -> dict[str, Any]:
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            result = self.get(job_id)
            if result["status"] in TERMINAL:
                return result
            time.sleep(0.05)
        raise AcceptanceError(f"job {job_id} did not reach terminal state")

    def assert_observable(self, result: dict[str, Any], source: str,
                          qdrant: QdrantEvidenceClient) -> dict[str, Any]:
        job_id = result["job_id"]
        detail = self.get(job_id)
        counts = detail.get("counts", {})
        if int(counts.get("documents_done", 0)) < 1 or int(counts.get("chunks_done", 0)) < 1:
            raise AcceptanceError("job detail did not expose completed document/chunk counts")
        events = self.client.call("jobs", "events", job_id, "--after-sequence", "0", "--limit", "100", "--json")
        streamed = self.client.call("jobs", "stream", job_id, "--after-sequence", "0", "--limit", "100", "--json")
        if not events.get("events") or not streamed.get("events"):
            raise AcceptanceError("job progress/events were not publicly observable")
        artifacts = self.client.call("artifacts", "list", "--job-id", job_id, "--json")
        operation = self._operation("source.artifacts")
        self._register(artifacts, "source.artifacts", operation)
        artifact_items = artifacts.get("items", [])
        if not artifact_items:
            raise AcceptanceError("source produced no public artifacts")
        if any(not isinstance(item, dict) or item.get("job_id") != job_id or not item.get("artifact_id")
               for item in artifact_items):
            raise AcceptanceError("artifact omitted mandatory exact job_id/artifact_id provenance")
        graph = self.client.call("graph", "query", source, "--limit", "10", "--json")
        if not graph.get("nodes") or result["source_id"] not in json.dumps(graph):
            raise AcceptanceError("graph omitted exact source linkage")
        query = self.client.call("query", "Atlas", "--collection", self.collection, "--json")
        retrieve = self.client.call("retrieve", source, "--collection", self.collection, "--json")
        if not self._has_results(query) or not self._has_results(retrieve):
            raise AcceptanceError("post-ingestion query/retrieve did not return indexed content")
        snapshot = qdrant.snapshot(self.collection, result["source_id"])
        generation = result.get("ledger", {}).get("generation")
        if not snapshot["count"] or snapshot["generations"] != [str(generation)]:
            raise AcceptanceError("published points diverged from exact committed generation")
        canonical = result.get("canonical_uri")
        if any(item["source_id"] != result["source_id"] or
               item["source_canonical_uri"] != canonical or not item["chunk_text"]
               for item in snapshot["lineage"]):
            raise AcceptanceError("retrieval lineage included foreign identity or empty text")
        evidence = json.dumps({"query": query, "retrieve": retrieve})
        if result["source_id"] not in evidence and canonical not in evidence:
            raise AcceptanceError("retrieval omitted exact source identity/canonical URI")
        foreign = {value for value in self._values({"query": query, "retrieve": retrieve}, "source_id")
                   if isinstance(value, str) and value != result["source_id"]}
        if foreign:
            raise AcceptanceError(f"retrieval returned foreign source identities: {sorted(foreign)}")
        return {"detail": detail, "events": events, "stream": streamed, "artifacts": artifacts,
                "graph": graph, "query": query, "retrieve": retrieve, "qdrant": snapshot}

    @staticmethod
    def _has_results(value: dict[str, Any]) -> bool:
        return any(isinstance(value.get(key), list) and value[key]
                   for key in ("items", "results", "chunks", "documents", "points"))

    def public_inventory(self, source_id: str) -> dict[str, Any]:
        stats = self.client.call("stats", "--collection", self.collection, "--json")
        collection = self.client.call("collections", "get", self.collection, "--json")
        return {"source_id": source_id, "stats": stats, "collection": collection}

    def refresh(self, stable_path: Path, initial: Path, unchanged: Path, changed: Path,
                qdrant: QdrantEvidenceClient | None = None) -> list[dict[str, Any]]:
        results = []
        snapshots = []
        for revision in (initial, unchanged, changed):
            shutil.copyfile(revision, stable_path)
            results.append(self.source(str(stable_path), "file"))
            if qdrant: snapshots.append(qdrant.snapshot(self.collection, results[-1]["source_id"]))
        first, same, updated = results
        if same.get("source_id") != first.get("source_id") or updated.get("source_id") != first.get("source_id"):
            raise AcceptanceError("refresh changed canonical source identity")
        if int(same.get("counts", {}).get("vector_points_total", -1)) != 0:
            raise AcceptanceError("unchanged refresh duplicated vector publication")
        if int(updated.get("counts", {}).get("vector_points_total", 0)) < 1:
            raise AcceptanceError("changed refresh did not publish a new generation")
        generations = [item.get("ledger", {}).get("generation") for item in results]
        if not generations[0] or generations[1] != generations[0] or generations[2] == generations[0]:
            raise AcceptanceError(f"refresh generation contract failed: {generations}")
        if qdrant:
            if snapshots[0] != snapshots[1]:
                raise AcceptanceError("unchanged refresh changed exact Qdrant point/generation snapshot")
            if snapshots[2]["generations"] != [str(generations[2])]:
                raise AcceptanceError("changed refresh retained duplicate old/new Qdrant generations")
        return results

    def chrome_rendered(self, source: str, qdrant: QdrantEvidenceClient) -> dict[str, Any]:
        result = self.source(source, "page")
        events = self.client.call("jobs", "events", result["job_id"], "--after-sequence", "0", "--limit", "200", "--json")
        snapshot = qdrant.snapshot(self.collection, result["source_id"])
        if "chrome_render" not in snapshot["fetch_methods"]:
            raise AcceptanceError("Qdrant payload omitted adapter-owned web_fetch_method=chrome_render")
        retrieval = self.client.call("retrieve", source, "--collection", self.collection, "--json")
        if not self._has_results(retrieval) or "AXON_E2E_JS_ONLY_CONTENT" not in json.dumps(retrieval):
            raise AcceptanceError("Chrome-rendered JS-only content was not retrievable")
        return {"source": result, "events": events, "retrieve": retrieval, "qdrant": snapshot}

    def cancel_complete_race(self, source: str, scope: str) -> dict[str, Any]:
        detached = self.source(source, scope, wait=False)
        job_id = detached["job_id"]
        with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
            cancel_future = pool.submit(
                self.client.call, "jobs", "cancel", job_id, "--reason", "e2e cancel/complete race", "--json"
            )
            get_future = pool.submit(self.get, job_id)
            cancel = cancel_future.result(); observed = get_future.result()
        terminal = self.wait(job_id)
        if terminal["status"] not in {"completed", "completed_degraded", "canceled", "cancelled"}:
            raise AcceptanceError(f"cancel/complete race ended illegally: {terminal}")
        if cancel.get("job_id") != job_id or observed.get("job_id") != job_id:
            raise AcceptanceError("cancel/complete race changed job identity")
        return terminal

    def lifecycle_negatives(self, running_job_id: str) -> None:
        unknown = "00000000-0000-0000-0000-000000000000"
        if self.client.call_nullable("jobs", "get", unknown, "--json") is not None:
            raise AcceptanceError("unknown CLI job did not return semantic JSON null")
        self.client.call_negative("jobs", "retry", running_job_id, "--mode", "same_config", "--json")
        artifacts = self.client.call("artifacts", "list", "--job-id", running_job_id, "--json")
        if artifacts.get("items"):
            raise AcceptanceError("result artifacts were visible before terminal completion")

    def provider_failure(self, source: str, expected_provider: str) -> None:
        failed = self.source(source, "page", wait=False)
        if self.wait(failed["job_id"])["status"] != "failed":
            raise AcceptanceError(f"{expected_provider} failure double did not fail")
        events = self.client.call("jobs", "events", failed["job_id"], "--after-sequence", "0", "--limit", "500", "--json")
        errors = [value for value in self._values(events, "error") if isinstance(value, dict)]
        classified = [error for error in errors
                      if error.get("provider_id") == expected_provider
                      and isinstance(error.get("code"), str)
                      and isinstance(error.get("retryable"), bool)]
        if not classified:
            raise AcceptanceError(f"{expected_provider} failure lacked structured provider_id/code/retryable")
        retry_events = [event for event in events.get("events", []) if isinstance(event, dict)
                        and event.get("event") in {"provider.retry", "source.provider_retry"}]
        if not retry_events or len(retry_events) > 3:
            raise AcceptanceError(f"{expected_provider} retry budget was not bounded")
        for event in retry_events:
            if expected_provider not in self._values(event, "provider_id"):
                raise AcceptanceError(f"{expected_provider} retry event lacked provider binding")
            if failed["job_id"] not in self._values(event, "job_id"):
                raise AcceptanceError(f"{expected_provider} retry event lacked job binding")
            attempts = self._values(event, "attempt") + self._values(event, "provider_attempt")
            delays = self._values(event, "retry_after_ms") + self._values(event, "delay_ms")
            if not any(isinstance(value, int) and value >= 1 for value in attempts):
                raise AcceptanceError(f"{expected_provider} retry event lacked attempt namespace")
            if not any(isinstance(value, int) and value >= 0 for value in delays):
                raise AcceptanceError(f"{expected_provider} retry event lacked structured delay")

    def retry_transient(self, source: str, qdrant: QdrantEvidenceClient | None = None) -> dict[str, Any]:
        failed = self.source(source, "page", wait=False)
        failed_terminal = self.wait(failed["job_id"])
        if failed_terminal["status"] != "failed":
            raise AcceptanceError("protocol double did not produce a retryable failed job")
        original_events = self.client.call("jobs", "events", failed["job_id"], "--after-sequence", "0", "--limit", "100", "--json")
        attempts = [event.get("attempt") for event in original_events.get("events", []) if isinstance(event.get("attempt"), int)]
        if attempts and max(attempts) > 3:
            raise AcceptanceError(f"provider retry budget exceeded: {max(attempts)} attempts")
        baseline = self.public_inventory(failed["source_id"])
        debt_before = set(self.client.call(
            "jobs", "cancel", failed["job_id"], "--reason", "observe retry baseline debt", "--json"
        ).get("cleanup_debt_ids", []))
        qdrant_before = qdrant.snapshot(self.collection, failed["source_id"]) if qdrant else None
        retried = self.client.call("jobs", "retry", failed["job_id"], "--mode", "same_config", "--json")
        retry_job = retried.get("retry_job", {})
        retry_id = retry_job.get("id")
        if retried.get("original_job_id") != failed["job_id"] or retry_id != failed["job_id"]:
            raise AcceptanceError("retry did not preserve canonical durable job identity")
        if retried.get("attempt") != int(failed_terminal.get("attempt", 1)) + 1:
            raise AcceptanceError("retry result did not increment durable attempt")
        self.manifest.register("job", retry_id, {"run_id": self.run_id, "scenario_id": "jobs.retry"})
        original_artifacts = self.client.call("artifacts", "list", "--job-id", failed["job_id"], "--json").get("items", [])
        original_artifact_ids = {item.get("artifact_id") for item in original_artifacts if isinstance(item, dict)}
        completed = self.wait(retry_id)
        retry_artifacts = self.client.call("artifacts", "list", "--job-id", retry_id, "--json").get("items", [])
        artifact_ids = [item.get("artifact_id") for item in retry_artifacts if isinstance(item, dict)]
        if len(artifact_ids) != len(set(artifact_ids)):
            raise AcceptanceError("retry duplicated artifact identity")
        if original_artifact_ids and set(artifact_ids) != original_artifact_ids:
            raise AcceptanceError("post-publication retry changed exact artifact identities/cardinality")
        if not original_artifact_ids and not artifact_ids:
            raise AcceptanceError("successful pre-publication retry added no expected artifact")
        after = self.public_inventory(failed["source_id"])
        qdrant_after = qdrant.snapshot(self.collection, failed["source_id"]) if qdrant else None
        before_points = self._values(baseline["collection"], "points_count")
        after_points = self._values(after["collection"], "points_count")
        if before_points and after_points and after_points[-1] < before_points[-1]:
            raise AcceptanceError("retry lost pre-existing Qdrant points")
        retry_events = self.client.call("jobs", "events", retry_id, "--after-sequence", "0", "--limit", "200", "--json")
        if retried.get("original_job_id") != failed["job_id"] or not retry_events.get("events"):
            raise AcceptanceError("retry linkage/attempt events were not publicly observable")
        if retried["attempt"] not in {event.get("attempt") for event in retry_events["events"] if isinstance(event, dict)}:
            raise AcceptanceError("retry history omitted incremented attempt")
        if qdrant_after is not None:
            if qdrant_before["count"] and qdrant_before != qdrant_after:
                raise AcceptanceError("post-publication retry changed exact Qdrant snapshot")
            if not qdrant_before["count"] and not qdrant_after["count"]:
                raise AcceptanceError("pre-publication retry produced no Qdrant points")
            if qdrant_after["count"] and len(qdrant_after["generations"]) != 1:
                raise AcceptanceError(f"retry published duplicate generations: {qdrant_after['generations']}")
        debt_after = set(self.client.call(
            "jobs", "cancel", retry_id, "--reason", "observe retry terminal debt", "--json"
        ).get("cleanup_debt_ids", []))
        if not debt_after <= debt_before or len(debt_after) > len(debt_before):
            raise AcceptanceError("retry introduced cleanup-debt identities or cardinality")
        return completed

    def cancel_at_stage(self, source: str, phase: str, control_url: str,
                        *, require_partial_effects: bool = False) -> dict[str, Any]:
        """Cancel only after the controllable provider reports the requested public stage."""
        detached = self.source(source, "page", wait=False); job_id = detached["job_id"]
        deadline = time.monotonic() + 30
        observed = False
        while time.monotonic() < deadline:
            events = self.client.call("jobs", "events", job_id, "--after-sequence", "0", "--limit", "200", "--json")
            if any(event.get("phase") == phase for event in events.get("events", []) if isinstance(event, dict)):
                observed = True; break
            time.sleep(.05)
        if not observed: raise AcceptanceError(f"provider double never exposed exact phase={phase}")
        cancel = self.client.call("jobs", "cancel", job_id, "--reason", f"e2e cancel during {phase}", "--json")
        operation = self._operation(f"jobs.cancel.{phase}"); self._register(cancel, f"jobs.cancel.{phase}", operation)
        if require_partial_effects:
            if not cancel.get("side_effects") or not cancel.get("cleanup_debt_ids"):
                raise AcceptanceError("partial publication cancel omitted side_effects/cleanup_debt_ids")
        # Release the blocking provider only after cancellation has persisted.
        with urllib.request.urlopen(control_url, timeout=5) as response:response.read()
        terminal = self.wait(job_id)
        if terminal["status"] not in {"canceled", "cancelled"}:
            raise AcceptanceError(f"{phase} cancellation did not persist: {terminal}")
        return {"terminal": terminal, "cancel": cancel}

    def cancel_after_partial_publication(self, source: str, partial_release_url: str,
                                         cleanup_failure_url: str) -> dict[str, Any]:
        detached = self.source(source, "page", wait=False)
        with urllib.request.urlopen(partial_release_url, timeout=5) as response:response.read()
        deadline = time.monotonic() + 30
        while time.monotonic() < deadline:
            events = self.client.call("jobs", "events", detached["job_id"], "--after-sequence", "0",
                                      "--limit", "300", "--json")
            partial = [event for event in events.get("events", []) if isinstance(event, dict)
                       and event.get("event") in {"source.partial_published", "vectors.partial_published"}]
            artifacts = self.client.call("artifacts", "list", "--job-id", detached["job_id"], "--json")
            if partial and artifacts.get("items"): break
            time.sleep(.05)
        else:
            raise AcceptanceError("partial-publication gate exposed no real side effect/artifact")
        cancel = self.client.call("jobs", "cancel", detached["job_id"], "--reason", "observe failed partial publication", "--json")
        with urllib.request.urlopen(cleanup_failure_url, timeout=5) as response:response.read()
        terminal = self.wait(detached["job_id"])
        if terminal["status"] not in {"canceled", "cancelled", "failed"}:
            raise AcceptanceError("partial publication cancellation did not become terminal")
        if not cancel.get("side_effects") or not cancel.get("cleanup_debt_ids"):
            raise AcceptanceError("failed partial publication omitted durable side effects/debt")
        result = {"terminal": terminal, "cancel": cancel}
        debt_ids = set(cancel["cleanup_debt_ids"])
        if len(debt_ids) != len(result["cancel"]["cleanup_debt_ids"]):
            raise AcceptanceError("partial publication returned duplicate cleanup debt IDs")
        source_id = self.get(result["terminal"]["job_id"]).get("source_id")
        if not source_id: raise AcceptanceError("canceled job omitted source_id for debt reconciliation")
        plan = self.client.call("prune", "plan", source_id, "--json")
        debt_steps = []
        def collect(value: Any) -> None:
            if isinstance(value, dict):
                if value.get("debt_id") in debt_ids: debt_steps.append(value)
                for item in value.values(): collect(item)
            elif isinstance(value, list):
                for item in value: collect(item)
        collect(plan)
        planned = {step.get("debt_id") for step in debt_steps}
        if planned != debt_ids or any(not step.get("kind") or not isinstance(step.get("selector"), dict)
                                      for step in debt_steps):
            raise AcceptanceError("prune plan omitted exact cleanup debt kind/selector binding")
        plan_id = plan.get("plan", {}).get("job_id")
        if not plan_id: raise AcceptanceError("prune plan omitted reviewed plan job_id")
        executed = self.client.call("prune", "exec", plan_id, "--confirm", "--json")
        remaining = executed.get("result", {}).get("cleanup_debt_remaining")
        if remaining != 0:
            raise AcceptanceError(f"exact cleanup debt reconciliation left {remaining!r} pending")
        post = self.client.call("jobs", "cancel", detached["job_id"], "--reason", "verify debt resolved", "--json")
        unresolved = set(post.get("cleanup_debt_ids", []))
        if debt_ids & unresolved:
            raise AcceptanceError(f"prune left exact debt IDs unresolved: {sorted(debt_ids & unresolved)}")
        events = self.client.call("jobs", "events", detached["job_id"], "--after-sequence", "0", "--limit", "500", "--json")
        resolved = {event.get("debt_id") for event in events.get("events", [])
                    if isinstance(event, dict) and event.get("event") == "cleanup.debt_resolved"}
        if not debt_ids <= resolved:
            raise AcceptanceError("public events omitted per-debt cleanup resolution")
        return {**result, "debt_ids": sorted(debt_ids), "prune": executed}

    @staticmethod
    def _values(value: Any, key: str) -> list[Any]:
        found: list[Any] = []
        if isinstance(value, dict):
            for name, item in value.items():
                if name == key: found.append(item)
                found.extend(SourceJobAcceptance._values(item, key))
        elif isinstance(value, list):
            for item in value: found.extend(SourceJobAcceptance._values(item, key))
        return found

    def recover_after_restart(self, job_id: str) -> dict[str, Any]:
        restarted = self.client.restarted()
        cutoff = dt.datetime.now(dt.timezone.utc).isoformat().replace("+00:00", "Z")
        recovery = restarted.call("jobs", "recover", "--stale-before", cutoff, "--json")
        operation = self._operation("jobs.restart_recover")
        self._register(recovery, "jobs.restart_recover", operation)
        observed = restarted.call("jobs", "get", job_id, "--json")
        if observed.get("job_id") != job_id:
            raise AcceptanceError("fresh process could not reconnect to durable job after recover")
        self.client = restarted
        return {"recovery": recovery, "job": observed}

    def worker_crash_recover(self, blocked_source: str, phase: str, release_url: str) -> dict[str, Any]:
        """Crash an actual standalone worker while a durable source job is in flight."""
        detached = self.source(blocked_source, "page", wait=False); job_id = detached["job_id"]
        worker = subprocess.Popen(
            [str(self.client.binary), "jobs", "worker", "--idle-exit-secs", "120"],
            cwd=ROOT, env=self.client.env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
            text=True, start_new_session=True,
        )
        try:
            deadline = time.monotonic() + 30
            while time.monotonic() < deadline:
                events = self.client.call("jobs", "events", job_id, "--after-sequence", "0", "--limit", "200", "--json")
                if any(item.get("phase") == phase for item in events.get("events", []) if isinstance(item, dict)):
                    break
                time.sleep(.05)
            else: raise AcceptanceError(f"worker never reached crash phase={phase}")
            worker.kill(); worker.wait(timeout=5)
            cutoff = dt.datetime.now(dt.timezone.utc).isoformat().replace("+00:00", "Z")
            restarted = self.client.restarted()
            recovery = restarted.call("jobs", "recover", "--stale-before", cutoff, "--json")
            recovered_count = sum(value for key in ("recovered", "recovered_count", "jobs_recovered")
                                  for value in [recovery.get(key)] if isinstance(value, int))
            if recovered_count < 1: raise AcceptanceError(f"fresh process recovered no stale jobs: {recovery}")
            fresh_worker = subprocess.Popen(
                [str(restarted.binary), "jobs", "worker", "--idle-exit-secs", "30"], cwd=ROOT,
                env=restarted.env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, text=True,
                start_new_session=True,
            )
            with urllib.request.urlopen(release_url, timeout=5) as response:response.read()
            self.client = restarted
            terminal = self.wait(job_id)
            fresh_worker.terminate(); fresh_worker.wait(timeout=5)
            history = restarted.call("jobs", "events", job_id, "--after-sequence", "0", "--limit", "500", "--json")
            attempts = {item.get("attempt") for item in history.get("events", [])
                        if isinstance(item, dict) and isinstance(item.get("attempt"), int)}
            if len(attempts) < 2: raise AcceptanceError("recovered job did not expose a new attempt transition")
            return {"recovery": recovery, "terminal": terminal, "events": history}
        finally:
            if worker.poll() is None: worker.kill(); worker.wait(timeout=5)

    def assert_transport_parity(self, job_id: str, http: HttpJobsClient, mcp: McpJobsClient) -> None:
        cli = self.get(job_id)
        http_value = http.request("GET", f"/v1/jobs/{job_id}")
        mcp_value = mcp.call({"action": "jobs", "subaction": "get", "job_id": job_id})
        operation = self._operation("jobs.transport_parity")
        for envelope in (http_value, mcp_value): self._register(envelope, "jobs.transport_parity", operation)
        for name, envelope in (("http", http_value), ("mcp", mcp_value)):
            ids = self._values(envelope, "job_id") + self._values(envelope, "id")
            statuses = self._values(envelope, "status")
            if job_id not in ids or cli["status"] not in statuses:
                raise AcceptanceError(f"{name} job evidence diverged from CLI identity/status")
        http_events = http.request("GET", f"/v1/jobs/{job_id}/events")
        mcp_events = mcp.call({"action": "jobs", "subaction": "events", "job_id": job_id})
        if not self._values(http_events, "sequence") or not self._values(mcp_events, "sequence"):
            raise AcceptanceError("HTTP/MCP lifecycle events were not observable")
        unknown = "00000000-0000-0000-0000-000000000000"
        http.rejected("GET", f"/v1/jobs/{unknown}")
        mcp.rejected({"action": "jobs", "subaction": "get", "job_id": unknown})

    def assert_transport_source_creation(self, source: str, http: HttpJobsClient,
                                         mcp: McpJobsClient,
                                         ssrf_sources: list[str] | None = None) -> dict[str, Any]:
        request = {"source": source, "scope": "page", "collection": self.collection}
        http_value = http.request("POST", "/v1/sources", request)
        mcp_value = mcp.call({"action": "source", **request, "detached": False,
                              "response_mode": "inline"})
        operation = self._operation("source.transport_creation")
        for envelope in (http_value, mcp_value):
            self._register(envelope, "source.transport_creation", operation)
            self._require_source_success(envelope)
        for key in ("source_id", "canonical_uri", "source_kind", "adapter", "scope", "ledger", "counts"):
            if key not in http_value or key not in mcp_value:
                raise AcceptanceError(f"source transport omitted production SourceResult field {key}")
        for key in ("source_id", "canonical_uri", "source_kind", "adapter", "scope", "status"):
            if http_value[key] != mcp_value[key]:
                raise AcceptanceError(f"HTTP/MCP SourceResult semantic mismatch for {key}")
        for key in ("ledger", "counts", "graph", "artifacts"):
            if http_value.get(key) != mcp_value.get(key):
                raise AcceptanceError(f"HTTP/MCP SourceResult deep semantic mismatch for {key}")
        if http_value["status"] not in {"completed", "completed_degraded"}:
            raise AcceptanceError("HTTP/MCP shared source lifecycle was not terminal-success")
        job_ids = {http_value["job_id"], mcp_value["job_id"]}
        for name, envelope in (("http", http_value), ("mcp", mcp_value)):
            detail = http.request("GET", f"/v1/sources/{envelope['source_id']}")
            summary = detail.get("summary", {})
            manifest = detail.get("manifest")
            documents = detail.get("documents")
            if summary.get("source_id") != envelope["source_id"] or summary.get("last_job_id") not in job_ids:
                raise AcceptanceError(f"{name} source was not committed to exact ledger detail route")
            generation = envelope.get("ledger", {}).get("committed_generation") or envelope.get("ledger", {}).get("generation")
            if detail.get("committed_generation") != generation or not isinstance(manifest, dict):
                raise AcceptanceError(f"{name} source detail omitted exact committed manifest generation")
            items = manifest.get("items")
            if manifest.get("item_count") != len(items or []) or not items:
                raise AcceptanceError(f"{name} source detail manifest/item counts diverged")
            if not isinstance(documents, list) or not documents:
                raise AcceptanceError(f"{name} source detail omitted document transition evidence")
            if any(doc.get("generation") != generation or doc.get("status") != "published"
                   or int(doc.get("chunk_count", 0)) < 1 or int(doc.get("vector_point_count", 0)) < 1
                   for doc in documents if isinstance(doc, dict)):
                raise AcceptanceError(f"{name} source document states were not committed/published")
        http_error = http.rejected("POST", "/v1/sources", {"source": ""})
        mcp_error = mcp.rejected({"action": "source", "source": "", "detached": False})
        if not any(marker in json.dumps(value).casefold() for value in (http_error, mcp_error)
                   for marker in ("required", "missing", "source")):
            raise AcceptanceError("HTTP/MCP negative source rejection lacked semantic error")
        blocked = ssrf_sources or [
            "http://169.254.169.254/latest/meta-data", "http://[::1]/",
            "http://127.0.0.1/", "http://localhost/", "http://2852039166/",
        ]
        before = http.request("GET", "/v1/jobs")
        before_ids = set(self._values(before, "job_id") + self._values(before, "id"))
        cli_before = self.client.call("jobs", "list", "--json")
        cli_before_ids = set(self._values(cli_before, "job_id") + self._values(cli_before, "id"))
        for blocked_source in blocked:
            ssrf = {"source": blocked_source, "scope": "page", "collection": self.collection}
            http.rejected("POST", "/v1/sources", ssrf)
            mcp.rejected({"action": "source", **ssrf, "detached": False})
            self.client.call_negative("source", blocked_source, "--scope", "page", "--wait", "true",
                                      "--collection", self.collection, "--json")
        after = http.request("GET", "/v1/jobs")
        after_ids = set(self._values(after, "job_id") + self._values(after, "id"))
        if before_ids != after_ids:
            raise AcceptanceError(f"SSRF rejection changed exact durable job IDs: {before_ids ^ after_ids}")
        cli_after = self.client.call("jobs", "list", "--json")
        cli_after_ids = set(self._values(cli_after, "job_id") + self._values(cli_after, "id"))
        if cli_before_ids != cli_after_ids:
            raise AcceptanceError(f"CLI SSRF rejection changed exact durable job IDs: {cli_before_ids ^ cli_after_ids}")
        return {"http": http_value, "mcp": mcp_value}

    def assert_transport_lifecycle_negatives(self, http: HttpJobsClient, mcp: McpJobsClient) -> None:
        """Exercise every production REST/MCP lifecycle mutation's fail-closed wire shape."""
        unknown = "00000000-0000-0000-0000-000000000000"
        http.rejected("POST", f"/v1/jobs/{unknown}/cancel", {"reason": "e2e unknown"})
        mcp.rejected({"action": "jobs", "subaction": "cancel", "job_id": unknown,
                      "reason": "e2e unknown"})
        http.rejected("POST", f"/v1/jobs/{unknown}/retry", {"mode": "same_config"})
        mcp.rejected({"action": "jobs", "subaction": "retry", "job_id": unknown,
                      "mode": "same_config"})
        # Recovery is collection-wide and admin-scoped on both transports. A
        # malformed timestamp proves exact DTO routing without mutating another run.
        http.rejected("POST", "/v1/jobs/recover", {"stale_before": "not-a-timestamp"})
        mcp.rejected({"action": "jobs", "subaction": "recover",
                      "stale_before": "not-a-timestamp"})

    def assert_transport_positive_lifecycle(self, http: HttpJobsClient, mcp: McpJobsClient,
                                            http_source: str, mcp_source: str,
                                            release_url: str) -> None:
        request = {"scope": "page", "collection": self.collection, "wait": False}
        http_job = http.request("POST", "/v1/sources", {"source": http_source, **request})
        mcp_job = mcp.call({"action": "source", "source": mcp_source, "scope": "page",
                            "collection": self.collection, "detached": True})
        pairs = (("http", http_job, lambda jid: http.request(
                    "POST", f"/v1/jobs/{jid}/cancel", {"reason": "e2e transport gate"})),
                 ("mcp", mcp_job, lambda jid: mcp.call(
                    {"action": "jobs", "subaction": "cancel", "job_id": jid,
                     "reason": "e2e transport gate"})))
        canceled: list[tuple[str, str]] = []
        for name, created, cancel_call in pairs:
            job_id = created.get("job_id")
            if not isinstance(job_id, str): raise AcceptanceError(f"{name} detached source omitted job_id")
            artifacts = http.request("GET", f"/v1/jobs/{job_id}/artifacts")
            if artifacts.get("items"): raise AcceptanceError(f"{name} exposed result artifacts before terminal")
            cancel = cancel_call(job_id)
            if job_id not in self._values(cancel, "job_id") + self._values(cancel, "id"):
                raise AcceptanceError(f"{name} cancel changed durable job identity")
            canceled.append((name, job_id))
        with urllib.request.urlopen(release_url, timeout=5) as response:response.read()
        for name, job_id in canceled:
            retry = (http.request("POST", f"/v1/jobs/{job_id}/retry", {"mode": "same_config"})
                     if name == "http" else mcp.call({"action": "jobs", "subaction": "retry",
                                                      "job_id": job_id, "mode": "same_config"}))
            ids = self._values(retry, "job_id") + self._values(retry, "id") + self._values(retry, "original_job_id")
            if job_id not in ids: raise AcceptanceError(f"{name} retry lost same-job linkage")
        cutoff = "1970-01-01T00:00:00Z"
        http_recover = http.request("POST", "/v1/jobs/recover", {"stale_before": cutoff})
        mcp_recover = mcp.call({"action": "jobs", "subaction": "recover", "stale_before": cutoff})
        if self._values(http_recover, "recovered") != self._values(mcp_recover, "recovered"):
            raise AcceptanceError("HTTP/MCP recovery semantics diverged")

    def verify_cleanup_registration(self) -> None:
        records = [record["payload"] for record in self.manifest.verify() if record["payload"].get("kind") == "resource"]
        registered = {(item["resource_type"], item["identity"]) for item in records}
        missing = self.returned_resources - registered
        if missing:
            raise AcceptanceError(f"unregistered returned resources: {sorted(missing)}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--axon-bin", type=Path, required=True)
    parser.add_argument("--fixture-base-url", required=True)
    parser.add_argument("--transient-source-url", required=True)
    parser.add_argument("--tei-failure-source-url", required=True)
    parser.add_argument("--qdrant-failure-source-url", required=True)
    parser.add_argument("--chrome-source-url", required=True)
    parser.add_argument("--acquire-block-source-url", required=True)
    parser.add_argument("--acquire-release-url", required=True)
    parser.add_argument("--embed-block-source-url", required=True)
    parser.add_argument("--embed-release-url", required=True)
    parser.add_argument("--publish-block-source-url", required=True)
    parser.add_argument("--publish-release-url", required=True)
    parser.add_argument("--publish-cleanup-failure-url", required=True)
    parser.add_argument("--worker-crash-source-url", required=True)
    parser.add_argument("--worker-crash-release-url", required=True)
    parser.add_argument("--transport-http-block-source-url", required=True)
    parser.add_argument("--transport-mcp-block-source-url", required=True)
    parser.add_argument("--transport-release-url", required=True)
    parser.add_argument("--http-base-url", required=True)
    parser.add_argument("--http-token")
    parser.add_argument("--mcporter", type=Path, required=True)
    parser.add_argument("--mcp-selector", required=True)
    parser.add_argument("--qdrant-url", required=True)
    parser.add_argument("--ssrf-redirect-url", required=True)
    parser.add_argument("--ssrf-rebinding-url", required=True)
    parser.add_argument("--work-root", type=Path, default=Path(tempfile.gettempdir()) / "axon-e2e-source-jobs")
    parser.add_argument("--timeout", type=int, default=120)
    args = parser.parse_args()
    try:
        acceptance = SourceJobAcceptance.create(args.axon_bin, args.work_root, args.timeout)
        qdrant = QdrantEvidenceClient(args.qdrant_url, timeout=args.timeout)
        corpus = ROOT / "tests/e2e/corpus/v1/revisions/atlas"
        stable = Path(acceptance.client.allocation["run_root"]) / "atlas.md"
        local = acceptance.source(str(corpus / "1.0.0.md"), "file")
        acceptance.assert_observable(local, str(corpus / "1.0.0.md"), qdrant)
        http = HttpJobsClient(args.http_base_url, args.http_token, args.timeout)
        mcp = McpJobsClient(args.mcporter, args.mcp_selector, args.timeout)
        acceptance.assert_transport_parity(local["job_id"], http, mcp)
        ssrf_sources = ["http://169.254.169.254/latest/meta-data", "http://[::1]/",
                        "http://127.0.0.1/", "http://localhost/", "http://2852039166/"]
        ssrf_sources.extend(value for value in (args.ssrf_redirect_url, args.ssrf_rebinding_url) if value)
        acceptance.assert_transport_source_creation(
            args.fixture_base_url.rstrip("/") + "/page?transport=source", http, mcp, ssrf_sources)
        acceptance.assert_transport_lifecycle_negatives(http, mcp)
        acceptance.assert_transport_positive_lifecycle(
            http, mcp, args.transport_http_block_source_url,
            args.transport_mcp_block_source_url, args.transport_release_url)
        directory_source = str(ROOT / "tests/e2e/corpus/v1/documents/micro")
        directory = acceptance.source(directory_source, "directory")
        acceptance.assert_observable(directory, directory_source, qdrant)
        page_source = args.fixture_base_url.rstrip("/") + "/page.html"
        page = acceptance.source(page_source, "page")
        acceptance.assert_observable(page, page_source, qdrant)
        site_source = args.fixture_base_url.rstrip("/") + "/"
        site = acceptance.source(site_source, "site")
        acceptance.assert_observable(site, site_source, qdrant)
        acceptance.chrome_rendered(args.chrome_source_url, qdrant)
        acceptance.refresh(stable, corpus / "1.0.0.md", corpus / "1.0.1-unchanged.md", corpus / "1.1.0-changed.md", qdrant)
        acceptance.cancel_complete_race(args.fixture_base_url.rstrip("/") + "/page.html?race=1", "page")
        negative = acceptance.source(args.acquire_block_source_url + "?negative=1", "page", wait=False)
        acceptance.lifecycle_negatives(negative["job_id"])
        acceptance.client.call("jobs", "cancel", negative["job_id"], "--reason", "negative cleanup", "--json")
        acceptance.cancel_at_stage(args.acquire_block_source_url, "fetching", args.acquire_release_url)
        embed_cancelled = acceptance.cancel_at_stage(args.embed_block_source_url, "embedding", args.embed_release_url)
        acceptance.cancel_after_partial_publication(
            args.publish_block_source_url, args.publish_release_url, args.publish_cleanup_failure_url)
        acceptance.recover_after_restart(embed_cancelled["terminal"]["job_id"])
        acceptance.worker_crash_recover(
            args.worker_crash_source_url, "fetching", args.worker_crash_release_url)
        acceptance.retry_transient(args.transient_source_url, qdrant)
        acceptance.provider_failure(args.tei_failure_source_url, "tei")
        acceptance.provider_failure(args.qdrant_failure_source_url, "qdrant")
        acceptance.verify_cleanup_registration()
        print(json.dumps({"status": "passed", "run_id": acceptance.run_id, "manifest": acceptance.client.allocation["manifest"]}))
        return 0
    except (AcceptanceError, isolation.IsolationError, subprocess.TimeoutExpired) as error:
        print(f"source/jobs E2E failed: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
