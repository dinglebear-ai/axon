//! Deterministic retrieval testing helpers.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axon_api::mcp_schema::AskRequest;
use axon_api::source::{
    ApiError, CapabilityBase, HealthStatus, MetadataMap, PublishGenerationRequest,
    PublishGenerationResult, PublishPlan, RetrievalCapability,
};
use axon_error::ErrorStage;

use crate::ask_context::AskContext;
use crate::boundary::{self, Result};
use crate::publish::GenerationPublisher;
use crate::query::{QueryRequest, QueryResult, RetrievalRequest, RetrievalResult};

pub const MODULE_NAME: &str = "testing";

/// Deterministic mode for [`FakeRetrievalEngine`], mirroring the
/// success/timeout/rate-limited/fatal shape used by other boundary fakes
/// (e.g. `axon_adapters::boundary::FakeAdapterProviders`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FakeRetrievalMode {
    Success,
    Degraded,
    Fatal,
}

/// Fake [`boundary::RetrievalEngine`] returning a fixed, caller-seeded
/// [`RetrievalResult`] for every request, with deterministic
/// success/degraded/fatal modes, a recorded-call log, and a
/// [`RetrievalCapability`] override for capability-shape assertions.
#[derive(Debug, Clone)]
pub struct FakeRetrievalEngine {
    result: RetrievalResult,
    mode: FakeRetrievalMode,
    capability_override: Option<RetrievalCapability>,
    calls: Arc<Mutex<Vec<RetrievalRequest>>>,
}

