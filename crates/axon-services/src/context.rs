use std::sync::Arc;
use std::time::Duration;

use crate::runtime::{ServiceJobRuntime, resolve_runtime_with_workers};
#[cfg(test)]
use axon_adapters::NoopSourceEnricher;
use axon_adapters::SourceEnricher;
use axon_adapters::boundary::{FetchProvider, RenderProvider};
#[cfg(test)]
use axon_adapters::providers::{
    chrome_render::{ChromeRenderConfig, ChromeRenderProvider},
    http_fetch::{HttpFetchConfig, HttpFetchProvider},
};
use axon_api::source::{JobKind, ProviderId};
use axon_core::boundary::{ArtifactStore, DocumentCache};
use axon_core::config::Config;
use axon_embedding::provider::EmbeddingProvider;
#[cfg(test)]
use axon_embedding::reservation::ProviderReservationConfig;
use axon_embedding::reservation::ProviderReservationManager;
use axon_jobs::boundary::JobStore;
use axon_ledger::store::LedgerStore;
use axon_vectors::store::VectorStore;

mod target_runtime;

pub use target_runtime::{TargetReadStores, build_read_stores_from_config};

#[derive(Clone)]
pub struct ServiceContext {
    pub cfg: Arc<Config>,
    pub jobs: Arc<dyn ServiceJobRuntime>,
    target_local_source: Option<Arc<TargetLocalSourceRuntime>>,
}

#[derive(Clone)]
pub struct TargetLocalSourceRuntime {
    pub jobs: Arc<dyn JobStore>,
    pub ledger: Arc<dyn LedgerStore>,
    pub embedding_provider: Arc<dyn EmbeddingProvider>,
    pub vector_store: Arc<dyn VectorStore>,
    pub embedding_provider_id: ProviderId,
    pub vector_provider_id: ProviderId,
    pub embedding_model: String,
    pub embedding_dimensions: u32,
    pub embedding_reservations: Arc<ProviderReservationManager>,
    pub vector_reservations: Arc<ProviderReservationManager>,
    pub artifact_store: Arc<dyn ArtifactStore>,
    pub document_cache: Arc<dyn DocumentCache>,
    /// Real acquisition boundary for `WebSourceAdapter` (issue #298 Wave 1b) —
    /// `dispatch_web` threads these into `WebSourceIndexInput` instead of
    /// running a `crawl_for_source` acquisition pre-pass.
    pub fetch_provider: Arc<dyn FetchProvider>,
    pub render_provider: Arc<dyn RenderProvider>,
    /// Enrichment-stage boundary (source-pipeline.md: `enriching`, between
    /// `fetching`/`acquire` and `normalizing`/`normalize`). Defaults to
    /// [`NoopSourceEnricher`] — the stage is wired end-to-end (see the git
    /// family's `prepare_changed_documents`) but every concrete enricher is a
    /// no-op passthrough until per-source-kind enrichers land (bead pmj7w).
    pub enricher: Arc<dyn SourceEnricher>,
}

impl TargetLocalSourceRuntime {
    #[cfg(test)]
    pub fn new(
        jobs: Arc<dyn JobStore>,
        ledger: Arc<dyn LedgerStore>,
        embedding_provider: Arc<dyn EmbeddingProvider>,
        vector_store: Arc<dyn VectorStore>,
        embedding_provider_id: ProviderId,
        embedding_model: impl Into<String>,
        embedding_dimensions: u32,
    ) -> Self {
        Self {
            jobs,
            ledger,
            embedding_provider,
            vector_store,
            embedding_reservations: Arc::new(ProviderReservationManager::new(
                ProviderReservationConfig {
                    provider_id: embedding_provider_id.clone(),
                    provider_kind: axon_api::source::ProviderKind::Embedding,
                    capacity: 2,
                    interactive_reserve: 1,
                    cooldown_after_failures: 1,
                    cooldown_secs: 30,
                },
            )),
            vector_reservations: Arc::new(ProviderReservationManager::new(
                ProviderReservationConfig {
                    provider_id: ProviderId::new("target-local-vector"),
                    provider_kind: axon_api::source::ProviderKind::Vector,
                    capacity: 2,
                    interactive_reserve: 1,
                    cooldown_after_failures: 1,
                    cooldown_secs: 30,
                },
            )),
            vector_provider_id: ProviderId::new("target-local-vector"),
            embedding_provider_id,
            embedding_model: embedding_model.into(),
            embedding_dimensions,
            artifact_store: Arc::new(axon_core::boundary::FakeCoreBoundaries::new()),
            document_cache: Arc::new(axon_core::boundary::FakeCoreBoundaries::new()),
            fetch_provider: Arc::new(HttpFetchProvider::new(HttpFetchConfig::default())),
            render_provider: Arc::new(ChromeRenderProvider::new(ChromeRenderConfig::default())),
            enricher: Arc::new(NoopSourceEnricher::new()),
        }
    }
}

