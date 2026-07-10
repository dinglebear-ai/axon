//! [`FakeSourceEnricher`] — split out of `testing.rs` to keep that file under
//! the monolith line cap.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axon_api::source::*;

use crate::adapter::Result;
use crate::enrichment::SourceEnricher;

/// Deterministic response mode for [`FakeSourceEnricher`].
///
/// Satisfies the trait-contract "Fake Requirements": deterministic success,
/// deterministic failure, and a degraded/warning mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FakeSourceEnricherMode {
    #[default]
    Success,
    Failure,
    Degraded,
}

/// Fake [`SourceEnricher`] with deterministic success/failure/degraded modes,
/// a caller-configurable [`EnrichmentKind`]/[`EnrichmentStatus`] pair, a
/// recorded-call log, and a capability override.
#[derive(Debug, Clone)]
pub struct FakeSourceEnricher {
    mode: FakeSourceEnricherMode,
    enrichment_kind: EnrichmentKind,
    enrichment_status: EnrichmentStatus,
    capability_override: Option<SourceEnricherCapability>,
    calls: Arc<Mutex<Vec<SourceItemKey>>>,
}

impl FakeSourceEnricher {
    pub fn new() -> Self {
        Self {
            mode: FakeSourceEnricherMode::Success,
            enrichment_kind: EnrichmentKind::Metadata,
            enrichment_status: EnrichmentStatus::Completed,
            capability_override: None,
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn with_mode(mut self, mode: FakeSourceEnricherMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn with_result(mut self, kind: EnrichmentKind, status: EnrichmentStatus) -> Self {
        self.enrichment_kind = kind;
        self.enrichment_status = status;
        self
    }

    pub fn with_capability_override(mut self, capability: SourceEnricherCapability) -> Self {
        self.capability_override = Some(capability);
        self
    }

    /// Item keys recorded by every `enrich` call, for call-count/order
    /// assertions.
    pub fn calls(&self) -> Vec<SourceItemKey> {
        self.calls
            .lock()
            .expect("fake source enricher call log mutex poisoned")
            .clone()
    }

    fn record(&self, key: &SourceItemKey) {
        self.calls
            .lock()
            .expect("fake source enricher call log mutex poisoned")
            .push(key.clone());
    }

    fn health(&self) -> HealthStatus {
        match self.mode {
            FakeSourceEnricherMode::Success => HealthStatus::Healthy,
            FakeSourceEnricherMode::Degraded => HealthStatus::Degraded,
            FakeSourceEnricherMode::Failure => HealthStatus::Unavailable,
        }
    }
}

impl Default for FakeSourceEnricher {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SourceEnricher for FakeSourceEnricher {
    async fn enrich(
        &self,
        plan: &SourcePlan,
        item: &AcquiredSourceItem,
    ) -> Result<SourceEnrichment> {
        self.record(&item.manifest_item.source_item_key);
        if self.mode == FakeSourceEnricherMode::Failure {
            return Err(ApiError::new(
                "adapter.enrich.fake_failure",
                axon_error::ErrorStage::Enriching,
                "fake source enricher configured to fail",
            ));
        }
        let warnings = if self.mode == FakeSourceEnricherMode::Degraded {
            vec![SourceWarning {
                code: "adapter.enrich.fake_degraded".to_string(),
                severity: Severity::Degraded,
                message: "fake source enricher running in degraded mode".to_string(),
                source_item_key: Some(item.manifest_item.source_item_key.clone()),
                retryable: true,
            }]
        } else {
            Vec::new()
        };
        Ok(SourceEnrichment {
            header: StageResultHeader {
                job_id: plan.job_id,
                stage_id: super::stage_id(3),
                phase: PipelinePhase::Enriching,
                status: LifecycleStatus::Completed,
                started_at: super::timestamp(),
                completed_at: Some(super::timestamp()),
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
                warnings: warnings.clone(),
                error: None,
            },
            source_id: plan.route.source.source_id.clone(),
            source_item_key: item.manifest_item.source_item_key.clone(),
            enrichment_kind: self.enrichment_kind,
            status: self.enrichment_status,
            metadata: MetadataMap::new(),
            parse_hints: Vec::new(),
            chunk_hints: Vec::new(),
            graph_candidates: Vec::new(),
            artifacts: Vec::new(),
            warnings,
        })
    }

    async fn capabilities(&self) -> Result<SourceEnricherCapability> {
        if let Some(capability) = &self.capability_override {
            return Ok(capability.clone());
        }
        Ok(SourceEnricherCapability(CapabilityBase {
            name: "fake-source-enricher".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            owner_crate: "axon-adapters".to_string(),
            health: self.health(),
            features: vec!["fake".to_string()],
            limits: MetadataMap::new(),
        }))
    }
}
