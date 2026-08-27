#!/usr/bin/env python3
"""Catalog-driven HTTP adapter for Axon E2E scenarios.

The catalog is data, never executable input. Requests are assembled as typed
values and sent with urllib directly; redirects across origins are rejected so
an Authorization header can never be forwarded to another service.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
import re
import ssl
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.parse import urljoin, urlparse
from urllib.request import HTTPRedirectHandler, Request, build_opener

ROOT = Path(__file__).resolve().parents[3]
DEFAULT_CATALOG = ROOT / "tests/e2e/catalog/catalog.json"
DEFAULT_OPENAPI = ROOT / "apps/web/openapi/axon.json"
DEFAULT_COVERAGE = ROOT / "tests/e2e/http/coverage.json"
SECRET = re.compile(r"authorization|cookie|token|secret|password|api[_-]?key", re.I)


class HttpAdapterError(ValueError):
    pass


@dataclass(frozen=True)
class HttpRequest:
    method: str
    path: str
    body: bytes | None = None


@dataclass(frozen=True)
class HttpResponse:
    status: int
    headers: dict[str, str]
    body: bytes


class SameOriginRedirects(HTTPRedirectHandler):
    """Allow redirects only within the request's scheme/host/port tuple."""

    def redirect_request(self, req: Request, fp: Any, code: int, msg: str,
                         headers: Any, newurl: str) -> Request | None:
        if origin(req.full_url) != origin(newurl):
            raise HttpAdapterError("cross-origin redirect refused")
        return super().redirect_request(req, fp, code, msg, headers, newurl)


def origin(url: str) -> tuple[str, str, int | None]:
    parsed = urlparse(url)
    if parsed.scheme not in {"http", "https"} or not parsed.hostname:
        raise HttpAdapterError(f"invalid HTTP URL: {url!r}")
    return parsed.scheme, parsed.hostname.casefold(), parsed.port


def load_catalog(path: Path = DEFAULT_CATALOG) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if value.get("schema_version") != 1 or not isinstance(value.get("scenarios"), list):
        raise HttpAdapterError("unsupported E2E catalog")
    return value


def scenarios(path: Path = DEFAULT_CATALOG, *, ids: set[str] | None = None,
              group: str | None = None, shard_index: int = 0,
              shard_count: int = 1) -> list[dict[str, Any]]:
    if shard_count < 1 or not 0 <= shard_index < shard_count:
        raise HttpAdapterError("shard index must be in [0, shard count)")
    selected = [item for item in load_catalog(path)["scenarios"] if "http" in item["surfaces"]]
    if ids:
        missing = ids - {item["id"] for item in selected}
        if missing:
            raise HttpAdapterError(f"unknown HTTP scenario(s): {', '.join(sorted(missing))}")
        selected = [item for item in selected if item["id"] in ids]
    if group:
        selected = [item for item in selected if item["lifecycle"] == group]
    return [item for item in selected if
            int(hashlib.sha256(item["id"].encode()).hexdigest(), 16) % shard_count == shard_index]


def fixture_for(scenario: dict[str, Any]) -> dict[str, Any]:
    relative = scenario.get("requests", {}).get("http")
    if not relative:
        raise HttpAdapterError(f"{scenario.get('id')}: HTTP request fixture is missing")
    path = (ROOT / relative).resolve()
    try:
        path.relative_to(ROOT / "tests")
    except ValueError as error:
        raise HttpAdapterError(f"{scenario['id']}: request fixture escapes tests") from error
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise HttpAdapterError(f"{scenario['id']}: request fixture must be an object")
    return value


def project(scenario: dict[str, Any], fixture: dict[str, Any], owned_collection: str,
            job_id: str | None = None) -> HttpRequest:
    scenario_id = scenario["id"]
    if scenario_id.startswith("source."):
        body = dict(fixture)
        body["wait"] = scenario["execution_mode"] != "detached"
        if scenario_id == "source.detached.negative":
            body["source"] = ""
        return json_request("POST", "/v1/sources", body)
    if scenario_id == "jobs.stream.happy":
        if not job_id:
            raise HttpAdapterError("jobs.stream.happy requires a harness-owned job id")
        return HttpRequest("GET", f"/v1/jobs/{job_id}/stream")
    if scenario_id == "jobs.cancel.negative":
        return json_request("POST", "/v1/jobs/e2e_missing_job/cancel", {})
    if scenario_id == "prune.plan.happy":
        body = {**fixture, "collection": owned_collection}
        return json_request("POST", "/v1/prune/plan", body)
    if scenario_id == "prune.execute.negative":
        body = {**fixture, "collection": "e2e_foreign_collection", "confirm": True}
        return json_request("POST", "/v1/prune/exec", body)
    raise HttpAdapterError(f"HTTP adapter has no projection for {scenario_id!r}")


