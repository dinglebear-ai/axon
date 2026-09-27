# axon-llm Agent Instructions

This file is the agent-facing contract for the `axon-llm` crate docs.

## When Editing

- Keep `LlmProvider`, model capabilities, prompts, structured output,
  streaming, and provider implementations here.
- Do not add retrieval planning, vector storage, source acquisition, or
  transport route ownership.
- Update `README.md`, `../../runtime/provider-contract.md`, configuration docs,
  and capability schemas together.
- Preserve provider fakes for synthesis, extraction, judging, and streaming.

## Review Checklist

- Provider capabilities are queryable before execution.
- Prompt diagnostics are redaction-safe.
- Structured output failure returns typed errors/degradation.