impl ServiceContext {
    async fn build(
        cfg: Arc<Config>,
        spawn_workers: bool,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        if spawn_workers {
            axon_core::health::assert_workers_allowed_by_cutover(&cfg)
                .await
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.into() })?;
        }
        let jobs = resolve_runtime_with_workers(Arc::clone(&cfg), spawn_workers).await?;
        let target_local_source = Self::build_target_local_source(&cfg, &jobs, spawn_workers).await;
        let context = Self {
            cfg: Arc::clone(&cfg),
            jobs: Arc::clone(&jobs),
            target_local_source,
        };
        if spawn_workers {
            spawn_queue_summary_logger(Arc::clone(&jobs), cfg.queue_summary_secs);
        }
        Ok(context)
    }

    /// Construct the production target local-source runtime, when applicable.
    ///
    /// Only worker-bearing contexts (`spawn_workers`, i.e. `serve`/`mcp` and
    /// foreground `--wait`) attach it, and only when both `qdrant_url` and
    /// `tei_url` are configured. Missing endpoints leave it unset rather than
    /// failing startup; a construction error (e.g. the ledger migrations) is
    /// logged and treated as absent so the process still comes up.
    async fn build_target_local_source(
        cfg: &Config,
        jobs: &Arc<dyn ServiceJobRuntime>,
        spawn_workers: bool,
    ) -> Option<Arc<TargetLocalSourceRuntime>> {
        if !spawn_workers || cfg.qdrant_url.trim().is_empty() || cfg.tei_url.trim().is_empty() {
            return None;
        }
        let Some(pool) = jobs.sqlite_pool() else {
            return None;
        };
        // Bind the durable observability sink to the SAME shared pool. Its
        // tables are created by the composed migration runner
        // (`apply_all_migrations`), so use the migration-free constructor to
        // avoid colliding with that runner's bookkeeping. Every status/heartbeat
        // transition routed through this store now also lands in
        // `axon_observe_events`/`axon_observe_heartbeats` with a
        // strictly-increasing per-job sequence, supplementing (not replacing) the
        // existing `job_events`/`progress_json` SSE/status streams.
        let observe_sink = Arc::new(
            axon_observe::sink::SqliteObservabilitySink::from_migrated_pool((*pool).clone()),
        );
        let store: Arc<dyn JobStore> = Arc::new(
            axon_jobs::unified::SqliteUnifiedJobStore::with_observe_sink(
                (*pool).clone(),
                observe_sink,
            ),
        );
        match TargetLocalSourceRuntime::from_config(cfg, store, (*pool).clone()).await {
            Ok(runtime) => Some(Arc::new(runtime)),
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    "failed to construct target local-source runtime; continuing without it"
                );
                None
            }
        }
    }

    /// Create a ServiceContext without in-process workers (enqueue-only in the SQLite runtime).
    ///
    /// This is the safe default for CLI commands that enqueue and exit.
    /// Use `new_with_workers()` for long-lived processes that should process jobs.
    pub async fn new(cfg: Arc<Config>) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Self::build(cfg, false).await
    }

    /// Create a ServiceContext with in-process workers (SQLite runtime only).
    ///
    /// Use for foreground CLI `--wait true`, where jobs should drain but
    /// unrelated recurring freshness schedules must not be swept.
    pub async fn new_with_workers(
        cfg: Arc<Config>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Self::build(cfg, true).await
    }

    /// Create a long-lived ServiceContext with in-process workers.
    ///
    /// Use for `axon serve`, MCP server, and web server runtimes.
    pub async fn new_with_workers_and_schedulers(
        cfg: Arc<Config>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Self::build(cfg, true).await
    }

    /// Factory for test helpers — inject a mock `ServiceJobRuntime`.
    pub fn from_runtime(cfg: Arc<Config>, jobs: Arc<dyn ServiceJobRuntime>) -> Self {
        Self {
            cfg,
            jobs,
            target_local_source: None,
        }
    }

    pub fn with_jobs_runtime(mut self, jobs: Arc<dyn ServiceJobRuntime>) -> Self {
        self.jobs = jobs;
        self
    }

    /// Inject the target source runtime.
    ///
    /// Used both by tests (with fakes via the `#[cfg(test)]`
    /// [`TargetLocalSourceRuntime::new`]) and by production startup (with the
    /// real stores via [`TargetLocalSourceRuntime::from_config`]).
    pub fn with_target_local_source_runtime(mut self, runtime: TargetLocalSourceRuntime) -> Self {
        self.target_local_source = Some(Arc::new(runtime));
        self
    }

    pub fn target_local_source_runtime(&self) -> Option<&TargetLocalSourceRuntime> {
        self.target_local_source.as_deref()
    }

    pub fn job_store(&self) -> Option<Arc<dyn JobStore>> {
        self.jobs.unified_job_store()
    }

    /// Wake the unified durable-job worker so a freshly enqueued job is
    /// claimed on its next wakeup instead of waiting out the poll interval.
    /// No-op for enqueue-only runtimes (no in-process workers).
    pub fn notify_unified(&self) {
        self.jobs.notify_unified();
    }

    /// Convenience accessor for the resolved config (A-H1).
    ///
    /// Read/RAG service functions (`query`, `ask`, `retrieve`, …) take `&Config`
    /// directly — use this when you only have a `&ServiceContext` but need to
    /// call a Tier-2 service fn without `Arc::clone`.
    ///
    /// See the Two-Tier Signature Convention in `src/services/CLAUDE.md`.
    pub fn cfg(&self) -> &Config {
        &self.cfg
    }
}

