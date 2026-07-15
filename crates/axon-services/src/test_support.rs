use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;

use async_trait::async_trait;
use axon_adapters::boundary::FakeAdapterProviders;
use axon_api::source::{
    AuthSnapshot, JobKind, JobListRequest, JobSummary, SourceGenerationId, SourceListRequest,
    SourceRequest, SourceSummary,
};
use axon_core::config::Config;
use axon_embedding::fake::FakeEmbeddingProvider;
use axon_jobs::SqliteJobBackend;
use axon_jobs::boundary::JobStore;
use axon_jobs::status::JobStatus;
use axon_jobs::unified::SqliteUnifiedJobStore;
use axon_jobs::workers::unified::UnifiedClaimedJob;
use axon_ledger::store::{FakeLedgerStore, LedgerStore};
use axon_vectors::payload::generation_payload_i64;
use axon_vectors::store::FakeVectorStore;
use serde_json::{Value, json};

use crate::context::{ServiceContext, TargetLocalSourceRuntime};
use crate::runtime::SqliteServiceRuntime;
use crate::runtime::{RuntimeResult, ServiceJobRuntime};

#[derive(Default)]
pub(crate) struct NoopServiceRuntime;

pub(crate) fn committed_generation_payload(generation: &SourceGenerationId) -> Value {
    json!(
        generation_payload_i64(generation, "committed_generation")
            .expect("test generation id is payload-encodable")
    )
}

pub(crate) fn source_generation_payload(generation: &SourceGenerationId) -> Value {
    json!(
        generation_payload_i64(generation, "source_generation")
            .expect("test generation id is payload-encodable")
    )
}

pub(crate) fn is_uncommitted_generation(value: &Value) -> bool {
    value.is_null()
}

#[async_trait]
impl ServiceJobRuntime for NoopServiceRuntime {
    fn mode_name(&self) -> &'static str {
        "test"
    }

    async fn wait_for_job(&self, _id: uuid::Uuid, _kind: JobKind) -> RuntimeResult<String> {
        Ok("completed".to_string())
    }

    async fn job_errors(&self, _id: uuid::Uuid, _kind: JobKind) -> RuntimeResult<Option<String>> {
        Ok(None)
    }

    async fn has_active_jobs(&self, _kind: JobKind) -> RuntimeResult<bool> {
        Ok(false)
    }

    async fn list_jobs(
        &self,
        _kind: JobKind,
        _limit: i64,
        _offset: i64,
    ) -> RuntimeResult<Vec<crate::types::ServiceJob>> {
        Ok(Vec::new())
    }

    async fn job_status(
        &self,
        _kind: JobKind,
        _id: uuid::Uuid,
    ) -> RuntimeResult<Option<crate::types::ServiceJob>> {
        Ok(None)
    }

    async fn cancel_job(&self, _kind: JobKind, _id: uuid::Uuid) -> RuntimeResult<bool> {
        Ok(false)
    }

    async fn cleanup_jobs(&self, _kind: JobKind) -> RuntimeResult<u64> {
        Ok(0)
    }

    async fn clear_jobs(&self, _kind: JobKind) -> RuntimeResult<u64> {
        Ok(0)
    }

    async fn recover_jobs(&self, _kind: JobKind, _stale_threshold_ms: i64) -> RuntimeResult<u64> {
        Ok(0)
    }

    async fn count_jobs(&self, _kind: JobKind) -> RuntimeResult<i64> {
        Ok(0)
    }

    async fn count_jobs_by_status(&self, _kind: JobKind) -> RuntimeResult<HashMap<JobStatus, i64>> {
        Ok(HashMap::new())
    }
}

pub(crate) struct SourceWebJobIdentityHarness {
    _tmp: tempfile::TempDir,
    ctx: ServiceContext,
    store: Arc<dyn JobStore>,
    ledger: Arc<FakeLedgerStore>,
}

impl SourceWebJobIdentityHarness {
    pub(crate) fn ctx(&self) -> &ServiceContext {
        &self.ctx
    }

