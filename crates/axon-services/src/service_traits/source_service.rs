//! `SourceService` — the unified source lifecycle entrypoint (submit/run_now/
//! resolve/get/list/items/generations).
//!
//! Contract: `docs/pipeline-unification/foundation/types/service-contract.md`
//! §SourceService. Only `submit`/`run_now` wrap a real free function
//! (`crate::source::index_source`, which is already fully synchronous, so
//! both trait methods currently wrap the same call). `resolve`/`get`/`list`/
//! `items`/`generations` have no backing free function today — see the
//! deferred list in the approved wiring plan — so their production impl
//! returns [`crate::service_traits::not_implemented`] and only the `Fake`
//! implements real (in-memory) semantics.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axon_api::source::{
    Page, ResolvedSource, SourceGenerationListRequest, SourceGenerationSummary, SourceId,
    SourceItem, SourceItemListRequest, SourceListRequest, SourceRequest, SourceResult,
    SourceSummary,
};

use crate::context::ServiceContext;
use crate::service_traits::not_implemented;
use crate::source::index_source;

#[async_trait]
pub trait SourceService: Send + Sync {
    async fn submit(&self, request: SourceRequest) -> anyhow::Result<SourceResult>;
    async fn run_now(&self, request: SourceRequest) -> anyhow::Result<SourceResult>;
    async fn resolve(&self, request: SourceRequest) -> anyhow::Result<ResolvedSource>;
    async fn get(&self, source_id: SourceId) -> anyhow::Result<SourceSummary>;
    async fn list(&self, request: SourceListRequest) -> anyhow::Result<Page<SourceSummary>>;
    async fn items(&self, request: SourceItemListRequest) -> anyhow::Result<Page<SourceItem>>;
    async fn generations(
        &self,
        request: SourceGenerationListRequest,
    ) -> anyhow::Result<Page<SourceGenerationSummary>>;
}

pub struct SourceServiceImpl {
    ctx: Arc<ServiceContext>,
}

impl SourceServiceImpl {
    pub fn new(ctx: Arc<ServiceContext>) -> Self {
        Self { ctx }
    }
}

#[async_trait]
impl SourceService for SourceServiceImpl {
    async fn submit(&self, request: SourceRequest) -> anyhow::Result<SourceResult> {
        index_source(request, &self.ctx).await
    }

    async fn run_now(&self, request: SourceRequest) -> anyhow::Result<SourceResult> {
        // `index_source` is already synchronous end-to-end today, so `run_now`
        // wraps the same call as `submit` until a real async-submit path
        // exists (see the wiring plan's SourceService notes).
        index_source(request, &self.ctx).await
    }

    async fn resolve(&self, _request: SourceRequest) -> anyhow::Result<ResolvedSource> {
        Err(not_implemented("SourceService::resolve"))
    }

    async fn get(&self, _source_id: SourceId) -> anyhow::Result<SourceSummary> {
        Err(not_implemented("SourceService::get"))
    }

    async fn list(&self, _request: SourceListRequest) -> anyhow::Result<Page<SourceSummary>> {
        Err(not_implemented("SourceService::list"))
    }

    async fn items(&self, _request: SourceItemListRequest) -> anyhow::Result<Page<SourceItem>> {
        Err(not_implemented("SourceService::items"))
    }

    async fn generations(
        &self,
        _request: SourceGenerationListRequest,
    ) -> anyhow::Result<Page<SourceGenerationSummary>> {
        Err(not_implemented("SourceService::generations"))
    }
}

/// Deterministic in-memory fake covering every `SourceService` method.
#[derive(Default)]
pub struct FakeSourceService {
    sources: Mutex<std::collections::HashMap<String, SourceSummary>>,
}

impl FakeSourceService {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn seed(&self, summary: SourceSummary) {
        self.sources
            .lock()
            .unwrap()
            .insert(summary.source_id.0.clone(), summary);
    }
}

