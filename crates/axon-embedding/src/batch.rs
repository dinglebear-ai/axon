//! Embedding batch construction and validation helpers.

use std::collections::BTreeSet;

use axon_api::source::*;
use axon_error::ErrorStage;

use crate::provider::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddingBatchValidation {
    pub item_count: usize,
    pub content_kinds: Vec<ContentKind>,
}

#[derive(Debug, Clone)]
pub struct EmbeddingBatchBuilder {
    batch_id: BatchId,
    job_id: JobId,
    provider_id: ProviderId,
    model: String,
    items: Vec<EmbeddingInput>,
    instruction: Option<String>,
    priority: JobPriority,
    metadata: MetadataMap,
}

impl EmbeddingBatchBuilder {
    pub fn new(
        batch_id: BatchId,
        job_id: JobId,
        provider_id: ProviderId,
        model: impl Into<String>,
    ) -> Self {
        Self {
            batch_id,
            job_id,
            provider_id,
            model: model.into(),
            items: Vec::new(),
            instruction: None,
            priority: JobPriority::Background,
            metadata: MetadataMap::new(),
        }
    }

    pub fn priority(mut self, priority: JobPriority) -> Self {
        self.priority = priority;
        self
    }

    pub fn instruction(mut self, instruction: impl Into<String>) -> Self {
        self.instruction = Some(instruction.into());
        self
    }

    pub fn push_input(mut self, input: EmbeddingInput) -> Self {
        self.items.push(input);
        self
    }

    pub fn build(self) -> Result<EmbeddingBatch> {
        let batch = EmbeddingBatch {
            batch_id: self.batch_id,
            job_id: self.job_id,
            provider_id: self.provider_id,
            model: self.model,
            items: self.items,
            instruction: self.instruction,
            priority: self.priority,
            metadata: self.metadata,
        };
        validate_batch(&batch)?;
        Ok(batch)
    }
}

pub fn validate_batch(batch: &EmbeddingBatch) -> Result<EmbeddingBatchValidation> {
    if batch.items.is_empty() {
        return Err(error(
            "embedding.batch_empty",
            &batch.provider_id,
            "embedding batch must contain at least one input",
            None,
        ));
    }

    let mut chunk_ids = BTreeSet::new();
    for item in &batch.items {
        if item.text.trim().is_empty() {
            return Err(error(
                "embedding.blank_text",
                &batch.provider_id,
                "embedding input text must not be blank",
                Some(&item.chunk_id),
            ));
        }
        if !chunk_ids.insert(item.chunk_id.clone()) {
            return Err(error(
                "embedding.duplicate_chunk_id",
                &batch.provider_id,
                "embedding batch contains duplicate chunk ids",
                Some(&item.chunk_id),
            ));
        }
    }

    Ok(EmbeddingBatchValidation {
        item_count: batch.items.len(),
        content_kinds: batch.items.iter().map(|item| item.content_kind).collect(),
    })
}

fn error(
    code: &str,
    provider_id: &ProviderId,
    message: &str,
    chunk_id: Option<&ChunkId>,
) -> ApiError {
    let mut error =
        ApiError::new(code, ErrorStage::Embedding, message).with_provider_id(provider_id.0.clone());
    if let Some(chunk_id) = chunk_id {
        error.chunk_id = Some(chunk_id.0.clone());
    }
    error
}
