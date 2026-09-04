#!/usr/bin/env python3
"""Auditable semantic oracle for retrieval and grounded synthesis E2E results.

The oracle intentionally accepts a small, transport-neutral response contract.
Adapters remain responsible for translating CLI, MCP, and HTTP responses into it.
It never compares generated prose exactly.
"""

from __future__ import annotations

import json
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any


PROVIDER_FAILURES = {
    "provider.unavailable",
    "provider.timeout",
    "provider.scheduler.queue_full",
    "provider.malformed_response",
    "embedding.tei.dimension_mismatch",
    "provider.schema_mismatch",
    "provider.token_limit",
}
TIMING_FIELDS = {
    "ask": ("retrieval", "context_build", "llm", "total"),
    "evaluate": ("retrieval", "context_build", "rag_llm", "baseline_llm",
                 "research_elapsed_ms", "analysis_llm_ms", "total"),
    "summarize": ("scrape", "llm", "total"),
    "research": ("total",),
}


class ContractError(ValueError):
    """The response cannot be treated as a valid E2E outcome."""


@dataclass(frozen=True)
class Assertion:
    id: str
    passed: bool
    detail: str

    def as_dict(self) -> dict[str, Any]:
        return {"id": self.id, "passed": self.passed, "detail": self.detail}


def provider_observation_assertions(usage: Any, limits: dict[str, int]) -> list[dict[str, Any]]:
    """Validate counters observed at an owned provider, never a product DTO."""
    values = usage if isinstance(usage, dict) else {}
    return [Assertion(f"provider.{field}_bounded",
                      isinstance(values.get(field), int) and
                      0 <= values[field] <= limits[f"max_{field}"],
                      f"{field}={values.get(field)!r} limit={limits[f'max_{field}']}").as_dict()
            for field in ("calls", "retries", "tokens")]


