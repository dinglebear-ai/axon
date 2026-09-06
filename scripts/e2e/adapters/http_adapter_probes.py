"""Supplementary HTTP E2E probes kept separate from the catalog adapter.

This module deliberately receives the adapter primitives it needs as arguments.
That keeps it import-safe when ``http_adapter.py`` is loaded by file path in
the hermetic tests, while leaving request construction and resource ownership
in the main adapter.
"""

from __future__ import annotations

import json
import os
import subprocess
import tempfile
from pathlib import Path
from typing import Any
from urllib.request import Request, build_opener


def compatibility_specs(HttpRequest: Any) -> dict[str, Any]:
    return {
        "compat.health": HttpRequest("GET", "/healthz"),
        "compat.ready": HttpRequest("GET", "/readyz"),
        "compat.openapi": HttpRequest("GET", "/openapi.json"),
        "compat.status": HttpRequest("GET", "/v1/status"),
    }


def validate_disconnect_reconnect(first_events: list[dict[str, Any]],
                                  status_envelope: dict[str, Any],
                                  resumed_events: list[dict[str, Any]]) -> bool:
    first_kinds = {str(item.get("kind", "")).casefold() for item in first_events}
    resumed_kinds = {str(item.get("kind", "")).casefold() for item in resumed_events}
    state = str(status_envelope.get("status", status_envelope.get("state", ""))).casefold()
    return (bool(first_kinds & {"progress", "started", "stage"}) and
            state in {"queued", "running", "completed", "succeeded"} and
            bool(resumed_kinds & {"final", "completed", "failed", "cancelled"}))


def disconnect_reconnect_probe(base_url: str, token: str, job_id: str, timeout: float,
                               *, HttpRequest: Any, SameOriginRedirects: Any,
                               request: Any, sse_events: Any, urljoin: Any) -> dict[str, Any]:
    path = f"/v1/jobs/{job_id}/stream"
    url = urljoin(base_url.rstrip("/") + "/", path.lstrip("/"))
    req = Request(url, headers={"Authorization": f"Bearer {token}",
                                "Accept": "text/event-stream"})
    first = []
    with build_opener(SameOriginRedirects()).open(req, timeout=timeout) as response:
        for raw in response:
            if raw.startswith(b"data:"):
                first = sse_events(raw)
                break
    status = request(base_url, token, HttpRequest("GET", f"/v1/jobs/{job_id}"), timeout)
    resumed = request(base_url, token, HttpRequest("GET", path), timeout)
    try:
        status_value = json.loads(status.body)
    except json.JSONDecodeError:
        status_value = {}
    passed = validate_disconnect_reconnect(first, status_value, sse_events(resumed.body))
    return {"id": "stream.disconnect_reconnect", "passed": passed,
            "status": status.status, "first_events": len(first),
            "resumed_events": len(sse_events(resumed.body))}


def run_compatibility(base_url: str, token: str, timeout: float, *, HttpRequest: Any,
                      request: Any) -> list[dict[str, Any]]:
    records = []
    for probe_id, spec in compatibility_specs(HttpRequest).items():
        response = request(base_url, token if spec.path.startswith("/v1/") else None, spec, timeout)
        content_type = response.headers.get("Content-Type", "")
        structured = spec.path != "/openapi.json" or ("json" in content_type and response.body.startswith(b"{"))
        records.append({"id": probe_id, "passed": 200 <= response.status < 300 and structured,
                        "status": response.status})
    return records


def probe_specs(HttpRequest: Any, json_request: Any) -> dict[str, tuple[Any, str]]:
    hostile = "bad\r\nInjected: true"
    return {
        "auth.valid": (HttpRequest("GET", "/v1/status"), "valid"),
        "auth.missing": (HttpRequest("GET", "/v1/status"), "missing"),
        "auth.invalid": (HttpRequest("GET", "/v1/status"), "invalid"),
        "auth.query_token": (HttpRequest("GET", "/v1/status?access_token=invalid"), "missing"),
        "auth.conflicting": (HttpRequest("GET", "/v1/status"), "conflicting"),
        "auth.forwarded_host": (HttpRequest("GET", "/v1/status"), "forwarded_host"),
        "auth.forwarded_origin": (HttpRequest("GET", "/v1/status"), "forwarded_origin"),
        "error.malformed_json": (HttpRequest("POST", "/v1/query", b"{"), "valid"),
        "error.unknown_id": (HttpRequest("GET", "/v1/jobs/e2e_missing_job"), "valid"),
        "error.conflict": (json_request("POST", "/v1/jobs/e2e_missing_job/cancel", {}), "valid"),
        "error.oversize": (HttpRequest("POST", "/v1/query", b"x" * (129 * 1024)), "valid"),
        "error.traversal": (HttpRequest("GET", "/v1/artifacts/%2e%2e%2fsecret/content"), "valid"),
        "error.hostile_headers": (HttpRequest("GET", "/v1/status"), hostile),
    }


