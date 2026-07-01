# axon-llm — Agent Guide

`axon-llm` owns the **LLM provider boundary** for synthesis, structured
extraction, judging, enrichment, query rewriting, and streaming completion: the
`LlmProvider` trait plus OpenAI-compatible, Codex app-server, and Gemini
CLI/headless implementations. All LLM behavior crosses this trait. Full contract
(owns / API / deps / tests):
[../../../docs/pipeline-unification/crates/axon-llm/README.md](../../../docs/pipeline-unification/crates/axon-llm/README.md)
· behavior spec:
[../../../docs/pipeline-unification/runtime/provider-contract.md](../../../docs/pipeline-unification/runtime/provider-contract.md).

## Status — PR0 skeleton
Modules below are **markers only**. Real implementation lands in **Phase 7**,
decomposed out of `axon-vector`/`axon-extract`'s LLM logic. Do not add retrieval
planning, vector storage, source acquisition, or transport routes here.

## Module map
| File | Owns |
|---|---|
| `provider.rs` | `LlmProvider` trait — the boundary all LLM callers use |
| `capability.rs` | `LlmCapability`, `LlmModelInfo` — context windows, tool/streaming support, model metadata |
| `completion.rs` | `LlmRequest`/`LlmResponse` + `StructuredOutputRequest` normalization |
| `stream.rs` | `LlmStream` — streaming completion deltas |
| `prompt.rs` | `PromptInput` request/response normalization + redacted diagnostics |
| `openai_compat.rs` | OpenAI-compatible provider |
| `codex.rs` | Codex app-server provider |
| `gemini.rs` | Gemini CLI/headless provider |
| `fake.rs` / `testing.rs` | `FakeLlmProvider` (sync + streaming) + timeout/rate-limit/malformed-output fixtures |

## Boundary — keep OUT of this crate
- Retrieval planning, source acquisition, parsing, vector storage, job scheduling.
- Transport-specific streaming routes.
- Extraction command semantics beyond provider execution primitives.

## Dependencies
- **Allowed:** `axon-api`, `axon-error`, `axon-core`, `axon-observe`, provider HTTP/process crates hidden behind provider modules.
- **Forbidden:** retrieval engine implementation, vector store implementation, transport crates. Enforced by `cargo xtask check-layering`.

## Invariants (review checklist)
- Provider capabilities are **queryable/surfaced before execution**.
- Redacted prompts/logs **never leak secrets**.
- Structured output **validates or returns typed degradation**.
- Provider failures are **observable and classed by `axon-error`**.
- Adding a provider **does not touch** retrieval, CLI, MCP, or REST logic.

## DTO ownership
Serializable wire shapes (`LlmRequest`, `LlmResponse`, `LlmCapability`,
`LlmModelInfo`, `StructuredOutputRequest`, streaming deltas) are defined in
**`axon-api`**; this crate produces and returns them — it does not redefine
transport-facing shapes.

## Keep in sync when shapes change
`README.md` (crate contract) · `runtime/provider-contract.md` · LLM provider
configuration docs · the LLM capability/request/response DTO components in
`axon-api`.