def load_json(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ContractError(f"{path} must contain a JSON object")
    return value


def validate_evidence(record: Any, schema: dict[str, Any]) -> None:
    """Apply the checked-in evidence schema without an optional dependency."""
    def check(value: Any, rule: dict[str, Any], path: str) -> None:
        expected = rule.get("type")
        allowed = expected if isinstance(expected, list) else [expected] if expected else []
        matches = {"object": lambda: isinstance(value, dict),
                   "array": lambda: isinstance(value, list),
                   "string": lambda: isinstance(value, str),
                   "integer": lambda: isinstance(value, int) and not isinstance(value, bool),
                   "number": lambda: isinstance(value, (int, float)) and not isinstance(value, bool),
                   "boolean": lambda: isinstance(value, bool), "null": lambda: value is None}
        if allowed and not any(matches[kind]() for kind in allowed):
            raise ContractError(f"{path} does not match schema type {expected!r}")
        if isinstance(value, dict):
            missing = set(rule.get("required", [])) - value.keys()
            if missing:
                raise ContractError(f"{path} missing schema fields: {sorted(missing)}")
            properties = rule.get("properties", {})
            for key, child in value.items():
                child_rule = properties.get(key, rule.get("additionalProperties"))
                if isinstance(child_rule, dict):
                    check(child, child_rule, f"{path}.{key}")
        if isinstance(value, list) and isinstance(rule.get("items"), dict):
            for index, child in enumerate(value):
                check(child, rule["items"], f"{path}[{index}]")
        if "minimum" in rule and value < rule["minimum"]:
            raise ContractError(f"{path} is below schema minimum")
        if "enum" in rule and value not in rule["enum"]:
            raise ContractError(f"{path} is outside schema enum")
        if isinstance(value, str) and "pattern" in rule and not re.search(rule["pattern"], value):
            raise ContractError(f"{path} does not match schema pattern")
    check(record, schema, "evidence")


def _string_set(value: Any, field: str) -> set[str]:
    if not isinstance(value, list) or any(not isinstance(item, str) for item in value):
        raise ContractError(f"{field} must be an array of strings")
    return set(value)


def validate_scenario(scenario: dict[str, Any]) -> None:
    required = {"id", "operation", "prompt", "expected_fact_ids", "max_results"}
    missing = sorted(required - scenario.keys())
    if missing:
        raise ContractError(f"scenario missing fields: {', '.join(missing)}")
    if not isinstance(scenario["max_results"], int) or not 1 <= scenario["max_results"] <= 20:
        raise ContractError("max_results must be between 1 and 20")
    if not 5 <= len(scenario["prompt"].split()) <= 80:
        raise ContractError("correctness prompt must contain 5-80 words")
    _string_set(scenario["expected_fact_ids"], "expected_fact_ids")


def evaluate(
    scenario: dict[str, Any],
    response: dict[str, Any],
    semantics: dict[str, Any],
    *,
    run_id: str,
) -> dict[str, Any]:
    """Evaluate one normalized result and return a stable evidence envelope."""
    validate_scenario(scenario)
    assertions: list[Assertion] = []
    error = response.get("error")
    if error is not None:
        code = error.get("code") if isinstance(error, dict) else None
        classified = code in PROVIDER_FAILURES or str(code).startswith(("retrieval.", "llm."))
        assertions.append(Assertion("failure.classified", classified, f"error code={code!r}"))
        assertions.append(Assertion("provider.error_not_pass", False, "provider error envelopes never pass"))
        return _envelope(scenario, response, run_id, assertions, "fail", str(code or "unclassified"))

    response_run = response.get("run_id")
    assertions.append(Assertion("run.isolated", response_run == run_id, f"response run={response_run!r}"))
    results = response.get("results", [])
    if not isinstance(results, list):
        raise ContractError("results must be an array")
    assertions.append(Assertion("retrieval.bounded", len(results) <= scenario["max_results"],
                                f"{len(results)} <= {scenario['max_results']}"))

    corpus_sources = {fact["source_id"] for fact in semantics.get("facts", [])}
    result_sources = {item.get("source_id") for item in results if isinstance(item, dict)}
    expected_sources = {
        fact["source_id"] for fact in semantics.get("facts", [])
        if fact.get("id") in set(scenario["expected_fact_ids"])
    }
    assertions.append(Assertion("retrieval.expected_sources", expected_sources <= result_sources,
                                f"expected={sorted(expected_sources)} actual={sorted(str(x) for x in result_sources)}"))
    assertions.append(Assertion("retrieval.run_corpus_only", result_sources <= corpus_sources,
                                "all results belong to the versioned run corpus"))

    answer = response.get("answer", "")
    if not isinstance(answer, str):
        raise ContractError("answer must be a string")
    facts = {fact["id"]: fact for fact in semantics.get("facts", [])}
    expected_values = [str(facts[fact_id]["value"]) for fact_id in scenario["expected_fact_ids"]]
    assertions.append(Assertion("grounding.expected_facts", all(value.casefold() in answer.casefold() for value in expected_values),
                                f"expected fixture values={expected_values}"))
    forbidden = [str(value) for value in scenario.get("forbidden_answers", [])]
    assertions.append(Assertion("grounding.no_distractor", not any(value.casefold() in answer.casefold() for value in forbidden),
                                f"forbidden fixture values={forbidden}"))
    hostile_markers = [str(value) for value in scenario.get("hostile_markers", [])]
    assertions.append(Assertion("grounding.prompt_injection_ignored",
                                not any(value.casefold() in answer.casefold() for value in hostile_markers),
                                "hostile corpus instructions were not followed"))

    citations = response.get("citations", [])
    if not isinstance(citations, list):
        raise ContractError("citations must be an array")
    citation_contract = {item["id"]: item for item in semantics.get("citations", [])}
    citation_ids = {item.get("id") for item in citations if isinstance(item, dict)}
    citation_sources = {item.get("source_id") for item in citations if isinstance(item, dict)}
    citation_excerpts = {item.get("id"): item.get("excerpt", "") for item in citations
                         if isinstance(item, dict) and isinstance(item.get("id"), str)}
    required_citations = {
        facts[fact_id].get("citation") for fact_id in scenario["expected_fact_ids"]
        if facts[fact_id].get("citation")
    }
    assertions.append(Assertion("citation.membership", citation_sources <= result_sources,
                                "every citation resolves to a retrieved source"))
    assertions.append(Assertion("citation.supports_fact", required_citations <= citation_ids and
                                required_citations <= citation_contract.keys() and
                                all(citation_contract[cid]["source_id"] in citation_sources and
                                    citation_contract[cid]["contains"].casefold() in
                                    str(citation_excerpts.get(cid, "")).casefold()
                                    for cid in required_citations),
                                f"required citations={sorted(required_citations)}"))

    # Provider call/retry/token budgets are harness observations from owned
    # doubles. Query/Ask/Evaluate DTOs intentionally do not expose them.
    usage = response.get("provider_usage")
    calls = usage.get("calls") if isinstance(usage, dict) else None
    artifacts = response.get("artifacts", [])
    if artifacts is not None:
        assertions.append(Assertion("provider.artifacts_not_multiplied", isinstance(artifacts, list) and
                                    (calls is None or len(artifacts) <= calls),
                                    f"artifacts={len(artifacts) if isinstance(artifacts, list) else 'invalid'} calls={calls}"))

    timing = response.get("timing_ms", {})
    operation = response.get("operation", scenario["operation"])
    required_timing = TIMING_FIELDS.get(operation, ())
    if required_timing:
        timing_ok = isinstance(timing, dict) and all(
            isinstance(timing.get(bucket), (int, float)) and timing[bucket] >= 0 for bucket in required_timing
        )
        assertions.append(Assertion("timing.public_fields", timing_ok, f"required={list(required_timing)}"))
    passed = all(item.passed for item in assertions)
    return _envelope(scenario, response, run_id, assertions, "pass" if passed else "fail",
                     None if passed else "semantic_invariant")


def _envelope(scenario: dict[str, Any], response: dict[str, Any], run_id: str,
              assertions: list[Assertion], result: str, failure: str | None) -> dict[str, Any]:
    return {
        "schema_version": 1,
        "scenario_id": scenario["id"],
        "operation": scenario["operation"],
        "run_id": run_id,
        "result": result,
        "failure_class": failure,
        "assertions": [item.as_dict() for item in assertions],
        "timing_ms": response.get("timing_ms", {}),
        "provider_usage": response.get("provider_usage", {}),
    }
