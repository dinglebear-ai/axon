//! Source enrichment boundary (`SourceEnricher`).
//!
//! Contract: `docs/pipeline-unification/foundation/types/trait-contract.md`
//! §SourceEnricher. Unlike `SourceAdapter` (which already has eight
//! per-source-kind concrete implementations), nothing in this crate performs
//! "enrichment" today — the `enriching` pipeline stage
//! (`docs/pipeline-unification/foundation/source-pipeline.md`) sits between
//! `fetching`/`acquire` and `normalizing`/`normalize` and has no existing
//! owner. This file is therefore purely additive: it introduces both the
//! trait and its first concrete production implementation
//! (`NoopSourceEnricher`) together, so the enrichment stage always has a
//! non-panicking default while per-source-kind enrichers land in later
//! rounds. Nothing here changes any existing adapter's behavior or
//! signature.

use async_trait::async_trait;
use axon_api::source::*;
use chrono::Utc;
use uuid::Uuid;

pub type Result<T> = std::result::Result<T, ApiError>;

#[async_trait]
pub trait SourceEnricher: Send + Sync {
    async fn enrich(
        &self,
        plan: &SourcePlan,
        item: &AcquiredSourceItem,
    ) -> Result<SourceEnrichment>;
    async fn capabilities(&self) -> Result<SourceEnricherCapability>;
}

/// Minimal production `SourceEnricher`: performs no enrichment work and
/// reports every item as `EnrichmentStatus::NotNeeded` /
/// `EnrichmentKind::None`, with empty hints/candidates/artifacts. It is a
/// legitimate, always-succeeding implementation of the contract shape, not a
/// test fake — see [`crate::testing::FakeSourceEnricher`] for the
/// deterministic-mode test double.
#[derive(Debug, Clone, Default)]
pub struct NoopSourceEnricher;

impl NoopSourceEnricher {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SourceEnricher for NoopSourceEnricher {
    async fn enrich(
        &self,
        plan: &SourcePlan,
        item: &AcquiredSourceItem,
    ) -> Result<SourceEnrichment> {
        let now = Timestamp::from(Utc::now());
        Ok(SourceEnrichment {
            header: StageResultHeader {
                job_id: plan.job_id,
                stage_id: StageId::from(Uuid::new_v4()),
                phase: PipelinePhase::Enriching,
                status: LifecycleStatus::Completed,
                started_at: now.clone(),
                completed_at: Some(now),
                counts: StageCounts {
                    items_total: Some(1),
                    items_done: 1,
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
            source_id: plan.route.source.source_id.clone(),
            source_item_key: item.manifest_item.source_item_key.clone(),
            enrichment_kind: EnrichmentKind::None,
            status: EnrichmentStatus::NotNeeded,
            metadata: MetadataMap::new(),
            parse_hints: Vec::new(),
            chunk_hints: Vec::new(),
            graph_candidates: Vec::new(),
            artifacts: Vec::new(),
            warnings: Vec::new(),
        })
    }

    async fn capabilities(&self) -> Result<SourceEnricherCapability> {
        Ok(SourceEnricherCapability(CapabilityBase {
            name: "axon-adapters::NoopSourceEnricher".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            owner_crate: "axon-adapters".to_string(),
            health: HealthStatus::Healthy,
            features: vec!["noop".to_string()],
            limits: MetadataMap::new(),
        }))
    }
}

#[cfg(test)]
#[path = "enrichment_tests.rs"]
mod tests;
