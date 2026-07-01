//! LLM streaming DTOs.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmDelta {
    pub text: String,
}

pub type LlmDeltaSink<'a> = &'a mut (dyn FnMut(LlmDelta) + Send);