def json_request(method: str, path: str, body: dict[str, Any]) -> HttpRequest:
    return HttpRequest(method, path, json.dumps(body, separators=(",", ":")).encode())


def request(base_url: str, token: str | None, spec: HttpRequest, timeout: float,
            extra_headers: dict[str, str] | None = None) -> HttpResponse:
    url = urljoin(base_url.rstrip("/") + "/", spec.path.lstrip("/"))
    if origin(url) != origin(base_url):
        raise HttpAdapterError("request path escaped configured origin")
    headers = {"Accept": "application/json, text/event-stream"}
    if spec.body is not None:
        headers["Content-Type"] = "application/json"
    if token:
        headers["Authorization"] = f"Bearer {token}"
    headers.update(extra_headers or {})
    req = Request(url, data=spec.body, headers=headers, method=spec.method)
    opener = build_opener(SameOriginRedirects())
    try:
        with opener.open(req, timeout=timeout) as response:
            return HttpResponse(response.status, dict(response.headers.items()), response.read())
    except HTTPError as error:
        # HTTP failures are transport evidence, not process failures to coerce.
        return HttpResponse(error.code, dict(error.headers.items()), error.read())
    except (URLError, TimeoutError, ssl.SSLError) as error:
        raise HttpAdapterError(f"HTTP transport failed: {error}") from error


def sse_events(body: bytes) -> list[dict[str, Any]]:
    events: list[dict[str, Any]] = []
    for line in body.decode("utf-8", errors="replace").splitlines():
        if not line.startswith("data:"):
            continue
        payload = line.removeprefix("data:").strip()
        try:
            value = json.loads(payload)
        except json.JSONDecodeError:
            value = {"raw": payload}
        events.append(value if isinstance(value, dict) else {"data": value})
    return events


def redact(value: Any) -> Any:
    if isinstance(value, dict):
        return {key: "[REDACTED]" if SECRET.search(key) else redact(item)
                for key, item in value.items()}
    if isinstance(value, list):
        return [redact(item) for item in value]
    return value


def normalize(scenario: dict[str, Any], response: HttpResponse, elapsed_ms: int) -> dict[str, Any]:
    content_type = response.headers.get("Content-Type", "")
    envelope: Any = None
    if "json" in content_type or response.body.lstrip().startswith((b"{", b"[")):
        try:
            envelope = json.loads(response.body)
        except json.JSONDecodeError:
            envelope = None
    events = sse_events(response.body) if "text/event-stream" in content_type else []
    negative = scenario["polarity"] == "negative"
    status_ok = 400 <= response.status < 500 if negative else 200 <= response.status < 300
    stream_ok = scenario["execution_mode"] != "streamed" or bool(events)
    assertions = [
        {"id": "http.expected_status_class", "passed": status_ok},
        {"id": "http.structured_response", "passed": envelope is not None or bool(events)},
        {"id": "http.stream_events", "passed": stream_ok},
    ]
    assertions.extend({"id": oracle, "passed": evaluate_oracle(
        oracle, scenario, response.status, envelope, events)} for oracle in scenario["semantic_oracles"])
    assertions.extend({"id": oracle, "passed": status_ok}
                      for oracle in scenario["envelope_oracles"]["http"])
    result = "pass" if all(item["passed"] for item in assertions) else "fail"
    return redact({
        "schema_version": 1, "surface": "http", "scenario_id": scenario["id"],
        "result": result, "failure_class": None if result == "pass" else "assertion",
        "status": response.status, "content_type": content_type,
        "headers": response.headers, "body": envelope,
        "stream": {"event_count": len(events), "events": events},
        "timing_ms": elapsed_ms, "attempts": 1,
        "assertions": assertions,
        "cleanup": {"contract": scenario["cleanup_contract"], "registered": True,
                    "status": "registered"},
    })


