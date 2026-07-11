//! `ProviderService` — provider capability/health discovery (capabilities/
//! providers/provider/health/doctor).
//!
//! Contract: `docs/pipeline-unification/foundation/types/service-contract.md`
//! §ProviderService. Only `doctor` wraps a real free function
//! (`crate::system::doctor::doctor`) — and even that returns the
//! `axon-services`-local `crate::types::DoctorResult`, not the contract's
//! `axon-api::HealthReport`-shaped `DoctorReport` (which doesn't exist as a
//! DTO). `capabilities`/`providers`/`provider`/`health` have no backing free
//! function anywhere in `axon-services` today, so they are FAKE_ONLY.

use std::sync::Arc;

use async_trait::async_trait;
use axon_api::source::{
    CapabilityDocument, HealthReport, ProviderCapability, ProviderId, ProviderSummary,
};

use crate::context::ServiceContext;
use crate::service_traits::not_implemented;
use crate::types::DoctorResult;

#[async_trait]
pub trait ProviderService: Send + Sync {
    async fn capabilities(&self) -> anyhow::Result<CapabilityDocument>;
    async fn providers(&self) -> anyhow::Result<Vec<ProviderSummary>>;
    async fn provider(&self, provider_id: ProviderId) -> anyhow::Result<ProviderCapability>;
    async fn health(&self) -> anyhow::Result<HealthReport>;
    async fn doctor(&self) -> anyhow::Result<DoctorResult>;
}

pub struct ProviderServiceImpl {
    ctx: Arc<ServiceContext>,
}

impl ProviderServiceImpl {
    pub fn new(ctx: Arc<ServiceContext>) -> Self {
        Self { ctx }
    }
}

#[async_trait]
impl ProviderService for ProviderServiceImpl {
    async fn capabilities(&self) -> anyhow::Result<CapabilityDocument> {
        Err(not_implemented("ProviderService::capabilities"))
    }

    async fn providers(&self) -> anyhow::Result<Vec<ProviderSummary>> {
        Err(not_implemented("ProviderService::providers"))
    }

    async fn provider(&self, _provider_id: ProviderId) -> anyhow::Result<ProviderCapability> {
        Err(not_implemented("ProviderService::provider"))
    }

    async fn health(&self) -> anyhow::Result<HealthReport> {
        Err(not_implemented("ProviderService::health"))
    }

    async fn doctor(&self) -> anyhow::Result<DoctorResult> {
        crate::system::doctor(&self.ctx)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}

/// Deterministic in-memory fake covering every `ProviderService` method.
#[derive(Default)]
pub struct FakeProviderService;

impl FakeProviderService {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ProviderService for FakeProviderService {
    async fn capabilities(&self) -> anyhow::Result<CapabilityDocument> {
        Ok(CapabilityDocument {
            server: axon_api::source::ServerInfo {
                name: "axon".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                build: None,
                environment: Some("fake".to_string()),
            },
            generated_at: axon_api::source::Timestamp::from(chrono::Utc::now()),
            source_kinds: Vec::new(),
            source_scopes: Vec::new(),
            pipeline_phases: Vec::new(),
            adapters: Vec::new(),
            providers: Vec::new(),
            stores: axon_api::source::StoreCapabilities {
                ledger: None,
                graph: None,
                memory: None,
                job: None,
                watch: None,
                artifact: None,
                config: None,
                document_cache: None,
            },
            metadata: axon_api::source::MetadataMap::new(),
        })
    }

    async fn providers(&self) -> anyhow::Result<Vec<ProviderSummary>> {
        Ok(vec![ProviderSummary {
            provider_id: ProviderId::new("fake-provider"),
            provider_kind: axon_api::source::ProviderKind::Embedding,
            health: axon_api::source::HealthStatus::Healthy,
            active_reservations: 0,
            queued_requests: 0,
            cooling_until: None,
        }])
    }

    async fn provider(&self, provider_id: ProviderId) -> anyhow::Result<ProviderCapability> {
        if provider_id.0 != "fake-provider" {
            anyhow::bail!("provider {} not found", provider_id.0);
        }
        Ok(ProviderCapability {
            provider_id,
            provider_kind: axon_api::source::ProviderKind::Embedding,
            implementation: "fake".to_string(),
            version: "0".to_string(),
            health: axon_api::source::HealthStatus::Healthy,
            limits: axon_api::source::ProviderLimits::default(),
            features: Vec::new(),
            cooldown_until: None,
            last_error: None,
            reservation_policy: axon_api::source::ReservationPolicy {
                supports_reservations: false,
                queue_policy: axon_api::source::QueuePolicy::Fifo,
                interactive_reserve: 0,
                cooldown_after_failures: 0,
                cooldown_secs: 0,
                retry_backoff_ms: None,
            },
            reservation_state: axon_api::source::ReservationStateSnapshot {
                queued: 0,
                active: 0,
                available_units: 0,
                oldest_queued_ms: None,
                priority_breakdown: Default::default(),
                states: Vec::new(),
            },
            cost_class: axon_api::source::ProviderCostClass::Free,
            degraded_modes: Vec::new(),
            fake_overrides_supported: true,
            embedding: None,
            llm: None,
            vector_store: None,
            fetch: None,
            render: None,
            credential: None,
        })
    }

    async fn health(&self) -> anyhow::Result<HealthReport> {
        let providers = self.providers().await?;
        Ok(HealthReport {
            status: axon_api::source::HealthStatus::Healthy,
            generated_at: axon_api::source::Timestamp::from(chrono::Utc::now()),
            providers,
            warnings: Vec::new(),
            metadata: axon_api::source::MetadataMap::new(),
        })
    }

    async fn doctor(&self) -> anyhow::Result<DoctorResult> {
        Ok(DoctorResult {
            payload: serde_json::json!({"status": "ok", "fake": true}),
        })
    }
}

#[cfg(test)]
#[path = "provider_service_tests.rs"]
mod tests;
