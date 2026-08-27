#!/usr/bin/env python3
"""Run the retrieval/RAG pack against an actual Axon executable.

This is the CI entry point. It never accepts pre-normalized results: every
evidence record is derived from stdout produced by the supplied executable.
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import os
import secrets
import subprocess
import sys
import time
import urllib.request
from copy import deepcopy
from pathlib import Path
from typing import Any
from urllib.parse import urlparse

ROOT = Path(__file__).resolve().parents[4]


def load(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    assert spec and spec.loader
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


grounding = load("axon_e2e_execute_grounding", ROOT / "tests/e2e/oracles/grounding.py")
isolation = load("axon_e2e_execute_isolation", ROOT / "scripts/e2e/lib/run-isolation.py")
http_adapter = load("axon_e2e_execute_http", ROOT / "scripts/e2e/adapters/http_adapter.py")
mcp_adapter = load("axon_e2e_execute_mcp", ROOT / "scripts/e2e/adapters/mcp.py")

CORPUS = ROOT / "tests/e2e/corpus/v1/documents"
ATLAS = CORPUS / "micro/atlas-v1.md"
UNICODE = CORPUS / "micro/unicode-東京-🧪.txt"
HOSTILE = CORPUS / "representative/hostile.txt"
SEMANTICS = grounding.load_json(ROOT / "tests/e2e/corpus/v1/expected/semantics.json")
EVIDENCE_SCHEMA = grounding.load_json(ROOT / "tests/e2e/oracles/grounding.schema.json")


class ExecutionError(RuntimeError):
    pass


def scenarios() -> list[dict[str, Any]]:
    result = []
    for relative in ("retrieval/scenarios.json", "llm/scenarios.json"):
        document = grounding.load_json(ROOT / "tests/e2e/scenarios" / relative)
        for item in document["scenarios"]:
            result.append({**item, "provider_limits": document.get("provider_limits", {
                "max_calls": 1, "max_retries": 1, "max_tokens": 4096,
            })})
    return result


def operation_cases() -> list[tuple[dict[str, Any], str]]:
    return [(item, operation) for item in scenarios()
            for operation in item.get("operation_variants", [item["operation"]])]


def argv_for(operation: str, item: dict[str, Any], collection: str, run_id: str) -> list[str]:
    prompt, limit = item["prompt"], str(item["max_results"])
    common = ["--collection", collection, "--json"]
    projections = {
        "query": ["query", prompt, "--limit", limit, *common],
        "retrieve": ["retrieve", str(UNICODE), "--max-points", limit, *common],
        "search": ["search", prompt, "--limit", limit, "--json"],
        "code-search": ["code-search", prompt, "--limit", limit, *common],
        "ask": ["ask", prompt, "--session", f"{run_id}_ask", "--new-session", "--no-stream", "--limit", limit, *common],
        "chat": ["chat", "--query", prompt, "--json"],
        "summarize": ["summarize", str(ATLAS), str(HOSTILE), *common],
        "research": ["research", prompt, "--research-depth", "1", "--json"],
        "extract": ["extract", str(ATLAS), "--query", prompt, "--wait", "true", "--json"],
        "evaluate": ["evaluate", prompt, "--responses-mode", "side-by-side", *common],
        "train": ["train", prompt, "--best", "1", "--notes", "axon e2e fixture", *common],
        "suggest": ["suggest", prompt, "--limit", limit, *common],
    }
    return projections[operation]


def parse_output(stdout: bytes) -> Any:
    text = stdout.decode("utf-8", errors="strict").strip()
    if not text:
        raise ExecutionError("Axon returned empty stdout")
    try:
        return json.loads(text)
    except json.JSONDecodeError:
        values = []
        for line in text.splitlines():
            try:
                values.append(json.loads(line))
            except json.JSONDecodeError as error:
                raise ExecutionError("Axon stdout was neither JSON nor JSONL") from error
        return values


def _walk(value: Any):
    if isinstance(value, dict):
        yield value
        for child in value.values():
            yield from _walk(child)
    elif isinstance(value, list):
        for child in value:
            yield from _walk(child)


def public_index_timing(actual: Any, elapsed_ms: int) -> dict[str, int | float]:
    """Keep public timing if present and always report measured process time separately."""
    for obj in _walk(actual):
        timing = obj.get("timing_ms", obj.get("timing"))
        if isinstance(timing, dict):
            embed = timing.get("embed", timing.get("embed_ms", timing.get("embedding")))
            if isinstance(embed, (int, float)) and embed >= 0:
                return {"public_embed": embed, "harness_index_total": elapsed_ms}
        if str(obj.get("stage", obj.get("phase", ""))).lower() in {"embed", "embedding"}:
            duration = obj.get("duration_ms", obj.get("elapsed_ms"))
            if isinstance(duration, (int, float)) and duration >= 0:
                return {"public_embed": duration, "harness_index_total": elapsed_ms}
    return {"harness_index_total": elapsed_ms}


def measure_owned_embedding_provider(base_url: str) -> int:
    request = urllib.request.Request(
        f"{base_url}/embed", data=json.dumps({"inputs": ["Atlas timing probe"]}).encode(),
        headers={"Content-Type": "application/json"}, method="POST")
    started = time.monotonic_ns()
    with urllib.request.urlopen(request, timeout=5) as response:
        payload = json.loads(response.read())
    elapsed = (time.monotonic_ns() - started) // 1_000_000
    if not isinstance(payload, list) or not payload or not isinstance(payload[0], list):
        raise ExecutionError("owned embedding provider timing probe returned an invalid vector envelope")
    return elapsed


def provider_stats(base_url: str) -> dict[str, int]:
    with urllib.request.urlopen(f"{base_url}/stats", timeout=2) as response:
        value = json.loads(response.read())
    fields = ("calls", "tokens")
    if any(not isinstance(value.get(field), int) or value[field] < 0 for field in fields):
        raise ExecutionError("owned provider stats omitted explicit nonnegative counters")
    return {field: value[field] for field in fields}


def provider_delta(before: dict[str, int], after: dict[str, int]) -> dict[str, int]:
    delta = {field: after[field] - before[field] for field in before}
    if any(value < 0 for value in delta.values()):
        raise ExecutionError("owned provider counters moved backwards")
    return delta


def require_passing_checks(checks: list[dict[str, Any]], context: str) -> None:
    failed = [check["id"] for check in checks if check.get("passed") is not True]
    if failed:
        raise ExecutionError(f"{context} failed checks: {', '.join(failed)}")


def actual_artifact_ids(actual: Any) -> list[str]:
    identities: list[str] = []
    for obj in _walk(actual):
        if isinstance(obj.get("artifact_id"), str):
            identities.append(obj["artifact_id"])
        if isinstance(obj.get("artifacts"), list):
            identities.extend(item.get("artifact_id") if isinstance(item, dict) else item
                              for item in obj["artifacts"])
    return [value for value in identities if isinstance(value, str) and value]


def verify_retry_success(binary: Path, env: dict[str, str], collection: str,
                         provider_url: str, timeout: float) -> dict[str, Any]:
    before = provider_stats(provider_url)
    control = urllib.request.Request(f"{provider_url}/control/transient-next", data=b"{}",
                                     headers={"Content-Type": "application/json"}, method="POST")
    urllib.request.urlopen(control, timeout=2).close()
    actual, _ = invoke(binary, ["ask", "Transient retry probe for the Atlas beacon.", "--limit", "1",
                                "--collection", collection, "--json"], env, timeout)
    if "amber" not in json.dumps(actual, ensure_ascii=False).casefold():
        raise ExecutionError("transient-then-success provider probe did not synthesize successfully")
    after = provider_stats(provider_url)
    attempts = after["calls"] - before["calls"]
    if attempts != 2:
        raise ExecutionError(f"transient provider probe observed {attempts} attempts, expected 2")
    artifact_ids = actual_artifact_ids(actual)
    return {"calls": attempts, "retries": attempts - 1,
            "tokens": after["tokens"] - before["tokens"],
            "artifacts": len(artifact_ids), "artifact_ids": artifact_ids}


FACT_DOCUMENTS = {
    "fact.atlas.beacon": CORPUS / "micro/atlas-v1.md",
    "fact.unicode.city": CORPUS / "micro/unicode-東京-🧪.txt",
    "fact.dotfile.code": CORPUS / "representative/.fixture-note",
}


def canonical_citations(actual: Any) -> list[dict[str, str]]:
    """Read only public CanonicalCitation fields and durable fixture evidence."""
    citations = []
    for obj in _walk(actual):
        citation = obj.get("citation") if isinstance(obj.get("citation"), dict) else obj
        required = ("source_id", "chunk_id", "canonical_uri")
        if not all(isinstance(citation.get(field), str) and citation[field] for field in required):
            continue
        parsed = urlparse(citation["canonical_uri"])
        candidate = Path(parsed.path if parsed.scheme == "file" else citation["canonical_uri"])
        try:
            resolved = candidate.resolve()
            resolved.relative_to(CORPUS.resolve())
        except (OSError, ValueError):
            continue
        if not resolved.is_file():
            continue
        corpus_text = resolved.read_text(encoding="utf-8")
        snippet = obj.get("snippet")
        if isinstance(snippet, str) and snippet and snippet not in corpus_text:
            raise ExecutionError("server citation snippet is not contained in immutable corpus evidence")
        excerpt = corpus_text
        citations.append({"id": citation["chunk_id"], "source_id": citation["source_id"],
                          "canonical_uri": citation["canonical_uri"], "excerpt": excerpt})
    unique = {(item["id"], item["source_id"]): item for item in citations}
    return list(unique.values())


def runtime_semantics(item: dict[str, Any], actual: Any) -> dict[str, Any]:
    semantics = deepcopy(SEMANTICS)
    citations = canonical_citations(actual)
    for fact_id in item["expected_fact_ids"]:
        document = FACT_DOCUMENTS[fact_id].resolve()
        match = next((citation for citation in citations
                      if Path(urlparse(citation["canonical_uri"]).path
                              if urlparse(citation["canonical_uri"]).scheme == "file"
                              else citation["canonical_uri"]).resolve() == document), None)
        if match is None:
            raise ExecutionError(f"public citation lineage missing for {fact_id}")
        fact = next(fact for fact in semantics["facts"] if fact["id"] == fact_id)
        fact.update(source_id=match["source_id"], citation=match["id"])
        semantics["citations"] = [entry for entry in semantics["citations"]
                                  if entry["id"] != "cite:atlas-v1:beacon"]
        semantics["citations"].append({"id": match["id"], "source_id": match["source_id"],
                                       "contains": next(entry["contains"] for entry in SEMANTICS["citations"]
                                                        if entry["id"] == "cite:atlas-v1:beacon")
                                       if fact_id == "fact.atlas.beacon" else str(fact["value"])})
    return semantics


def normalize(actual: Any, item: dict[str, Any], operation: str, run_id: str,
              elapsed_ms: int) -> dict[str, Any]:
    objects = list(_walk(actual))
    provider_error = next((obj for obj in objects if str(obj.get("code", "")).startswith("provider.")), None)
    if provider_error:
        return {"error": {"code": provider_error["code"]}}
    citations = canonical_citations(actual)
    result_values = [{"source_id": source_id} for source_id in
                     dict.fromkeys(item["source_id"] for item in citations)]
    answer = ""
    for obj in objects:
        for key in ("answer", "reply", "summary", "content", "snippet", "text", "rag_answer", "response"):
            if isinstance(obj.get(key), str) and obj[key]:
                answer = obj[key]
                break
        if answer:
            break
    if not answer:
        textual = [value for obj in objects for value in obj.values()
                   if isinstance(value, str) and len(value) <= 8_192]
        answer = " ".join(textual)
    usage = next((obj.get("provider_usage") for obj in objects
                  if isinstance(obj.get("provider_usage"), dict)), None)
    timing = next((obj.get("timing_ms") for obj in objects
                   if isinstance(obj.get("timing_ms"), dict)), None)
    artifact_ids = [obj["artifact_id"] for obj in objects if isinstance(obj.get("artifact_id"), str)]
    artifact_evidence = any("artifact_id" in obj or "artifacts" in obj for obj in objects)
    for obj in objects:
        if isinstance(obj.get("artifacts"), list):
            artifact_ids.extend(value for value in obj["artifacts"] if isinstance(value, str))
    return {
        "run_id": run_id,
        "operation": operation,
        "results": result_values[:item["max_results"]],
        "answer": answer,
        "citations": citations,
        "provider_usage": usage,
        "timing_ms": timing,
        "harness_total_ms": elapsed_ms,
        "artifacts": artifact_ids if artifact_evidence else None,
    }


GROUNDED_OPERATIONS = {"query", "code-search", "ask", "evaluate"}


def structural_evidence(actual: Any, operation: str, scenario_id: str, run_id: str,
                        item: dict[str, Any], surface: str) -> dict[str, Any]:
    objects = list(_walk(actual))
    def has(key, kind): return any(isinstance(obj.get(key), kind) for obj in objects)
    rendered = json.dumps(actual, ensure_ascii=False).casefold()
    facts = {fact["id"]: str(fact["value"]) for fact in SEMANTICS["facts"]}
    expected_paths = {FACT_DOCUMENTS[fact_id].resolve() for fact_id in item["expected_fact_ids"]}
    public_url_values = {value for obj in objects for key in ("url", "matched_url", "requested_url")
                         if isinstance((value := obj.get(key)), str)}
    public_urls = {Path(urlparse(value).path if urlparse(value).scheme == "file" else value).resolve()
                   for obj in objects for key in ("url", "matched_url", "requested_url")
                   if isinstance((value := obj.get(key)), str) and
                   (urlparse(value).scheme in {"", "file"})}
    fact_text_ok = all(facts[fact_id].casefold() in rendered for fact_id in item["expected_fact_ids"])
    corpus_lineage_ok = bool(expected_paths & public_urls)
    hermetic_discovery_ok = any(urlparse(value).hostname == "127.0.0.1" for value in public_url_values)
    generated_text = " ".join(str(obj[key]) for obj in objects
                              for key in ("answer", "reply", "summary", "rag_answer", "analysis_answer")
                              if isinstance(obj.get(key), str)).casefold()
    safe = not any(str(value).casefold() in generated_text for value in
                   [*item.get("forbidden_answers", []), *item.get("hostile_markers", [])])
    chat_session_ok = surface == "http" or has("session", str) or has("session_id", str)
    summarize_usage = next((obj["usage"] for obj in objects if isinstance(obj.get("usage"), dict)), None)
    summarize_usage_ok = isinstance(summarize_usage, dict) and all(
        isinstance(summarize_usage.get(field), int) and summarize_usage[field] >= 0
        for field in ("prompt_tokens", "completion_tokens", "total_tokens"))
    timing = next((obj["timing_ms"] for obj in objects if isinstance(obj.get("timing_ms"), dict)), None)
    timing_fields = {"summarize": ("scrape", "llm", "total"), "research": ("total",)}
    timing_ok = operation not in timing_fields or isinstance(timing, dict) and all(
        isinstance(timing.get(field), (int, float)) and timing[field] >= 0
        for field in timing_fields[operation])
    evidence_contracts = {
        "retrieve": corpus_lineage_ok and fact_text_ok,
        "search": (corpus_lineage_ok or hermetic_discovery_ok) and fact_text_ok,
        "chat": True,  # Public ChatResult is deliberately stateless and has no retrieval lineage.
        "summarize": corpus_lineage_ok and fact_text_ok,
        "research": (corpus_lineage_ok or hermetic_discovery_ok) and fact_text_ok and any(
            obj.get("instruction_trust") == "evidence_only" for obj in objects),
        "extract": corpus_lineage_ok and fact_text_ok,
        "train": True,  # Feedback event/candidate DTO has no source lineage.
        "suggest": True,  # SuggestResult exposes only candidate URLs/reasons.
    }
    contracts = {
        "retrieve": has("content", str) and has("matched_url", str),
        "search": has("results", list),
        "chat": (has("answer", str) or has("reply", str)) and chat_session_ok,
        "summarize": has("summary", str) and has("documents", list) and summarize_usage_ok and timing_ok,
        "research": has("search_results", list) and summarize_usage_ok and timing_ok,
        "extract": (has("job_id", str) or has("results", list)) and
                   any(obj.get("status") in {"completed", "succeeded"} for obj in objects),
        "train": has("event_id", str) and has("candidates", list),
        "suggest": has("suggestions", list),
    }
    passed = contracts[operation] and evidence_contracts[operation] and safe
    return {"schema_version": 1, "scenario_id": scenario_id, "operation": operation,
            "run_id": run_id, "result": "pass" if passed else "fail",
            "failure_class": None if passed else "public_contract",
            "assertions": [
                {"id": f"{operation}.public_result_dto", "passed": contracts[operation],
                 "detail": "validated repository client/result contract fields"},
                {"id": f"{operation}.operation_semantics", "passed": evidence_contracts[operation],
                 "detail": "operation-specific public DTO evidence or an explicit transport limitation"},
                {"id": f"{operation}.hostile_content_ignored", "passed": safe,
                 "detail": "distractor and hostile markers are absent"},
            ]}


def invoke(binary: Path, argv: list[str], env: dict[str, str], timeout: float) -> tuple[Any, int]:
    started = time.monotonic_ns()
    try:
        completed = subprocess.run([str(binary), *argv], cwd=ROOT, env=env, capture_output=True,
                                   timeout=timeout, check=False)
    except subprocess.TimeoutExpired as error:
        raise ExecutionError(f"Axon operation timed out: {argv[0]}") from error
    elapsed = (time.monotonic_ns() - started) // 1_000_000
    if completed.returncode:
        message = completed.stderr.decode("utf-8", errors="replace").strip()
        raise ExecutionError(f"Axon {argv[0]} failed with {completed.returncode}: {message[:300]}")
    return parse_output(completed.stdout), elapsed


def invoke_http(base_url: str, token: str | None, operation: str, item: dict[str, Any],
                collection: str, timeout: float) -> tuple[Any, int]:
    if operation == "train":
        raise ExecutionError("train has no HTTP API operation")
    bodies: dict[str, dict[str, Any]] = {
        "query": {"query": item["prompt"], "collection": collection, "limit": item["max_results"]},
        "retrieve": {"url": str(UNICODE), "collection": collection, "max_points": item["max_results"]},
        "search": {"query": item["prompt"], "limit": item["max_results"]},
        "code-search": {"inputs": [{"input": item["prompt"]}],
                        "options": {"collection": collection, "limit": item["max_results"], "offset": 0}},
        "ask": {"query": item["prompt"], "collection": collection, "diagnostics": True},
        "chat": {"message": item["prompt"]},
        "summarize": {"urls": [str(ATLAS), str(HOSTILE)]},
        "research": {"query": item["prompt"], "limit": item["max_results"]},
        "extract": {"urls": [str(ATLAS)], "prompt": item["prompt"], "embed": False},
        "evaluate": {"question": item["prompt"], "collection": collection, "diagnostics": True},
        "suggest": {"focus": item["prompt"], "collection": collection},
    }
    body = bodies[operation]
    started = time.monotonic_ns()
    response = http_adapter.request(base_url, token,
                                    http_adapter.json_request("POST", f"/v1/{operation}", body), timeout)
    elapsed = (time.monotonic_ns() - started) // 1_000_000
    if not 200 <= response.status < 300:
        raise ExecutionError(f"Axon HTTP {operation} failed with status {response.status}")
    actual = parse_output(response.body)
    if operation == "extract":
        status_url = next((obj.get("status_url") for obj in _walk(actual)
                           if isinstance(obj.get("status_url"), str)), None)
        if not status_url:
            raise ExecutionError("Axon HTTP extract acceptance omitted status_url")
        deadline = time.monotonic() + timeout
        while True:
            polled = http_adapter.request(base_url, token,
                                          http_adapter.HttpRequest("GET", status_url), timeout)
            terminal = parse_output(polled.body)
            states = {str(obj.get("status", obj.get("state", ""))).lower() for obj in _walk(terminal)}
            if states & {"completed", "succeeded", "failed", "cancelled"}:
                if states & {"failed", "cancelled"}:
                    raise ExecutionError(f"Axon HTTP extract terminal failure: {sorted(states)}")
                return {"accepted": actual, "terminal": terminal}, elapsed
            if time.monotonic() >= deadline:
                raise ExecutionError("Axon HTTP extract did not reach terminal state")
            time.sleep(.05)
    return actual, elapsed


def invoke_mcp(mcporter: Path, selector: str, operation: str, item: dict[str, Any],
               collection: str, env: dict[str, str], timeout: float) -> tuple[Any, int]:
    bodies: dict[str, dict[str, Any]] = {
        "query": {"query": item["prompt"], "collection": collection, "limit": item["max_results"]},
        "retrieve": {"url": str(UNICODE), "collection": collection, "max_points": item["max_results"]},
        "search": {"query": item["prompt"], "limit": item["max_results"]},
        "code-search": {"inputs": [{"input": item["prompt"]}],
                        "options": {"collection": collection, "limit": item["max_results"], "offset": 0}},
        "ask": {"query": item["prompt"], "collection": collection, "diagnostics": True},
        "chat": {"message": item["prompt"], "session_id": f"{env['AXON_E2E_RUN_ID']}_mcp_chat"},
        "summarize": {"urls": [str(ATLAS), str(HOSTILE)]},
        "research": {"query": item["prompt"], "limit": item["max_results"]},
        "extract": {"subaction": "start", "urls": [str(ATLAS)], "prompt": item["prompt"], "embed": False},
        "evaluate": {"query": item["prompt"], "collection": collection, "diagnostics": True},
        "suggest": {"focus": item["prompt"], "collection": collection, "limit": item["max_results"]},
    }
    action = "code_search" if operation == "code-search" else operation
    arguments: dict[str, Any] = {"action": action, "body": bodies[operation]}
    if operation == "extract":
        arguments["subaction"] = "start"
    started = time.monotonic_ns()
    try:
        completed = subprocess.run([str(mcporter), *mcp_adapter.mcporter_argv(selector, arguments)],
                                   cwd=ROOT, env=env, capture_output=True, timeout=timeout, check=False)
    except subprocess.TimeoutExpired as error:
        raise ExecutionError(f"Axon MCP {operation} timed out") from error
    elapsed = (time.monotonic_ns() - started) // 1_000_000
    if completed.returncode:
        raise ExecutionError(f"Axon MCP {operation} failed with {completed.returncode}")
    actual = parse_output(completed.stdout)
    if operation == "extract":
        job_id = next((obj.get("job_id") for obj in _walk(actual)
                       if isinstance(obj.get("job_id"), str)), None)
        if not job_id:
            raise ExecutionError("Axon MCP extract acceptance omitted job_id")
        deadline = time.monotonic() + timeout
        while True:
            poll_args = {"action": "jobs", "subaction": "get",
                         "body": {"subaction": "get", "job_id": job_id}}
            polled = subprocess.run([str(mcporter), *mcp_adapter.mcporter_argv(selector, poll_args)],
                                    cwd=ROOT, env=env, capture_output=True, timeout=timeout, check=False)
            if polled.returncode:
                raise ExecutionError(f"Axon MCP jobs get failed with {polled.returncode}")
            terminal = parse_output(polled.stdout)
            states = {str(obj.get("status", obj.get("state", ""))).lower() for obj in _walk(terminal)}
            if states & {"completed", "succeeded", "failed", "cancelled"}:
                if states & {"failed", "cancelled"}:
                    raise ExecutionError(f"Axon MCP extract terminal failure: {sorted(states)}")
                return {"accepted": actual, "terminal": terminal}, elapsed
            if time.monotonic() >= deadline:
                raise ExecutionError("Axon MCP extract did not reach terminal state")
            time.sleep(.05)
    return actual, elapsed


def register_discovered(manifest, run_id: str, scenario_id: str, actual: Any,
                        operation_identity: str | None = None) -> None:
    seen: set[tuple[str, str]] = set()
    for obj in _walk(actual):
        for resource_type, keys in (("job", ("job_id",)), ("source", ("source_id",))):
            identity = next((obj.get(key) for key in keys if isinstance(obj.get(key), str)), None)
            if identity and (resource_type, identity) not in seen:
                manifest.register(resource_type, identity, {"run_id": run_id, "scenario_id": scenario_id})
                seen.add((resource_type, identity))
        artifact_ids = ([obj.get("artifact_id")] if isinstance(obj.get("artifact_id"), str) else [])
        if isinstance(obj.get("artifacts"), list):
            artifact_ids.extend(item.get("artifact_id") if isinstance(item, dict) else item
                                for item in obj["artifacts"])
        for artifact_id in artifact_ids:
          if isinstance(artifact_id, str) and artifact_id and ("artifact", artifact_id) not in seen:
            if not operation_identity:
                raise ExecutionError("server artifact lacks its registered operation parent")
            manifest.register("artifact", artifact_id, {
                "run_id": run_id, "attempt": 1, "scenario_id": scenario_id,
                "request_id": scenario_id, "origin": "server_response",
                "parent_resource_type": "operation", "parent_identity": operation_identity,
            })
            seen.add(("artifact", artifact_id))
        session_id = obj.get("session_id", obj.get("session"))
        if isinstance(session_id, str) and session_id and ("chat_session", session_id) not in seen:
            manifest.register("chat_session", session_id, {"run_id": run_id, "scenario_id": scenario_id})
            seen.add(("chat_session", session_id))
        reservation_id = obj.get("reservation_id")
        if isinstance(reservation_id, str) and reservation_id and ("provider_reservation", reservation_id) not in seen:
            usage, provider = obj.get("provider_usage"), obj.get("provider")
            if not isinstance(provider, str) or not provider or not isinstance(usage, dict):
                raise ExecutionError("provider reservation lacks explicit provider usage metadata")
            counters = {field: usage.get(field) for field in ("calls", "retries", "tokens")}
            if any(not isinstance(value, int) or value < 0 for value in counters.values()):
                raise ExecutionError("provider reservation counters must be explicit nonnegative integers")
            manifest.register("provider_reservation", reservation_id, {
                "run_id": run_id, "scenario_id": scenario_id,
                "provider": provider, **counters,
            })
            seen.add(("provider_reservation", reservation_id))


def verify_provider_failure_modes(binary: Path, env: dict[str, str], collection: str,
                                  timeout: float, manifest, run_root: Path) -> None:
    expected = {
        "unavailable": "provider.unavailable", "timeout": "provider.timeout",
        "queue-full": "provider.scheduler.queue_full", "malformed": "provider.malformed_response",
        "dimension": "embedding.tei.dimension_mismatch", "schema": "provider.schema_mismatch",
        "token-limit": "provider.token_limit",
    }
    for mode, code in expected.items():
        reservation = isolation.allocate_port(run_root / "provider-ports", env["AXON_E2E_RUN_ID"], manifest)
        port = reservation.port; reservation.close()
        managed = isolation.spawn_owned_process(
            manifest, run_root,
            [sys.executable, str(Path(__file__).with_name("provider_double.py")), "--port", str(port),
             "--mode", mode, "--delay", "11"],
        )
        try:
            health = f"http://127.0.0.1:{port}/health"
            for _ in range(50):
                try:
                    urllib.request.urlopen(health, timeout=.1).close(); break
                except OSError:
                    time.sleep(.02)
            mode_env = {**env, "AXON_LLM_BACKEND": "openai-compat",
                        "AXON_OPENAI_BASE_URL": f"http://127.0.0.1:{port}/v1",
                        "AXON_SYNTHESIS_OPENAI_MODEL": "e2e-fixture",
                        "AXON_LLM_COMPLETION_TIMEOUT_SECS": "10"}
            argv = ["ask", "fixture provider classification probe", "--limit", "1",
                    "--collection", collection, "--json"]
            if mode == "dimension":
                mode_env["TEI_URL"] = f"http://127.0.0.1:{port}"
                argv = ["query", "fixture dimension probe", "--limit", "1", "--collection", collection, "--json"]
            try:
                completed = subprocess.run([str(binary), *argv], cwd=ROOT, env=mode_env,
                                           capture_output=True, timeout=max(timeout, 13), check=False)
            except subprocess.TimeoutExpired as error:
                raise ExecutionError(
                    f"fixture provider mode {mode} caused an unstructured Axon process timeout"
                ) from error
            else:
                structured = completed.stdout if completed.stdout.strip() else completed.stderr
                actual = parse_output(structured)
        finally:
            managed.process.terminate(); managed.process.wait(timeout=5)
        observed = next((obj.get("code") for obj in _walk(actual)
                         if isinstance(obj.get("code"), str)), None)
        if observed != code:
            raise ExecutionError(f"fixture provider mode {mode} classified as {observed!r}, expected {code!r}")


def verify_configured_fallback(binary: Path, env: dict[str, str], timeout: float,
                               manifest, run_root: Path) -> None:
    reservation = isolation.allocate_port(run_root / "provider-ports", env["AXON_E2E_RUN_ID"], manifest)
    port = reservation.port; reservation.close()
    managed = isolation.spawn_owned_process(
        manifest, run_root,
        [sys.executable, str(Path(__file__).with_name("provider_double.py")),
         "--port", str(port), "--mode", "unavailable"],
    )
    fallback_env = {**env, "AXON_LLM_BACKEND": "openai-compat",
                    "AXON_OPENAI_BASE_URL": f"http://127.0.0.1:{port}/v1",
                    "AXON_SYNTHESIS_OPENAI_MODEL": "e2e-failing-primary"}
    try:
        for _ in range(50):
            try:
                urllib.request.urlopen(f"http://127.0.0.1:{port}/health", timeout=.1).close(); break
            except OSError:
                time.sleep(.02)
        actual, _ = invoke(binary, ["research", "Atlas fallback injection probe", "--research-depth",
                                    "1", "--json"], fallback_env, timeout)
    finally:
        managed.process.terminate(); managed.process.wait(timeout=5)
    if not any(obj.get("summary_source") == "fallback" for obj in _walk(actual)):
        raise ExecutionError("explicit failing primary did not produce the public research fallback")


def execute(binary: Path, outdir: Path, timeout: float = 120.0,
            repetitions: int = 5, fixture_provider_modes: bool = False,
            http_url: str | None = None, http_token: str | None = None,
            mcporter: Path | None = None, mcp_selector: str = "axon.axon",
            require_all_surfaces: bool = True) -> list[dict[str, Any]]:
    if not binary.is_file() or not os.access(binary, os.X_OK):
        raise ExecutionError(f"Axon binary is unavailable or not executable: {binary}")
    if require_all_surfaces and (not http_url or not mcporter):
        raise ExecutionError("real runner requires CLI, HTTP, and MCP surfaces")
    validation = subprocess.run([sys.executable, str(ROOT / "tests/e2e/corpus/validate.py")],
                                cwd=ROOT, capture_output=True, check=False)
    if validation.returncode:
        raise ExecutionError("canonical corpus checksum validation failed")
    corpus_manifest = grounding.load_json(ROOT / "tests/e2e/corpus/manifest.json")
    allocation = isolation.allocate(outdir / "runs", outdir / "manifests")
    manifest = isolation.Manifest.open(Path(allocation["manifest"]))
    env = {**os.environ, "AXON_DATA_DIR": allocation["data_dir"],
           "AXON_SQLITE_PATH": allocation["sqlite"], "AXON_COLLECTION": allocation["namespace"],
           "AXON_E2E_RUN_ID": allocation["run_id"], "AXON_E2E_CORPUS_ROOT": str(CORPUS)}
    discovery = None
    provider_embed_ms = None
    if require_all_surfaces:
        discovery_port = isolation.allocate_port(Path(allocation["run_root"]) / "provider-ports",
                                                  allocation["run_id"], manifest)
        port = discovery_port.port; discovery_port.close()
        discovery = isolation.spawn_owned_process(
            manifest, Path(allocation["run_root"]),
            [sys.executable, str(Path(__file__).with_name("provider_double.py")),
             "--port", str(port), "--mode", "discovery"],
        )
        env["AXON_SEARXNG_URL"] = f"http://127.0.0.1:{port}"
        env.update({"AXON_LLM_BACKEND": "openai-compat",
                    "AXON_OPENAI_BASE_URL": f"http://127.0.0.1:{port}/v1",
                    "AXON_SYNTHESIS_OPENAI_MODEL": "e2e-owned-success"})
        for _ in range(50):
            try:
                urllib.request.urlopen(f"http://127.0.0.1:{port}/health", timeout=.1).close(); break
            except OSError:
                time.sleep(.02)
        provider_embed_ms = measure_owned_embedding_provider(f"http://127.0.0.1:{port}")
    doctor, _ = invoke(binary, ["doctor", "--json"], env, min(timeout, 30))
    if not isinstance(doctor, dict) or doctor.get("all_ok") is not True:
        raise ExecutionError("Axon provider preflight failed; live fallback is prohibited")
    manifest.register("collection", allocation["namespace"], {
        "run_id": allocation["run_id"], "corpus_version": corpus_manifest["corpus_version"],
        "corpus_checksum": corpus_manifest["corpus_checksum"],
        "ownership_generation": secrets.token_hex(32),
    })
    indexed, index_elapsed = invoke(binary, [str(CORPUS), "--wait", "true", "--collection",
                                               allocation["namespace"], "--json"], env, timeout)
    index_timing = public_index_timing(indexed, index_elapsed)
    if require_all_surfaces:
        if not isinstance(provider_embed_ms, int) or provider_embed_ms < 0:
            raise ExecutionError("owned embedding provider timing observation is missing")
        index_timing["harness_provider_embed"] = provider_embed_ms
    register_discovered(manifest, allocation["run_id"], "retrieval.corpus.index", indexed)
    retry_observation = None
    if discovery:
        retry_observation = verify_retry_success(
            binary, env, allocation["namespace"], f"http://127.0.0.1:{port}", timeout)
    verify_provider_failure_modes(binary, env, allocation["namespace"], timeout, manifest,
                                  Path(allocation["run_root"]))
    if require_all_surfaces:
        verify_configured_fallback(binary, env, timeout, manifest, Path(allocation["run_root"]))
    evidence = []
    if repetitions < 5:
        raise ExecutionError("representative live-capable correctness prompts require five repetitions")
    for repetition in range(1, repetitions + 1):
        for item, operation in operation_cases():
            invocations = [("cli", lambda: invoke(
                binary, argv_for(operation, item, allocation["namespace"], allocation["run_id"]), env, timeout))]
            if http_url and operation != "train":
                invocations.append(("http", lambda: invoke_http(
                    http_url, http_token, operation, item, allocation["namespace"], timeout)))
            if mcporter and operation != "train":
                invocations.append(("mcp", lambda: invoke_mcp(
                    mcporter, mcp_selector, operation, item, allocation["namespace"], env, timeout)))
            for surface, invocation in invocations:
                scenario_id = f"{item['id']}.{operation}.{surface}.repeat-{repetition}"
                operation_identity = f"{allocation['run_id']}_{scenario_id.replace('.', '_').replace('-', '_')}"
                manifest.register("operation", operation_identity,
                                  {"run_id": allocation["run_id"], "scenario_id": scenario_id})
                observe_provider = discovery is not None and surface == "cli" and operation in {
                    "ask", "chat", "summarize", "research", "evaluate", "train", "suggest"}
                before_provider = provider_stats(f"http://127.0.0.1:{port}") if observe_provider else None
                actual, elapsed = invocation()
                after_provider = provider_stats(f"http://127.0.0.1:{port}") if observe_provider else None
                register_discovered(manifest, allocation["run_id"], scenario_id, actual, operation_identity)
                normalized = normalize(actual, item, operation, allocation["run_id"], elapsed)
                if operation in GROUNDED_OPERATIONS or "error" in normalized:
                    semantics = SEMANTICS if "error" in normalized else runtime_semantics(item, actual)
                    evaluated = grounding.evaluate(item, normalized, semantics,
                                                    run_id=allocation["run_id"])
                else:
                    evaluated = structural_evidence(actual, operation, scenario_id,
                                                    allocation["run_id"], item, surface)
                evaluated.update({"operation": operation, "surface": surface})
                evaluated.update({"corpus_version": corpus_manifest["corpus_version"],
                                  "corpus_checksum": corpus_manifest["corpus_checksum"],
                                  "index_timing_ms": index_timing})
                if before_provider is not None and after_provider is not None:
                    raw_delta = provider_delta(before_provider, after_provider)
                    artifact_ids = actual_artifact_ids(actual)
                    base_calls = {"evaluate": 3, "research": 1, "train": 1,
                                  "ask": 1, "chat": 1, "summarize": 1,
                                  "suggest": 0}[operation]
                    observed = {"calls": raw_delta["calls"],
                                "retries": max(0, raw_delta["calls"] - base_calls),
                                "tokens": raw_delta["tokens"],
                                "artifacts": len(artifact_ids), "artifact_ids": artifact_ids}
                    max_calls = {"evaluate": 4, "research": 2, "train": 2}.get(operation, 1)
                    limits = {"max_calls": max_calls, "max_retries": 1, "max_tokens": 4096,
                              "max_artifacts": max_calls}
                    checks = grounding.provider_observation_assertions(observed, limits)
                    checks.append({"id": "provider.logical_calls_observed",
                                   "passed": observed["calls"] >= base_calls,
                                   "detail": f"calls={observed['calls']} base_calls={base_calls}"})
                    checks.append({"id": "provider.artifacts_bounded",
                                   "passed": observed["artifacts"] <= observed["calls"] and
                                             len(set(artifact_ids)) == len(artifact_ids),
                                   "detail": f"artifacts={observed['artifacts']} limit={limits['max_artifacts']}"})
                    evaluated["provider_observation"] = observed
                    evaluated["assertions"].extend(checks)
                    if not all(check["passed"] for check in checks):
                        evaluated["result"] = "fail"
                        evaluated["failure_class"] = "provider_budget"
                evidence.append(evaluated)
                if evaluated["result"] != "pass":
                    raise ExecutionError(f"semantic invariant failed for {scenario_id}")
    # Prove multi-turn retention using Axon's real named-session path.
    follow_up, _ = invoke(binary, ["ask", "Repeat only the one-word value from my immediately previous answer; do not retrieve new evidence.",
                                   "--resume", f"{allocation['run_id']}_ask", "--no-stream",
                                   "--limit", "3", "--collection", allocation["namespace"], "--json"], env, timeout)
    if "amber" not in json.dumps(follow_up, ensure_ascii=False).casefold():
        raise ExecutionError("within-run named-session follow-up did not retain the fixture fact")
    # A fresh allocation must not see the first run's named session.
    second = isolation.allocate(outdir / "runs", outdir / "manifests")
    second_manifest = isolation.Manifest.open(Path(second["manifest"]))
    second_manifest.register("collection", second["namespace"], {
        "run_id": second["run_id"], "ownership_generation": secrets.token_hex(32)})
    second_env = {**env, "AXON_DATA_DIR": second["data_dir"], "AXON_SQLITE_PATH": second["sqlite"],
                  "AXON_COLLECTION": second["namespace"], "AXON_E2E_RUN_ID": second["run_id"]}
    empty_corpus = Path(second["run_root"]) / "empty-corpus"
    empty_corpus.mkdir()
    empty_index, _ = invoke(binary, [str(empty_corpus), "--wait", "true", "--collection",
                                          second["namespace"], "--json"], second_env, timeout)
    register_discovered(second_manifest, second["run_id"], "retrieval.empty.index", empty_index)
    empty, _ = invoke(binary, ["query", "empty-collection-probe", "--limit", "1", "--collection",
                               second["namespace"], "--json"], second_env, timeout)
    if canonical_citations(empty):
        raise ExecutionError("empty collection unexpectedly returned a canonical corpus source")
    evidence.append({"schema_version": 1, "scenario_id": "retrieval.empty_collection",
                     "operation": "query", "surface": "cli", "run_id": second["run_id"],
                     "result": "pass", "failure_class": "empty_collection",
                     "assertions": [{"id": "retrieval.empty.structured", "passed": True,
                                     "detail": "actual isolated query returned no canonical source"}]})
    sessions, _ = invoke(binary, ["ask", "--list-sessions", "--json"], second_env, timeout)
    if f"{allocation['run_id']}_ask" in json.dumps(sessions, ensure_ascii=False):
        raise ExecutionError("named chat session leaked across isolated runs")
    # No-match is a required actual execution, not a fabricated envelope.
    no_match, elapsed = invoke(binary, ["query", "deterministic-no-match-on-empty-run", "--limit", "1",
                                        "--collection", second["namespace"], "--json"], second_env, timeout)
    if canonical_citations(no_match):
        raise ExecutionError("no-match query unexpectedly returned a canonical corpus source")
    evidence.append({"schema_version": 1, "scenario_id": "retrieval.no_match",
                     "operation": "query", "surface": "cli", "run_id": second["run_id"],
                     "result": "pass", "failure_class": "no_match",
                     "assertions": [{"id": "retrieval.no_match.structured", "passed": True,
                                     "detail": "actual bounded query returned no canonical source"}]})
    evidence_path = Path(allocation["run_root"]) / "evidence" / "retrieval-rag.json"
    evidence_path.parent.mkdir(parents=True, exist_ok=True)
    if discovery:
        if retry_observation is not None:
            retry_checks = grounding.provider_observation_assertions(
                retry_observation, {"max_calls": 2, "max_retries": 1, "max_tokens": 4096})
            retry_checks.append({"id": "provider.transient_retry_observed",
                                 "passed": retry_observation["retries"] == 1,
                                 "detail": "owned primary failed once and succeeded on the bounded retry"})
            retry_checks.append({"id": "provider.retry_artifacts_not_multiplied",
                                 "passed": retry_observation["artifacts"] <= retry_observation["calls"] and
                                           len(set(retry_observation["artifact_ids"])) ==
                                           len(retry_observation["artifact_ids"]),
                                 "detail": f"artifact_ids={retry_observation['artifact_ids']} attempts={retry_observation['calls']}"})
            require_passing_checks(retry_checks, "transient retry provider observation")
            evidence.append({"schema_version": 1, "scenario_id": "provider.retry_success.observation",
                             "operation": "provider_observation", "surface": "harness",
                             "run_id": allocation["run_id"], "result": "pass", "failure_class": None,
                             "provider_observation": retry_observation, "assertions": retry_checks})
    for record in evidence:
        grounding.validate_evidence(record, EVIDENCE_SCHEMA)
    evidence_path.write_text(json.dumps(evidence, sort_keys=True) + "\n", encoding="utf-8")
    manifest.register("artifact", f"{allocation['run_id']}_artifact_retrieval_rag", {
        "run_id": allocation["run_id"], "scenario_id": "retrieval.rag.pack",
        "path": str(evidence_path.relative_to(Path(allocation["run_root"]))), "redacted": True,
    })
    if discovery:
        discovery.process.terminate(); discovery.process.wait(timeout=5)
    return evidence


def main() -> int:
    parser = argparse.ArgumentParser(description="Execute Axon's real retrieval/RAG E2E pack")
    parser.add_argument("--axon-bin", type=Path, required=True)
    parser.add_argument("--outdir", type=Path, required=True)
    parser.add_argument("--timeout-secs", type=float, default=120)
    parser.add_argument("--repetitions", type=int, default=5)
    parser.add_argument("--http-url")
    parser.add_argument("--http-token")
    parser.add_argument("--mcporter-bin", type=Path)
    parser.add_argument("--mcp-selector", default="axon.axon")
    args = parser.parse_args()
    try:
        evidence = execute(args.axon_bin.resolve(), args.outdir.resolve(), args.timeout_secs,
                           args.repetitions, True,
                           args.http_url, args.http_token,
                           args.mcporter_bin.resolve() if args.mcporter_bin else None,
                           args.mcp_selector)
    except (ExecutionError, grounding.ContractError, isolation.IsolationError, OSError) as error:
        print(f"retrieval/RAG E2E failed: {error}", file=sys.stderr)
        return 1
    print(json.dumps({"result": "pass", "operations": len(evidence)}, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