def evaluate_oracle(oracle: str, scenario: dict[str, Any], status: int,
                    envelope: Any, events: list[dict[str, Any]]) -> bool:
    """Evaluate known semantic contracts; unknown assertions fail closed."""
    if oracle in {"source.accepted", "source.completed", "job.terminal_success"}:
        if not 200 <= status < 300 or not isinstance(envelope, dict):
            return False
        state = str(envelope.get("status", envelope.get("state", ""))).casefold()
        return bool(response_job_id(HttpResponse(status, {}, json.dumps(envelope).encode()))) or state in {
            "accepted", "queued", "running", "completed", "succeeded"
        }
    if oracle in {"job.lifecycle_visible", "job.stream_terminal", "job.visible", "job.transition_valid"}:
        kinds = {str(item.get("kind", item.get("event", ""))).casefold() for item in events}
        if scenario["id"] == "jobs.cancel.negative":
            return status in {404, 409} and isinstance(envelope, dict)
        return 200 <= status < 300 and bool(events) and bool(kinds & {"final", "completed", "failed", "cancelled"})
    if oracle in {"job.not_found", "job.cancel_rejected"}:
        return status in {404, 409} and isinstance(envelope, dict) and ("error" in envelope or "code" in envelope)
    if oracle == "prune.plan_digest_bound":
        if scenario["id"] == "prune.execute.negative":
            return status in {400, 403, 409} and isinstance(envelope, dict)
        return 200 <= status < 300 and isinstance(envelope, dict) and plan_digest(envelope) is not None
    if oracle == "resource.ownership_checked":
        if not isinstance(envelope, dict): return False
        return (200 <= status < 300 and plan_digest(envelope) is not None) or status in {400, 403, 409}
    if oracle in {"rejection.job_missing", "rejection.source_invalid", "rejection.ownership_guard"}:
        return 400 <= status < 500 and isinstance(envelope, dict) and ("error" in envelope or "code" in envelope)
    if oracle == "failure.taxonomy":
        if not 400 <= status < 500 or not isinstance(envelope, dict): return False
        error = envelope.get("error")
        return isinstance(envelope.get("code"), str) or (isinstance(error, dict) and isinstance(error.get("code"), str))
    if oracle.startswith("prune.exec") or oracle.endswith("foreign_rejected"):
        return status in {400, 403, 409} and isinstance(envelope, dict)
    return False


def plan_digest(envelope: dict[str, Any]) -> str | None:
    candidate = envelope.get("plan_digest", envelope.get("digest"))
    return candidate if isinstance(candidate, str) and re.fullmatch(r"[a-fA-F0-9]{32,128}", candidate) else None


def reconcile_inventory(openapi_path: Path = DEFAULT_OPENAPI,
                        coverage_path: Path = DEFAULT_COVERAGE) -> dict[str, list[str]]:
    coverage = json.loads(coverage_path.read_text(encoding="utf-8"))
    groups: dict[str, list[str]] = {name: [] for name in coverage["tag_groups"]}
    document = json.loads(openapi_path.read_text(encoding="utf-8"))
    unclassified = []
    methods = {"get", "put", "post", "delete", "patch", "head", "options", "trace"}
    for path, path_item in document["paths"].items():
      for method, operation_data in path_item.items():
        if method not in methods: continue
        operation = f"{method.upper()} {path}"
        tags = set(operation_data.get("tags", []))
        matches = [name for name, allowed in coverage["tag_groups"].items() if tags & set(allowed)]
        if not matches:
            unclassified.append(operation)
        for name in matches:
            groups[name].append(operation)
    empty = [name for name, operations in groups.items() if not operations]
    if unclassified or empty:
        raise HttpAdapterError(f"HTTP inventory reconciliation failed: unclassified={unclassified}, empty={empty}")
    return groups


def load_isolation() -> Any:
    path = ROOT / "scripts/e2e/lib/run-isolation.py"
    spec = importlib.util.spec_from_file_location("axon_http_run_isolation", path)
    if not spec or not spec.loader:
        raise HttpAdapterError("run-isolation module is unavailable")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def register_resource(manifest_path: Path, resource_type: str, identity: str,
                      metadata: dict[str, Any] | None = None) -> None:
    isolation = load_isolation()
    try:
        isolation.Manifest.open(manifest_path).register(resource_type, identity, metadata)
    except isolation.IsolationError as error:
        raise HttpAdapterError(f"resource registration rejected: {error}") from error


