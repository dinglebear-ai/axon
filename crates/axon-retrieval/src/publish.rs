//! Generation publish boundary (`GenerationPublisher`).
//!
//! Contract: `docs/pipeline-unification/foundation/types/trait-contract.md`
//! §GenerationPublisher (grouped with `RetrievalEngine` under "Retrieval and
//! Publish Traits" — both traits describe the read/write ends of the same
//! generation-scoped vector data this crate already owns).
//!
//! Nothing in `axon-retrieval` (or elsewhere in this crate's territory)
//! performs a `publishing` pipeline stage
//! (`docs/pipeline-unification/foundation/source-pipeline.md`) today — the
//! real implementation is expected to sit on top of `axon-ledger`'s
//! `LedgerStore::commit_generation`/`SourceGeneration` state once the
//! runtime cutover lands (issue #298). This file is therefore purely
//! additive: it introduces the trait and a self-contained, in-process
//! `InMemoryGenerationPublisher` together, so the contract has a concrete,
//! non-panicking owner in this round without reaching into another
//! workstream's territory (`axon-ledger`) or changing any existing struct's
//! behavior.

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use axon_api::source::{
    ApiError, PublishGenerationRequest, PublishGenerationResult, PublishPlan, SourceGenerationId,
    SourceId, SourceWarning, Timestamp,
};
use axon_error::ErrorStage;
use chrono::Utc;

pub type Result<T> = std::result::Result<T, ApiError>;

#[async_trait]
pub trait GenerationPublisher: Send + Sync {
    async fn validate_publish(&self, request: PublishGenerationRequest) -> Result<PublishPlan>;
    async fn publish_generation(
        &self,
        request: PublishGenerationRequest,
    ) -> Result<PublishGenerationResult>;
}

/// Minimal production `GenerationPublisher`: tracks the last-committed
/// generation per [`SourceId`] in an in-process map guarded by a `Mutex`.
///
/// This is a legitimate, always-consistent implementation of the contract
/// shape (not a test fake — see [`crate::testing::FakeGenerationPublisher`]
/// for the deterministic-mode test double), but it is **not** durable: state
/// does not survive a process restart and is not shared across workers. It
/// exists so the `publishing` stage has a concrete owner while the
/// ledger-backed implementation lands in a later round.
#[derive(Debug, Default)]
pub struct InMemoryGenerationPublisher {
    committed: Mutex<HashMap<SourceId, SourceGenerationId>>,
}

impl InMemoryGenerationPublisher {
    pub fn new() -> Self {
        Self {
            committed: Mutex::new(HashMap::new()),
        }
    }

    fn current_generation(&self, source_id: &SourceId) -> Option<SourceGenerationId> {
        self.committed
            .lock()
            .expect("in-memory generation publisher mutex poisoned")
            .get(source_id)
            .cloned()
    }

    fn plan_for(&self, request: &PublishGenerationRequest) -> PublishPlan {
        let previous_generation = self.current_generation(&request.source_id);
        let mut warnings = Vec::new();
        let ready = match (&request.expected_previous_generation, &previous_generation) {
            (Some(expected), Some(actual)) if expected != actual => {
                warnings.push(mismatch_warning(&request.source_id, expected, actual));
                false
            }
            (Some(expected), None) => {
                warnings.push(mismatch_warning(
                    &request.source_id,
                    expected,
                    &SourceGenerationId::default(),
                ));
                false
            }
            _ => true,
        };
        PublishPlan {
            source_id: request.source_id.clone(),
            generation: request.generation.clone(),
            previous_generation,
            ready,
            estimated_document_count: 0,
            estimated_chunk_count: 0,
            cleanup_debt_preview: Vec::new(),
            warnings,
        }
    }
}

fn mismatch_warning(
    source_id: &SourceId,
    expected: &SourceGenerationId,
    actual: &SourceGenerationId,
) -> SourceWarning {
    SourceWarning {
        code: "retrieval.publish.generation_mismatch".to_string(),
        severity: axon_api::source::Severity::Warning,
        message: format!(
            "source {} expected previous generation {:?} but current committed generation is {:?}",
            source_id.0, expected.0, actual.0
        ),
        source_item_key: None,
        retryable: false,
    }
}

#[async_trait]
impl GenerationPublisher for InMemoryGenerationPublisher {
    async fn validate_publish(&self, request: PublishGenerationRequest) -> Result<PublishPlan> {
        Ok(self.plan_for(&request))
    }

    async fn publish_generation(
        &self,
        request: PublishGenerationRequest,
    ) -> Result<PublishGenerationResult> {
        let plan = self.plan_for(&request);
        if !plan.ready {
            return Err(ApiError::new(
                "retrieval.publish.not_ready",
                ErrorStage::Publishing,
                "publish plan is not ready: expected_previous_generation does not match the \
                 currently committed generation",
            ));
        }
        self.committed
            .lock()
            .expect("in-memory generation publisher mutex poisoned")
            .insert(request.source_id.clone(), request.generation.clone());
        Ok(PublishGenerationResult {
            header: axon_api::source::StageResultHeader {
                job_id: axon_api::source::JobId::default(),
                stage_id: axon_api::source::StageId::default(),
                phase: axon_api::source::PipelinePhase::Publishing,
                status: axon_api::source::LifecycleStatus::Completed,
                started_at: Timestamp::from(Utc::now()),
                completed_at: Some(Timestamp::from(Utc::now())),
                counts: axon_api::source::StageCounts {
                    items_total: None,
                    items_done: 0,
                    documents_total: None,
                    documents_done: 0,
                    chunks_total: None,
                    chunks_done: 0,
                    bytes_total: None,
                    bytes_done: 0,
                },
                warnings: Vec::new(),
                error: None,
            },
            source_id: request.source_id,
            generation: request.generation,
            published_at: Timestamp::from(Utc::now()),
            document_count: 0,
            chunk_count: 0,
            vector_point_count: 0,
            cleanup_debt: Vec::new(),
        })
    }
}

#[cfg(test)]
#[path = "publish_tests.rs"]
mod tests;
