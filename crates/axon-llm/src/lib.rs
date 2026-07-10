//! `axon-llm` — the LLM provider boundary.
//!
//! The PR0 skeleton (trait-based `provider`/`fake`/`completion`/`stream`
//! markers) coexists with the real completion backends relocated here from
//! `axon-core::llm` (see [`runtime`]). The DTO/config types the backends operate
//! on remain in `axon-core::llm` because they are embedded in `Config`; this
//! crate re-exports them so callers use a single `axon_llm::…` surface.

pub mod capability;
pub mod codex;
pub mod completion;
pub mod fake;
pub mod gemini;
pub mod openai_compat;
pub mod prompt;
pub mod provider;
pub mod reservation;
pub mod stream;
pub mod testing;

/// Real LLM completion backends (Gemini headless, Codex app-server,
/// OpenAI-compatible) plus the backend-selection dispatch and per-backend
/// completion concurrency limiter. Relocated here from `axon-core::llm` so the
/// provider implementations live behind the LLM boundary; the DTO/config types
/// they operate on remain in `axon-core::llm` (embedded in `Config`).
pub mod runtime;

// Flat re-exports so callers use a single `axon_llm::…` surface instead of
// reaching into `axon_llm::runtime::…`. These mirror the old `axon_core::llm`
// public API 1:1 so the move is transparent to callers.
pub use runtime::completer::{BackendTextCompleter, backend_text_completer};
pub use runtime::doctor_probe::build_llm_doctor_probe;
pub use runtime::{
    CompletionRequest, CompletionResponse, CompletionRunner, CompletionTurnResult,
    LlmBackendConfig, LlmBackendKind, LlmModelPurpose, ReasoningEffort, SynthesisModelProfile,
    SynthesisModelTier, UsageSnapshot, complete_streaming, complete_streaming_with_runner,
    complete_text, complete_text_with_runner, configured_chat_model_from_config,
    configured_model_for_config, configured_model_from_config, extract_completion_result,
    normalize_stream_flag,
};

pub const CRATE_NAME: &str = "axon-llm";