#[async_trait]
impl SourceService for FakeSourceService {
    async fn submit(&self, request: SourceRequest) -> anyhow::Result<SourceResult> {
        Ok(fake_source_result(&request))
    }

    async fn run_now(&self, request: SourceRequest) -> anyhow::Result<SourceResult> {
        Ok(fake_source_result(&request))
    }

    async fn resolve(&self, request: SourceRequest) -> anyhow::Result<ResolvedSource> {
        let source_id = SourceId::new(format!("fake:{}", request.source));
        let scope = request.scope.unwrap_or(axon_api::source::SourceScope::Page);
        Ok(ResolvedSource::resolved(
            request.source.clone(),
            request.source,
            source_id,
            axon_api::source::SourceKind::Web,
            axon_api::source::AdapterRef {
                name: "fake".to_string(),
                version: "0".to_string(),
            },
            scope,
            axon_api::source::AuthorityLevel::Unknown,
            1.0,
            "fake resolution: single acquisition family match",
        ))
    }

    async fn get(&self, source_id: SourceId) -> anyhow::Result<SourceSummary> {
        self.sources
            .lock()
            .unwrap()
            .get(&source_id.0)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("source {} not found", source_id.0))
    }

    async fn list(&self, request: SourceListRequest) -> anyhow::Result<Page<SourceSummary>> {
        let sources = self.sources.lock().unwrap();
        let limit = request.limit.unwrap_or(50);
        Ok(Page {
            items: sources.values().take(limit as usize).cloned().collect(),
            next_cursor: None,
            limit,
            total: Some(sources.len() as u64),
        })
    }

    async fn items(&self, request: SourceItemListRequest) -> anyhow::Result<Page<SourceItem>> {
        Ok(Page {
            items: Vec::new(),
            next_cursor: None,
            limit: request.limit.unwrap_or(50),
            total: Some(0),
        })
    }

    async fn generations(
        &self,
        request: SourceGenerationListRequest,
    ) -> anyhow::Result<Page<SourceGenerationSummary>> {
        Ok(Page {
            items: Vec::new(),
            next_cursor: None,
            limit: request.limit.unwrap_or(50),
            total: Some(0),
        })
    }
}

fn fake_source_result(request: &SourceRequest) -> SourceResult {
    use axon_api::source::{
        AdapterRef, GraphWriteSummary, JobId, LedgerSummary, LifecycleStatus, SourceCounts,
        SourceGenerationId, SourceKind, SourceScope,
    };

    let source_id = SourceId::new(format!("fake:{}", request.source));
    let generation = SourceGenerationId::new("fake-generation-1");
    SourceResult {
        job_id: JobId::new(uuid::Uuid::new_v4()),
        source_id: source_id.clone(),
        canonical_uri: request.source.clone(),
        source_kind: SourceKind::Web,
        adapter: AdapterRef {
            name: "fake".to_string(),
            version: "0".to_string(),
        },
        scope: request.scope.unwrap_or(SourceScope::Page),
        status: LifecycleStatus::Completed,
        ledger: LedgerSummary {
            source_id,
            generation,
            committed_generation: None,
            status: LifecycleStatus::Completed,
            counts: SourceCounts {
                items_total: 1,
                items_changed: 1,
                documents_total: 1,
                chunks_total: 0,
                vector_points_total: 0,
                bytes_total: 0,
            },
        },
        graph: GraphWriteSummary {
            nodes_upserted: 0,
            edges_upserted: 0,
            evidence_records: 0,
            degraded: false,
        },
        counts: SourceCounts {
            items_total: 1,
            items_changed: 1,
            documents_total: 1,
            chunks_total: 0,
            vector_points_total: 0,
            bytes_total: 0,
        },
        warnings: Vec::new(),
        inline: None,
        job: None,
        watch: None,
        artifacts: Vec::new(),
        errors: Vec::new(),
    }
}

#[cfg(test)]
#[path = "source_service_tests.rs"]
mod tests;
