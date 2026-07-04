//! LLM streaming DTO re-exports.

pub use axon_api::source::LlmDelta;

pub type LlmDeltaSink<'a> = &'a mut (dyn FnMut(LlmDelta) + Send);