def register_response_resources(manifest_path: Path, response: HttpResponse,
                                binding: dict[str, Any] | None = None) -> list[tuple[str, str]]:
    """Register resource IDs returned by mutations; foreign IDs fail closed."""
    try:
        value = json.loads(response.body)
    except (UnicodeDecodeError, json.JSONDecodeError):
        return []
    if not isinstance(value, dict): return []
    registered = []
    for resource_type, keys in {"upload": ("upload_id", "uploadId"),
                                "artifact": ("artifact_id", "artifactId"),
                                "watch": ("watch_id", "watchId")}.items():
        identity = next((value.get(key) for key in keys if isinstance(value.get(key), str)), None)
        if identity:
            metadata = {"owner": "http-adapter", **(binding or {})}
            register_resource(manifest_path, resource_type, identity, metadata)
            registered.append((resource_type, identity))
    return registered


def compatibility_specs() -> dict[str, HttpRequest]:
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


def disconnect_reconnect_probe(base_url: str, token: str, job_id: str,
                               timeout: float) -> dict[str, Any]:
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
        # Closing here deliberately simulates a vanished client.
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


def run_compatibility(base_url: str, token: str, timeout: float) -> list[dict[str, Any]]:
    records = []
    for probe_id, spec in compatibility_specs().items():
        response = request(base_url, token if spec.path.startswith("/v1/") else None, spec, timeout)
        content_type = response.headers.get("Content-Type", "")
        structured = spec.path != "/openapi.json" or ("json" in content_type and response.body.startswith(b"{"))
        records.append({"id": probe_id, "passed": 200 <= response.status < 300 and structured,
                        "status": response.status})
    return records


def probe_specs() -> dict[str, tuple[HttpRequest, str]]:
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


def redirect_policy_probe() -> dict[str, Any]:
    req = Request("http://127.0.0.1:31001/v1/status", headers={"Authorization": "Bearer sentinel"})
    try:
        SameOriginRedirects().redirect_request(req, None, 302, "Found", {},
                                               "http://127.0.0.1:31002/sink")
    except HttpAdapterError:
        return {"id": "redirect.cross_origin", "passed": True}
    return {"id": "redirect.cross_origin", "passed": False}


def upload_artifact_lifecycle(base_url: str, token: str, manifest: Path,
                              namespace: str, timeout: float) -> list[dict[str, Any]]:
    content = b"Axon HTTP E2E owned upload\n"
    create_body = {"filename": f"{namespace}.txt", "content_type": "text/plain",
                   "size_bytes": len(content), "purpose": "source_artifact", "metadata": {"e2e": namespace}}
    records = []
    isolation = load_isolation()
    run_id = isolation.Manifest.open(manifest).verify()[0]["payload"]["run_id"]
    operation_id = f"{namespace}_http_upload_lifecycle"
    register_resource(manifest, "operation", operation_id,
                      {"run_id": run_id, "scenario_id": "http.upload_artifact.lifecycle"})
    attempt = 0
    def call(probe_id: str, spec: HttpRequest, expected: set[int], headers: dict[str, str] | None = None) -> HttpResponse:
        nonlocal attempt
        attempt += 1
        response = request(base_url, token, spec, timeout, headers)
        records.append({"id": probe_id, "passed": response.status in expected, "status": response.status})
        return response
    created = call("upload.create", json_request("POST", "/v1/uploads", create_body), {200, 201})
    binding = {"run_id": run_id, "attempt": attempt, "scenario_id": "http.upload.create",
               "request_id": f"{operation_id}:{attempt}", "origin": "server_response",
               "parent_resource_type": "operation", "parent_identity": operation_id}
    register_response_resources(manifest, created, binding)
    try: payload = json.loads(created.body)
    except json.JSONDecodeError: payload = {}
    upload_id = payload.get("upload_id")
    if not isinstance(upload_id, str):
        records.append({"id": "upload.lifecycle", "passed": False, "error": "missing upload_id"}); return records
    call("upload.put", HttpRequest("PUT", f"/v1/uploads/{upload_id}/content", content), {200},
         {"Content-Type": "text/plain"})
    call("upload.get", HttpRequest("GET", f"/v1/uploads/{upload_id}"), {200})
    call("upload.list", HttpRequest("GET", "/v1/uploads?limit=10"), {200})
    completed = call("upload.complete", json_request("POST", f"/v1/uploads/{upload_id}/complete", {}), {200})
    register_response_resources(manifest, completed, {**binding, "attempt": attempt,
                                "scenario_id": "http.upload.complete",
                                "request_id": f"{operation_id}:{attempt}"})
    try: completed_payload = json.loads(completed.body)
    except json.JSONDecodeError: completed_payload = {}
    artifact_id = completed_payload.get("artifact_id")
    if isinstance(artifact_id, str):
        call("artifact.get", HttpRequest("GET", f"/v1/artifacts/{artifact_id}"), {200})
        call("artifact.content", HttpRequest("GET", f"/v1/artifacts/{artifact_id}/content"), {200})
    call("artifact.list", HttpRequest("GET", "/v1/artifacts?limit=10"), {200})
    second = call("upload.create_abort", json_request("POST", "/v1/uploads", create_body), {200, 201})
    register_response_resources(manifest, second, {**binding, "attempt": attempt,
                                "scenario_id": "http.upload.create_abort",
                                "request_id": f"{operation_id}:{attempt}"})
    try: second_id = json.loads(second.body).get("upload_id")
    except json.JSONDecodeError: second_id = None
    if isinstance(second_id, str):
        call("upload.abort", HttpRequest("DELETE", f"/v1/uploads/{second_id}", b'{"reason":"e2e"}'), {200})
    return records