impl FakeRetrievalEngine {
    pub fn new(result: RetrievalResult) -> Self {
        Self {
            result,
            mode: FakeRetrievalMode::Success,
            capability_override: None,
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn with_mode(mut self, mode: FakeRetrievalMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn with_capability_override(mut self, capability: RetrievalCapability) -> Self {
        self.capability_override = Some(capability);
        self
    }

    /// Requests recorded by every trait-method call that reaches
    /// [`Self::do_retrieve`] (`retrieve`, and indirectly `query` /
    /// `build_ask_context`, which both compose through it).
    pub fn calls(&self) -> Vec<RetrievalRequest> {
        self.calls
            .lock()
            .expect("fake retrieval call log mutex poisoned")
            .clone()
    }

    fn record(&self, request: &RetrievalRequest) {
        self.calls
            .lock()
            .expect("fake retrieval call log mutex poisoned")
            .push(request.clone());
    }

    fn mode_error(&self) -> Option<ApiError> {
        match self.mode {
            FakeRetrievalMode::Success | FakeRetrievalMode::Degraded => None,
            FakeRetrievalMode::Fatal => {
                let mut error = ApiError::new(
                    "retrieval.fake_fatal",
                    ErrorStage::Retrieving,
                    "fake retrieval engine failed fatally",
                );
                error.retryable = false;
                Some(error)
            }
        }
    }

    fn health(&self) -> HealthStatus {
        match self.mode {
            FakeRetrievalMode::Success => HealthStatus::Healthy,
            FakeRetrievalMode::Degraded => HealthStatus::Degraded,
            FakeRetrievalMode::Fatal => HealthStatus::Unavailable,
        }
    }

    async fn do_retrieve(&self, request: RetrievalRequest) -> Result<RetrievalResult> {
        self.record(&request);
        if let Some(err) = self.mode_error() {
            return Err(err);
        }
        Ok(self.result.clone())
    }
}

#[async_trait]
impl boundary::RetrievalEngine for FakeRetrievalEngine {
    async fn query(&self, request: QueryRequest) -> Result<QueryResult> {
        let retrieval_request = RetrievalRequest {
            query: request.query,
            collection: request.collection,
            limit: request.limit,
            source_id: None,
            generation: None,
            namespace_filters: request.namespace_filters,
            excluded_namespaces: Vec::new(),
            byte_budget: u64::MAX,
            token_budget: u32::MAX,
        };
        let result = self.do_retrieve(retrieval_request).await?;
        Ok(QueryResult {
            matches: result.matches,
            citations: result.citations,
        })
    }

    async fn retrieve(&self, request: RetrievalRequest) -> Result<RetrievalResult> {
        self.do_retrieve(request).await
    }

    async fn build_ask_context(&self, request: AskRequest) -> Result<AskContext> {
        let retrieval_request = RetrievalRequest {
            query: request.query.unwrap_or_default(),
            collection: request.collection.unwrap_or_else(|| "axon".to_string()),
            limit: request
                .ask_chunk_limit
                .and_then(|n| u32::try_from(n).ok())
                .unwrap_or(12),
            source_id: None,
            generation: None,
            namespace_filters: Vec::new(),
            excluded_namespaces: Vec::new(),
            byte_budget: u64::MAX,
            token_budget: u32::MAX,
        };
        let retrieval = self.do_retrieve(retrieval_request).await?;
        Ok(AskContext {
            context: retrieval.context.clone(),
            citations: retrieval.citations.clone(),
            retrieval,
        })
    }

    async fn capabilities(&self) -> Result<RetrievalCapability> {
        if let Some(capability) = &self.capability_override {
            return Ok(capability.clone());
        }
        Ok(RetrievalCapability(CapabilityBase {
            name: "fake_retrieval_engine".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            owner_crate: "axon-retrieval".to_string(),
            health: self.health(),
            features: vec!["fake".to_string()],
            limits: MetadataMap::new(),
        }))
    }
}

/// Deterministic mode for [`FakeGenerationPublisher`], mirroring the
/// success/degraded/fatal shape used by [`FakeRetrievalEngine`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FakeGenerationPublisherMode {
    Success,
    Degraded,
    Fatal,
}

/// Fake [`boundary::GenerationPublisher`]-shaped publisher (`GenerationPublisher`
/// lives in [`crate::publish`], not `crate::boundary`, since — unlike
/// `RetrievalEngine` — it has no concrete-struct name collision to route
/// around) with deterministic success/degraded/fatal modes, a recorded-call
/// log, and caller-seeded [`PublishPlan`]/[`PublishGenerationResult`]
/// responses.
#[derive(Debug, Clone)]
pub struct FakeGenerationPublisher {
    plan: PublishPlan,
    result: PublishGenerationResult,
    mode: FakeGenerationPublisherMode,
    calls: Arc<Mutex<Vec<PublishGenerationRequest>>>,
}

impl FakeGenerationPublisher {
    pub fn new(plan: PublishPlan, result: PublishGenerationResult) -> Self {
        Self {
            plan,
            result,
            mode: FakeGenerationPublisherMode::Success,
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn with_mode(mut self, mode: FakeGenerationPublisherMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn calls(&self) -> Vec<PublishGenerationRequest> {
        self.calls
            .lock()
            .expect("fake generation publisher call log mutex poisoned")
            .clone()
    }

    fn record(&self, request: &PublishGenerationRequest) {
        self.calls
            .lock()
            .expect("fake generation publisher call log mutex poisoned")
            .push(request.clone());
    }

    fn mode_error(&self) -> Option<ApiError> {
        match self.mode {
            FakeGenerationPublisherMode::Success | FakeGenerationPublisherMode::Degraded => None,
            FakeGenerationPublisherMode::Fatal => {
                let mut error = ApiError::new(
                    "retrieval.publish.fake_fatal",
                    ErrorStage::Publishing,
                    "fake generation publisher failed fatally",
                );
                error.retryable = false;
                Some(error)
            }
        }
    }
}

#[async_trait]
impl GenerationPublisher for FakeGenerationPublisher {
    async fn validate_publish(&self, request: PublishGenerationRequest) -> Result<PublishPlan> {
        self.record(&request);
        if let Some(err) = self.mode_error() {
            return Err(err);
        }
        Ok(self.plan.clone())
    }

    async fn publish_generation(
        &self,
        request: PublishGenerationRequest,
    ) -> Result<PublishGenerationResult> {
        self.record(&request);
        if let Some(err) = self.mode_error() {
            return Err(err);
        }
        Ok(self.result.clone())
    }
}

#[cfg(test)]
#[path = "testing_tests.rs"]
mod tests;