def non_loopback_bind_probe(axon_bin: Path, timeout: float) -> dict[str, Any]:
    env = dict(os.environ)
    for key in ("AXON_HTTP_TOKEN", "AXON_AUTH_MODE", "AXON_GOOGLE_CLIENT_ID", "AXON_GOOGLE_CLIENT_SECRET"):
        env.pop(key, None)
    env["AXON_HTTP_HOST"] = "0.0.0.0"
    with tempfile.TemporaryDirectory(prefix="axon-e2e-bind-") as directory:
        env["AXON_DATA_DIR"] = directory
        try:
            result = subprocess.run([str(axon_bin), "serve"], env=env, capture_output=True,
                                    timeout=min(timeout, 15), text=True)
        except subprocess.TimeoutExpired:
            return {"id": "auth.non_loopback_bind", "passed": False,
                    "error": "tokenless non-loopback server remained running"}
    output = (result.stdout + result.stderr).casefold()
    return {"id": "auth.non_loopback_bind",
            "passed": result.returncode != 0 and ("auth" in output or "token" in output),
            "exit_code": result.returncode}


def redirect_policy_probe(*, HttpAdapterError: type[Exception], SameOriginRedirects: Any) -> dict[str, Any]:
    req = Request("http://127.0.0.1:31001/v1/status", headers={"Authorization": "Bearer sentinel"})
    try:
        SameOriginRedirects().redirect_request(req, None, 302, "Found", {},
                                               "http://127.0.0.1:31002/sink")
    except HttpAdapterError:
        return {"id": "redirect.cross_origin", "passed": True}
    return {"id": "redirect.cross_origin", "passed": False}


def upload_artifact_lifecycle(base_url: str, token: str, manifest: Path, namespace: str,
                              timeout: float, *, adapter: Any) -> list[dict[str, Any]]:
    content = b"Axon HTTP E2E owned upload\n"
    create_body = {"filename": f"{namespace}.txt", "content_type": "text/plain",
                   "size_bytes": len(content), "purpose": "source_artifact", "metadata": {"e2e": namespace}}
    records: list[dict[str, Any]] = []
    isolation = adapter.load_isolation()
    run_id = isolation.Manifest.open(manifest).verify()[0]["payload"]["run_id"]
    operation_id = f"{namespace}_http_upload_lifecycle"
    adapter.register_resource(manifest, "operation", operation_id,
                              {"run_id": run_id, "scenario_id": "http.upload_artifact.lifecycle"})
    attempt = 0

    def call(probe_id: str, spec: Any, expected: set[int], headers: dict[str, str] | None = None) -> Any:
        nonlocal attempt
        attempt += 1
        response = adapter.request(base_url, token, spec, timeout, headers)
        records.append({"id": probe_id, "passed": response.status in expected, "status": response.status})
        return response

    created = call("upload.create", adapter.json_request("POST", "/v1/uploads", create_body), {200, 201})
    binding = {"run_id": run_id, "attempt": attempt, "scenario_id": "http.upload.create",
               "request_id": f"{operation_id}:{attempt}", "origin": "server_response",
               "parent_resource_type": "operation", "parent_identity": operation_id}
    adapter.register_response_resources(manifest, created, binding)
    try:
        payload = json.loads(created.body)
    except json.JSONDecodeError:
        payload = {}
    upload_id = payload.get("upload_id")
    if not isinstance(upload_id, str):
        records.append({"id": "upload.lifecycle", "passed": False, "error": "missing upload_id"})
        return records
    call("upload.put", adapter.HttpRequest("PUT", f"/v1/uploads/{upload_id}/content", content), {200},
         {"Content-Type": "text/plain"})
    call("upload.get", adapter.HttpRequest("GET", f"/v1/uploads/{upload_id}"), {200})
    call("upload.list", adapter.HttpRequest("GET", "/v1/uploads?limit=10"), {200})
    completed = call("upload.complete", adapter.json_request("POST", f"/v1/uploads/{upload_id}/complete", {}), {200})
    adapter.register_response_resources(manifest, completed, {**binding, "attempt": attempt,
                                        "scenario_id": "http.upload.complete",
                                        "request_id": f"{operation_id}:{attempt}"})
    try:
        artifact_id = json.loads(completed.body).get("artifact_id")
    except json.JSONDecodeError:
        artifact_id = None
    if isinstance(artifact_id, str):
        call("artifact.get", adapter.HttpRequest("GET", f"/v1/artifacts/{artifact_id}"), {200})
        call("artifact.content", adapter.HttpRequest("GET", f"/v1/artifacts/{artifact_id}/content"), {200})
    call("artifact.list", adapter.HttpRequest("GET", "/v1/artifacts?limit=10"), {200})
    second = call("upload.create_abort", adapter.json_request("POST", "/v1/uploads", create_body), {200, 201})
    adapter.register_response_resources(manifest, second, {**binding, "attempt": attempt,
                                        "scenario_id": "http.upload.create_abort",
                                        "request_id": f"{operation_id}:{attempt}"})
    try:
        second_id = json.loads(second.body).get("upload_id")
    except json.JSONDecodeError:
        second_id = None
    if isinstance(second_id, str):
        call("upload.abort", adapter.HttpRequest("DELETE", f"/v1/uploads/{second_id}", b'{"reason":"e2e"}'), {200})
    return records