def probe_headers(profile: str, token: str) -> dict[str, str]:
    if profile == "valid": return {"Authorization": f"Bearer {token}"}
    if profile == "missing": return {}
    if profile == "invalid": return {"Authorization": "Bearer axon_e2e_invalid"}
    if profile == "conflicting": return {"Authorization": f"Bearer {token}", "x-api-key": "axon_e2e_invalid"}
    if profile == "forwarded_host": return {"Authorization": f"Bearer {token}", "Forwarded": "host=evil.invalid;proto=https", "X-Forwarded-Host": "evil.invalid"}
    if profile == "forwarded_origin": return {"Authorization": f"Bearer {token}", "Origin": "https://evil.invalid"}
    # Header libraries must reject CRLF rather than transmitting it.
    if "\r" in profile or "\n" in profile: raise HttpAdapterError("hostile header value rejected locally")
    raise HttpAdapterError(f"unknown auth profile: {profile}")


def run_probes(base_url: str, token: str, timeout: float) -> list[dict[str, Any]]:
    records = []
    for probe_id, (spec, profile) in probe_specs().items():
        try:
            response = request(base_url, None, spec, timeout, probe_headers(profile, token))
            if probe_id == "auth.valid": passed = 200 <= response.status < 300
            elif probe_id.startswith("auth."): passed = response.status in {400, 401, 403}
            else: passed = response.status in {400, 404, 409, 413, 414, 422, 431}
            records.append({"id": probe_id, "passed": passed, "status": response.status})
        except HttpAdapterError as error:
            records.append({"id": probe_id, "passed": probe_id == "error.hostile_headers",
                            "local_rejection": str(error)})
    return records


def response_job_id(response: HttpResponse) -> str | None:
    """Extract an opaque job identifier without accepting path-like values."""
    try:
        value = json.loads(response.body)
    except (UnicodeDecodeError, json.JSONDecodeError):
        return None
    if not isinstance(value, dict):
        return None
    candidates = [value.get("job_id"), value.get("jobId"), value.get("id")]
    if isinstance(value.get("job"), dict):
        candidates.extend((value["job"].get("id"), value["job"].get("job_id")))
    for candidate in candidates:
        if isinstance(candidate, str) and re.fullmatch(r"[A-Za-z0-9_-]{1,128}", candidate):
            return candidate
    return None


