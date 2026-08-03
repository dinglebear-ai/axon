# axon-llm Crate Contract
Last Modified: 2026-06-30

## Purpose

`axon-llm` owns LLM provider boundaries for synthesis, structured extraction,
judging, enrichment, query rewriting, and streaming completion.

## Owns

- `LlmProvider` trait and provider implementations
- provider capabilities, model metadata, context windows, tool support, and
  streaming support
- OpenAI-compatible, Codex app-server, Gemini CLI/headless, and fake providers
- prompt request/response normalization and redacted diagnostics

## Must Not Own

- retrieval planning, source acquisition, parsing, vector storage, or job
  scheduling
- transport-specific streaming routes
- extraction command semantics beyond provider execution primitives

## Public Modules

```text
lib.rs
provider.rs
capability.rs
completion.rs
stream.rs
prompt.rs
openai_compat.rs
codex.rs
gemini.rs
fake.rs
testing.rs
```

## Public API

- `LlmProvider`
- `LlmRequest`
- `LlmResponse`
- `LlmStream`
- `LlmCapability`
- `LlmModelInfo`
- `PromptInput`
- `StructuredOutputRequest`
- `FakeLlmProvider`

## Dependencies Allowed

- `axon-api`, `axon-error`, `axon-core`, `axon-observe`
- provider HTTP/process crates hidden behind provider modules

## Dependencies Forbidden

- retrieval engine implementation
- vector store implementation
- transport crates

## Generated Artifacts

- provider capability schema entries
- config schema for LLM provider selection
- fake provider fixtures for ask/extract/evaluate tests

## Fixtures And Fakes

- deterministic completion fixture
- streaming response fixture
- malformed structured output fixture
- provider timeout and rate-limit fixtures

## Tests

- provider capabilities are surfaced before execution
- redacted prompts/logs never leak secrets
- structured output validates or returns typed degradation
- fake provider supports sync and streaming test paths

## Acceptance Criteria

- all LLM behavior crosses `LlmProvider`
- adding a provider does not touch retrieval, CLI, MCP, or REST logic
- provider failures are observable and classed by `axon-error`

See [../README.md](../README.md) and
[../../runtime/provider-contract.md](../../runtime/provider-contract.md).
