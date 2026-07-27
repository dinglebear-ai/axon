use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;

use async_trait::async_trait;
use axon_adapters::boundary::FakeAdapterProviders;
use axon_adapters::web::WebSourceAdapter;
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
use axon_ledger::sqlite::SqliteLedgerStore;
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
    ledger: Arc<dyn LedgerStore>,
    /// Concrete handles to the fakes wired into `ctx`'s
    /// `TargetLocalSourceRuntime`, kept alongside the trait-object versions
    /// so differential/family-parity tests can inspect `.calls()` after
    /// driving a dispatch through `ctx()` — `Arc<dyn EmbeddingProvider>` /
    /// `Arc<dyn VectorStore>` have no downcast path back to these.
    embedder: Arc<FakeEmbeddingProvider>,
    vectors: Arc<FakeVectorStore>,
}

impl SourceWebJobIdentityHarness {
    pub(crate) fn ctx(&self) -> &ServiceContext {
        &self.ctx
    }

    pub(crate) fn embedder(&self) -> &Arc<FakeEmbeddingProvider> {
        &self.embedder
    }

    pub(crate) fn vectors(&self) -> &Arc<FakeVectorStore> {
        &self.vectors
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
                // `{error:#}` (anyhow's alternate Display) prints the full
                // `.context()` chain instead of only the outermost frame —
                // needed so test failures surface the real cause instead of
                // a generic "... indexing failed" wrapper message.
                format!("{error:#}"),
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

/// Which `LedgerStore` backs a [`SourceWebJobIdentityHarness`]. Both variants
/// share the same real SQLite-backed `jobs` store; only the ledger differs.
enum LedgerBackend {
    /// In-memory, non-persisting fake. Fine for web-source dispatch, whose
    /// `SourceEventEmitter::emit` swallows `jobs.update_status` failures
    /// (logs a warning, does not propagate) — so a `jobs.source_id` FK
    /// mismatch against a ledger that never actually writes `sources` rows
    /// never surfaces.
    Fake,
    /// A real `SqliteLedgerStore` bound to the *same* pool as `jobs`,
    /// matching the production contract ("the runtime uses ONE database so
    /// `jobs.source_id` can FK to `sources(source_id)`" —
    /// `SqliteLedgerStore::from_pool`'s doc comment). Local-source dispatch's
    /// `JobProgressSink::record_phase` (unlike web's event emitter)
    /// propagates `jobs.update_status` errors via `?`, so it needs a ledger
    /// that really persists `upsert_source` into the same database the
    /// `jobs` FK checks against — the in-memory `Fake` backend fails every
    /// local-source run with `FOREIGN KEY constraint failed` the instant the
    /// first phase update stamps `jobs.source_id`.
    SharedSqlite,
}

async fn build_source_job_identity_harness(
    ledger_backend: LedgerBackend,
) -> anyhow::Result<SourceWebJobIdentityHarness> {
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
    let store: Arc<dyn JobStore> = Arc::new(SqliteUnifiedJobStore::new(pool.clone()));

    let ledger: Arc<dyn LedgerStore> = match ledger_backend {
        LedgerBackend::Fake => Arc::new(FakeLedgerStore::new()),
        LedgerBackend::SharedSqlite => Arc::new(SqliteLedgerStore::from_pool(pool)),
    };
    let vectors = Arc::new(FakeVectorStore::new("fake-vector"));
    let embedder = Arc::new(FakeEmbeddingProvider::new("fake-embedding", 8));
    let mut target = TargetLocalSourceRuntime::new(
        Arc::clone(&store),
        ledger.clone(),
        embedder.clone(),
        vectors.clone(),
        axon_api::source::ProviderId::new("fake-embedding"),
        "fake-embedding",
        8,
    );
    let providers = Arc::new(FakeAdapterProviders::new());
    let web_fetch_provider = Arc::clone(&providers);
    let web_render_provider = Arc::clone(&providers);
    target.web_source_adapter = Arc::new(WebSourceAdapter::new(
        web_fetch_provider,
        web_render_provider,
    ));
    target.fetch_provider = providers.clone();
    target.render_provider = providers;

    let ctx = ServiceContext::from_runtime(cfg, runtime).with_target_local_source_runtime(target);
    Ok(SourceWebJobIdentityHarness {
        _tmp: tmp,
        ctx,
        store,
        ledger,
        embedder,
        vectors,
    })
}

pub(crate) async fn source_context_with_fake_web() -> anyhow::Result<SourceWebJobIdentityHarness> {
    build_source_job_identity_harness(LedgerBackend::Fake).await
}

/// Same runtime wiring as [`source_context_with_fake_web`], but for exercising
/// **local**-source dispatch: a real `SqliteLedgerStore` shares the `jobs`
/// pool instead of the non-persisting `FakeLedgerStore` — see
/// [`LedgerBackend::SharedSqlite`] for why local dispatch specifically needs
/// this. Everything else (fake embedding/vector providers, fake web
/// fetch/render providers on `target`, `ServiceContext` wiring) is identical;
/// local dispatch never touches the fake web providers, so the same harness
/// shape covers both families.
pub(crate) async fn source_context_with_local_sqlite_ledger()
-> anyhow::Result<SourceWebJobIdentityHarness> {
    build_source_job_identity_harness(LedgerBackend::SharedSqlite).await
}