def inventory(openapi_path: Path = DEFAULT_OPENAPI) -> list[str]:
    document = json.loads(openapi_path.read_text(encoding="utf-8"))
    methods = {"get", "put", "post", "delete", "patch", "head", "options", "trace"}
    return sorted(f"{method.upper()} {path}" for path, item in document["paths"].items()
                  for method in item if method in methods)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--catalog", type=Path, default=DEFAULT_CATALOG)
    sub = parser.add_subparsers(dest="command", required=True)
    inventory_parser = sub.add_parser("inventory")
    inventory_parser.add_argument("--openapi", type=Path, default=DEFAULT_OPENAPI)
    inventory_parser.add_argument("--coverage", type=Path, default=DEFAULT_COVERAGE)
    listing = sub.add_parser("list")
    listing.add_argument("--scenario", action="append", default=[])
    run = sub.add_parser("run")
    run.add_argument("--base-url", required=True)
    run.add_argument("--token")
    run.add_argument("--outdir", type=Path, required=True)
    run.add_argument("--owned-collection", required=True)
    run.add_argument("--resource-manifest", type=Path, required=True)
    run.add_argument("--probes", action="store_true")
    run.add_argument("--axon-bin", type=Path)
    run.add_argument("--scenario", action="append", default=[])
    run.add_argument("--scenario-group")
    run.add_argument("--shard-index", type=int, default=0)
    run.add_argument("--shard-count", type=int, default=1)
    run.add_argument("--timeout-secs", type=float, default=120)
    args = parser.parse_args()
    if args.command == "inventory":
        print(json.dumps(reconcile_inventory(args.openapi, args.coverage), sort_keys=True))
        return 0
    selected = scenarios(args.catalog, ids=set(args.scenario),
                         group=getattr(args, "scenario_group", None),
                         shard_index=getattr(args, "shard_index", 0),
                         shard_count=getattr(args, "shard_count", 1))
    if args.command == "list":
        print(json.dumps([item["id"] for item in selected]))
        return 0
    args.outdir.mkdir(parents=True, exist_ok=True)
    register_resource(args.resource_manifest, "collection", args.owned_collection,
                      {"owner": "http-adapter"})
    failed = False
    with (args.outdir / "http-evidence.jsonl").open("w", encoding="utf-8") as evidence:
        owned_job_id: str | None = None
        stream_progress_final: dict[str, Any] = {"id": "stream.progress_final", "passed": False,
                                                 "error": "stream scenario not executed"}
        for scenario in selected:
            spec = project(scenario, fixture_for(scenario), args.owned_collection, owned_job_id)
            started = time.monotonic_ns()
            response = request(args.base_url, args.token, spec, args.timeout_secs)
            register_response_resources(args.resource_manifest, response)
            record = normalize(scenario, response, (time.monotonic_ns() - started) // 1_000_000)
            if scenario["capability"] == "source":
                owned_job_id = response_job_id(response) or owned_job_id
            if scenario["id"] == "jobs.stream.happy":
                kinds = {str(item.get("kind", "")).casefold() for item in sse_events(response.body)}
                stream_progress_final = {"id": "stream.progress_final",
                                         "passed": bool(kinds & {"progress", "started", "stage"}) and
                                                   bool(kinds & {"final", "completed", "failed", "cancelled"}),
                                         "event_kinds": sorted(kinds)}
            evidence.write(json.dumps(record, ensure_ascii=False) + "\n")
            failed |= record["result"] != "pass"
        if args.probes:
            probes = run_probes(args.base_url, args.token or "", args.timeout_secs)
            probes.extend(run_compatibility(args.base_url, args.token or "", args.timeout_secs))
            probes.append(redirect_policy_probe())
            probes.append(stream_progress_final)
            probes.extend(upload_artifact_lifecycle(args.base_url, args.token or "",
                                                     args.resource_manifest, args.owned_collection,
                                                     args.timeout_secs))
            if args.axon_bin:
                probes.append(non_loopback_bind_probe(args.axon_bin, args.timeout_secs))
            else:
                probes.append({"id": "auth.non_loopback_bind", "passed": False,
                               "error": "--axon-bin is required for bind-policy probe"})
            if owned_job_id:
                probes.append(disconnect_reconnect_probe(args.base_url, args.token or "",
                                                         owned_job_id, args.timeout_secs))
            else:
                probes.append({"id": "stream.disconnect_reconnect", "passed": False,
                               "error": "no harness-owned job id"})
            for probe in probes:
                evidence.write(json.dumps({"schema_version": 1, "surface": "http", "scenario_id": probe["id"],
                                           "result": "pass" if probe["passed"] else "fail",
                                           "assertions": [probe]}, ensure_ascii=False) + "\n")
                failed |= not probe["passed"]
    return 1 if failed else 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, KeyError, json.JSONDecodeError, HttpAdapterError) as error:
        print(f"HTTP catalog error: {error}", file=sys.stderr)
        raise SystemExit(2)