/// Periodic queue-depth summary logger for log-based monitoring.
///
/// Spawned only by worker-bearing contexts. Interval is `AXON_QUEUE_SUMMARY_SECS`
/// (default 30s).
fn spawn_queue_summary_logger(jobs: Arc<dyn ServiceJobRuntime>, secs: u64) {
    if secs == 0 {
        return;
    }
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(secs));
        // Skip the initial fire so the first log is one period in, not at startup.
        interval.tick().await;
        loop {
            interval.tick().await;
            let Some(source) = queue_depth(&jobs, JobKind::Source).await else {
                continue;
            };
            let Some(extract) = queue_depth(&jobs, JobKind::Extract).await else {
                continue;
            };
            let Some(watch) = queue_depth(&jobs, JobKind::Watch).await else {
                continue;
            };
            let Some(prune) = queue_depth(&jobs, JobKind::Prune).await else {
                continue;
            };
            tracing::info!(
                source,
                extract,
                watch,
                prune,
                interval_secs = secs,
                "job queue summary"
            );
        }
    });
}

async fn queue_depth(jobs: &Arc<dyn ServiceJobRuntime>, kind: JobKind) -> Option<i64> {
    match jobs.count_jobs(kind).await {
        Ok(count) => Some(count),
        Err(err) => {
            tracing::warn!(
                ?kind,
                error = %err,
                "failed to read job queue depth"
            );
            None
        }
    }
}
