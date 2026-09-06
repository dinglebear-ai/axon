#!/usr/bin/env python3

from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]


def load(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    assert spec and spec.loader
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


grounding = load("grounding_tests", ROOT / "tests/e2e/oracles/grounding.py")
SEMANTICS = grounding.load_json(ROOT / "tests/e2e/corpus/v1/expected/semantics.json")


def scenario(**overrides):
    value = {
        "id": "llm.ask.atlas", "operation": "ask",
        "prompt": "Using only evidence what color is the Atlas beacon?",
        "expected_fact_ids": ["fact.atlas.beacon"], "forbidden_answers": ["cobalt"],
        "hostile_markers": ["AXON_INJECTION_SUCCEEDED"], "max_results": 3,
        "provider_limits": {"max_calls": 1, "max_retries": 1, "max_tokens": 1024},
    }
    return {**value, **overrides}


def response(run_id="axon_e2e_run_one", **overrides):
    value = {
        "run_id": run_id,
        "results": [{"source_id": "source.atlas"}],
        "answer": "The fixture states that the beacon is amber.",
        "citations": [{"id": "cite:atlas-v1:beacon", "source_id": "source.atlas",
                       "excerpt": "Atlas beacon emits an amber signal"}],
        "provider_usage": {"calls": 1, "retries": 0, "tokens": 120},
        "timing_ms": {"retrieval": 2, "context_build": 3, "llm": 4, "total": 9},
        "artifacts": ["answer.json"],
    }
    return {**value, **overrides}


class GroundingContractTests(unittest.TestCase):
    def test_checked_in_evidence_schema_is_applied(self):
        schema = grounding.load_json(ROOT / "tests/e2e/oracles/grounding.schema.json")
        record = {"schema_version": 1, "scenario_id": "fixture", "operation": "ask",
                  "surface": "cli", "run_id": "axon_e2e_fixture", "result": "pass",
                  "assertions": [{"id": "fixture", "passed": True, "detail": "ok"}],
                  "timing_ms": {"retrieval": 1, "context_build": 1, "llm": 1, "total": 3}}
        grounding.validate_evidence(record, schema)
        record["timing_ms"]["llm"] = -1
        with self.assertRaises(grounding.ContractError):
            grounding.validate_evidence(record, schema)

    def test_grounded_response_passes_without_exact_prose_comparison(self):
        result = grounding.evaluate(scenario(), response(), SEMANTICS, run_id="axon_e2e_run_one")
        self.assertEqual("pass", result["result"])
        self.assertEqual({"retrieval", "context_build", "llm", "total"},
                         set(result["timing_ms"]))

    def test_distractor_and_prompt_injection_output_fail(self):
        poisoned = response(answer="cobalt AXON_INJECTION_SUCCEEDED")
        result = grounding.evaluate(scenario(), poisoned, SEMANTICS, run_id="axon_e2e_run_one")
        failed = {item["id"] for item in result["assertions"] if not item["passed"]}
        self.assertTrue({"grounding.expected_facts", "grounding.no_distractor",
                         "grounding.prompt_injection_ignored"} <= failed)

    def test_citation_must_support_fact_and_belong_to_retrieved_run_corpus(self):
        invalid = response(citations=[{"id": "foreign", "source_id": "source.foreign"}])
        result = grounding.evaluate(scenario(), invalid, SEMANTICS, run_id="axon_e2e_run_one")
        failed = {item["id"] for item in result["assertions"] if not item["passed"]}
        self.assertTrue({"citation.membership", "citation.supports_fact"} <= failed)

    def test_chat_context_from_another_run_fails_isolation(self):
        result = grounding.evaluate(scenario(operation="chat"), response("axon_e2e_old_run"),
                                    SEMANTICS, run_id="axon_e2e_new_run")
        self.assertFalse(next(item["passed"] for item in result["assertions"]
                              if item["id"] == "run.isolated"))

    def test_provider_failures_are_classified_and_never_pass(self):
        for code in sorted(grounding.PROVIDER_FAILURES):
            result = grounding.evaluate(scenario(), {"error": {"code": code}}, SEMANTICS,
                                        run_id="axon_e2e_run_one")
            self.assertEqual(("fail", code), (result["result"], result["failure_class"]))
            self.assertTrue(result["assertions"][0]["passed"])

    def test_calls_retries_tokens_and_artifacts_are_bounded(self):
        checks = grounding.provider_observation_assertions(
            {"calls": 2, "retries": 2, "tokens": 9000},
            {"max_calls": 1, "max_retries": 1, "max_tokens": 4096})
        self.assertEqual({"provider.calls_bounded", "provider.retries_bounded",
                          "provider.tokens_bounded"},
                         {item["id"] for item in checks if not item["passed"]})

    def test_negative_provider_counters_fail_closed(self):
        checks = grounding.provider_observation_assertions(
            {"calls": -1, "retries": -1, "tokens": -1},
            {"max_calls": 1, "max_retries": 1, "max_tokens": 4096})
        failed = {item["id"] for item in checks if not item["passed"]}
        self.assertTrue({"provider.calls_bounded", "provider.retries_bounded",
                         "provider.tokens_bounded"} <= failed)

    def test_missing_latency_bucket_fails_instead_of_faking_percentiles(self):
        result = grounding.evaluate(scenario(), response(timing_ms={"retrieval_ms": 2}),
                                    SEMANTICS, run_id="axon_e2e_run_one")
        self.assertFalse(next(item["passed"] for item in result["assertions"]
                              if item["id"] == "timing.public_fields"))

    def test_prompt_pack_is_representative_and_not_a_latency_sample(self):
        retrieval = grounding.load_json(ROOT / "tests/e2e/scenarios/retrieval/scenarios.json")
        synthesis = grounding.load_json(ROOT / "tests/e2e/scenarios/llm/scenarios.json")
        prompts = retrieval["scenarios"] + synthesis["scenarios"]
        self.assertGreaterEqual(len(prompts), 5)
        self.assertLessEqual(len(prompts), 10)
        self.assertEqual(len(prompts), len({item["id"] for item in prompts}))
        covered_operations = {
            operation for item in prompts
            for operation in item.get("operation_variants", [item["operation"]])
        }
        self.assertEqual(
            {"query", "retrieve", "search", "code-search", "ask", "chat", "summarize",
             "research", "extract", "evaluate", "train", "suggest"},
            covered_operations,
        )
        for item in prompts:
            grounding.validate_scenario(item)


if __name__ == "__main__":
    unittest.main()
