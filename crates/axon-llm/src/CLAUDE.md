# axon-llm — Agent Guide

`axon-llm` owns the **LLM provider boundary** for synthesis, structured
extraction, judging, enrichment, query rewriting, and streaming completion: the
`LlmProvider` trait plus OpenAI-compatible, Codex app-server, and Gemini
CLI/headless implementations. All LLM behavior crosses this trait. Full contract
(owns / API / deps / tests):
[../../../docs/pipeline-unification/crates/axon-llm/README.md](../../../docs/pipeline-unification/crates/axon-llm/README.md)
· behavior spec:
[../../../docs/pipeline-unification/runtime/provider-contract.md](../../../docs/pipeline-unification/runtime/provider-contract.md).

## Status — real backends landed (#298 p7uwm)
The **real LLM completion backends** now live here (relocated from
`axon-core::llm`): Gemini headless, Codex app-server, OpenAI-compatible, the
backend-selection dispatch, and the per-backend completion concurrency limiter.
They live under [`runtime`] and are re-exported flat at the crate root so callers
use a single `axon_llm::…` surface (`complete_text`, `complete_streaming`,
`CompletionRequest`, `LlmBackendKind`, `configured_model_from_config`, …).

The PR0 trait skeleton (`provider.rs`/`capability.rs`/`completion.rs`/`stream.rs`/
`prompt.rs`/`fake.rs`/`testing.rs`) is the **future target contract** and coexists
with the runtime backends until callers migrate onto the `LlmProvider` trait.

The LLM **DTO/config types** the backends operate on (`CompletionRequest`,
`LlmBackendConfig`, `LlmBackendKind`, `SynthesisModelProfile`, `configured_model_*`,
`CompletionRunner`/`TextCompleter`) intentionally stay in `axon-core::llm` — they
are embedded in `Config`, so moving them would cycle (`axon-llm` → `axon-core`).
This crate re-exports them.

## Module map
| File | Owns |
|---|---|
| `runtime.rs` + `runtime/` | **Real backends**: `runtime/headless/` (Gemini CLI), `runtime/codex_app_server/` (Codex app-server), `runtime/openai_compat.rs`, `runtime/concurrency.rs` (completion limiter), plus `complete_text`/`complete_streaming` dispatch. `runtime/completer.rs` = `BackendTextCompleter` (`axon_core::TextCompleter` impl for injection). `runtime/doctor_probe.rs` = the doctor's LLM legs (`build_llm_doctor_probe`). |
| `provider.rs` | `LlmProvider` trait — the target boundary all LLM callers will use |
| `capability.rs` | `LlmCapability`, `LlmModelInfo` — context windows, tool/streaming support, model metadata |
| `completion.rs` | `LlmRequest`/`LlmResponse` + `StructuredOutputRequest` normalization |
| `stream.rs` | `LlmStream` — streaming completion deltas |
| `prompt.rs` | `PromptInput` request/response normalization + redacted diagnostics |
| `openai_compat.rs` | OpenAI-compatible provider (target trait marker; runtime impl is `runtime/openai_compat.rs`) |
| `codex.rs` | Codex app-server provider (target trait marker; runtime impl is `runtime/codex_app_server/`) |
| `gemini.rs` | Gemini CLI/headless provider (target trait marker; runtime impl is `runtime/headless/`) |
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