    pub(crate) async fn enqueue_and_claim_source(
        &self,
        request: SourceRequest,
    ) -> anyhow::Result<UnifiedClaimedJob> {
        let auth_snapshot = AuthSnapshot::trusted_system("test");
        let queued = crate::source::enqueue::enqueue_source(
            request,
            self.store.as_ref(),
            Some(auth_snapshot.clone()),
        )
        .await?;
        let descriptor = queued.job.expect("queued source job descriptor");
        let request_json = self
            .store
            .request_json(descriptor.job_id)
            .await?
            .expect("stored source request json");
        Ok(UnifiedClaimedJob {
            job_id: descriptor.job_id,
            kind: JobKind::Source,
            attempt: 1,
            request_json: Some(request_json),
            auth_snapshot,
        })
    }

    pub(crate) async fn run_source_claim_once(
        &self,
        claimed: &UnifiedClaimedJob,
    ) -> Result<(), axon_api::source::ApiError> {
        let source_request = claimed
            .request_json
            .as_ref()
            .and_then(|json| json.get("source_request"))
            .cloned()
            .ok_or_else(|| {
                axon_api::source::ApiError::new(
                    "job_runner.source_failed",
                    axon_api::source::ErrorStage::Fetching,
                    "source job request is missing `source_request`",
                )
            })
            .and_then(|value| {
                serde_json::from_value(value).map_err(|error| {
                    axon_api::source::ApiError::new(
                        "job_runner.source_failed",
                        axon_api::source::ErrorStage::Fetching,
                        format!("malformed source_request: {error}"),
                    )
                })
            })?;

        crate::runtime::job_runners::run_source_request_with_context(
            claimed,
            source_request,
            &self.ctx,
        )
        .await
        .map(|_| ())
        .map_err(|error| {
            axon_api::source::ApiError::new(
                "job_runner.source_failed",
                axon_api::source::ErrorStage::Fetching,
                error.to_string(),
            )
        })
    }

    pub(crate) async fn jobs_by_kind(&self, kind: JobKind) -> anyhow::Result<Vec<JobSummary>> {
        let page = self
            .store
            .list(JobListRequest {
                status: None,
                kind: Some(kind),
                source_id: None,
                watch_id: None,
                limit: Some(100),
                cursor: None,
            })
            .await?;
        Ok(page.items)
    }

    pub(crate) async fn source_summary_for(&self, source: &str) -> anyhow::Result<SourceSummary> {
        let page = self
            .ledger
            .list_sources(SourceListRequest {
                source_kind: None,
                adapter: None,
                status: None,
                authority: None,
                watch_enabled: None,
                tag: None,
                query: Some(source.to_string()),
                limit: Some(100),
                cursor: None,
            })
            .await?;
        page.items
            .into_iter()
            .find(|summary| summary.canonical_uri == source)
            .ok_or_else(|| anyhow::anyhow!("missing source summary for {source}"))
    }
}

pub(crate) async fn source_context_with_fake_web() -> anyhow::Result<SourceWebJobIdentityHarness> {
    let tmp = tempfile::tempdir()?;
    let mut cfg = Config::test_default();
    cfg.sqlite_path = tmp.path().join("jobs.db");
    cfg.qdrant_url = String::new();
    cfg.tei_url = String::new();
    let cfg = Arc::new(cfg);

    let backend = SqliteJobBackend::new(Arc::clone(&cfg))
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    let pool = backend.pool().as_ref().clone();
    let runtime: Arc<dyn ServiceJobRuntime> = Arc::new(SqliteServiceRuntime::new_for_backend(
        Arc::clone(&cfg),
        backend,
    ));
    let store: Arc<dyn JobStore> = Arc::new(SqliteUnifiedJobStore::new(pool));

    let ledger = Arc::new(FakeLedgerStore::new());
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let embedder = Arc::new(FakeEmbeddingProvider::new("fake-embedding", 8));
    let mut target = TargetLocalSourceRuntime::new(
        Arc::clone(&store),
        ledger.clone(),
        embedder,
        vectors,
        axon_api::source::ProviderId::new("fake-embedding"),
        "fake-embedding",
        8,
    );
    let providers = Arc::new(FakeAdapterProviders::new());
    target.fetch_provider = providers.clone();
    target.render_provider = providers;

    let ctx = ServiceContext::from_runtime(cfg, runtime).with_target_local_source_runtime(target);
    Ok(SourceWebJobIdentityHarness {
        _tmp: tmp,
        ctx,
        store,
        ledger,
    })
}
