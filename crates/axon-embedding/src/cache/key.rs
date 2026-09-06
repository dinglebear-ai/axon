//! Stable cache-key derivation.

use axon_api::source::{EmbeddingBatch, EmbeddingInput, InstructionSupport, ProviderId};
use sha2::{Digest, Sha256};

const CACHE_KEY_VERSION: &str = "embedding-vector-cache-v1";

pub(super) fn cache_key(
    authority: &str,
    provider_id: &ProviderId,
    model: &str,
    dimensions: u32,
    instruction_support: InstructionSupport,
    batch: &EmbeddingBatch,
    input: &EmbeddingInput,
) -> String {
    let effective_instruction = match batch.instruction.as_deref() {
        Some(instruction)
            if !instruction.is_empty() && instruction_support != InstructionSupport::None =>
        {
            instruction
        }
        _ => "",
    };
    let mut hasher = Sha256::new();
    for part in [
        CACHE_KEY_VERSION.as_bytes(),
        authority.as_bytes(),
        provider_id.0.as_bytes(),
        model.as_bytes(),
        &dimensions.to_le_bytes(),
        effective_instruction.as_bytes(),
        serde_json::to_string(&input.content_kind)
            .expect("content kind serializes")
            .as_bytes(),
        input.text.as_bytes(),
    ] {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part);
    }
    format!("sha256:{}", hex::encode(hasher.finalize()))
}
