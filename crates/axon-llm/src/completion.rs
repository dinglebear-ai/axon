//! LLM completion DTOs.

use axon_api::source::{JobPriority, MetadataMap, ProviderUsage};

#[derive(Debug, Clone, PartialEq)]
pub struct LlmCompletionRequest {
    pub prompt: String,
    pub system_prompt: Option<String>,
    pub model: Option<String>,
    pub max_output_tokens: Option<u32>,
    pub json_schema: Option<serde_json::Value>,
    pub priority: JobPriority,
    pub metadata: MetadataMap,
}

impl LlmCompletionRequest {
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            system_prompt: None,
            model: None,
            max_output_tokens: None,
            json_schema: None,
            priority: JobPriority::Normal,
            metadata: MetadataMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LlmCompletionResponse {
    pub text: String,
    pub model: String,
    pub finish_reason: String,
    pub usage: ProviderUsage,
    pub structured: Option<serde_json::Value>,
}
