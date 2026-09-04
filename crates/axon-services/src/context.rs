use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::runtime::{ServiceJobRuntime, resolve_runtime_with_workers};
use axon_adapters::boundary::{FetchProvider, RenderProvider};
#[cfg(test)]
use axon_adapters::providers::{
    chrome_render::{ChromeRenderConfig, ChromeRenderProvider},
    http_fetch::{HttpFetchConfig, HttpFetchProvider},
};
use axon_adapters::{ArtifactCandidateSink, SourceAdapter, SourceAdapterRegistry, SourceEnricher};
#[cfg(test)]
use axon_adapters::{NoopArtifactCandidateSink, NoopSourceEnricher, web::WebSourceAdapter};
use axon_api::source::{JobKind, ProviderId};
use axon_core::boundary::{ArtifactStore, DocumentCache};
use axon_core::config::Config;
use axon_document::DocumentPreparer;
use axon_embedding::provider::EmbeddingProvider;
use axon_jobs::boundary::JobStore;
#[cfg(test)]
use axon_jobs::embedding_cache_store::SqliteEmbeddingVectorCacheStore;
use axon_jobs::scheduler::ProviderScheduler;
#[cfg(test)]
use axon_jobs::scheduler::SqliteWriteGate;
use axon_ledger::store::LedgerStore;
use axon_vectors::store::VectorStore;
use tokio::sync::{OnceCell, Semaphore};

use self::db_limited_ledger::DbLimitedLedgerStore;
use crate::artifact_candidate_outbox::SharedArtifactCandidateOutbox;

mod db_limited_ledger;
mod scheduled_web;
mod target_runtime;

pub use target_runtime::{
    TargetReadStores, build_read_stores_from_config, invalidate_embedding_identity_cache,
};

#[derive(Clone)]
pub struct ServiceContext {
    pub cfg: Arc<Config>,
    pub jobs: Arc<dyn ServiceJobRuntime>,
    target_local_source: Option<Arc<TargetLocalSourceRuntime>>,
    /// Held for the lifetime of a long-lived schedulers context (`serve` /
    /// HTTP `mcp`) to announce worker liveness to detached CLI enqueues, so a
    /// running server suppresses redundant auto-spawned workers
    /// (`axon_rust-x4gxr.2`). `None` for enqueue-only contexts and short-lived
    /// `--wait` contexts, which must not hold it. Best-effort: acquisition
    /// failure leaves it `None` and never blocks startup — job-claim
    /// correctness does not depend on the lock.
    ///
    /// Held purely for its RAII effect: the lock releases when the last
    /// `ServiceContext` clone drops (i.e. at server shutdown). Never read.
    #[allow(dead_code)]
    drain_lock: Option<Arc<crate::runtime::WorkerDrainLock>>,
    #[allow(dead_code)]
    queue_summary: Option<Arc<QueueSummaryTask>>,
    cleanup_debt: Option<Arc<QueueSummaryTask>>,
}

pub(crate) struct QueueSummaryTask {
    stop: std::sync::mpsc::Sender<()>,
    thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl QueueSummaryTask {
    pub(crate) fn new(
        stop: std::sync::mpsc::Sender<()>,
        thread: std::thread::JoinHandle<()>,
    ) -> Self {
        Self {
            stop,
            thread: Mutex::new(Some(thread)),
        }
    }

    async fn shutdown(&self) {
        self.stop_and_join();
    }

    fn stop_and_join(&self) {
        let _ = self.stop.send(());
        if let Some(thread) = self.thread.lock().expect("queue summary task lock").take() {
            let _ = thread.join();
        }
    }
}

impl Drop for QueueSummaryTask {
    fn drop(&mut self) {
        self.stop_and_join();
    }
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
    pub document_preparer: DocumentPreparer,
    pub document_prepare_concurrency: usize,
    pub embed_pool_max_inputs: usize,
    pub embed_prepared_byte_budget: usize,
    pub(crate) db_stage_slots: Arc<Semaphore>,
    pub embedding_scheduler: Option<Arc<ProviderScheduler>>,
    pub vector_scheduler: Option<Arc<ProviderScheduler>>,
    pub parse_scheduler: Option<Arc<ProviderScheduler>>,
    pub graph_scheduler: Option<Arc<ProviderScheduler>>,
    pub artifact_scheduler: Option<Arc<ProviderScheduler>>,
    #[cfg(test)]
    pub(crate) sqlite_write_gate: SqliteWriteGate,
    #[cfg(test)]
    pub(crate) embedding_cache_store: Option<Arc<SqliteEmbeddingVectorCacheStore>>,
    pub artifact_store: Arc<dyn ArtifactStore>,
    pub document_cache: Arc<dyn DocumentCache>,
    /// Optional evidence delivery boundary for ArtifactCandidate batches. The
    /// production default is a no-op sink so existing SourceRequest/RAG behavior
    /// is unchanged unless a sink (for example Depot) is explicitly configured.
    pub artifact_candidate_sink: Arc<dyn ArtifactCandidateSink>,
    pub(crate) artifact_candidate_outbox: Option<SharedArtifactCandidateOutbox>,
    source_adapters: Arc<OnceCell<SourceAdapterRegistry>>,
    pub(crate) web_source_adapter: Arc<dyn SourceAdapter>,
    /// Real acquisition boundaries injected into the canonical web adapter.
    /// The family-blind source executor never performs an out-of-band crawl.
    pub fetch_provider: Arc<dyn FetchProvider>,
    pub render_provider: Arc<dyn RenderProvider>,
    /// Enrichment-stage boundary (source-pipeline.md: `enriching`, between
    /// `fetching`/`acquire` and `normalizing`/`normalize`). Defaults to
    /// `NoopSourceEnricher` — the stage is wired end-to-end (see the git
    /// family's `prepare_changed_documents`) but every concrete enricher is a
    /// no-op passthrough until per-source-kind enrichers land (bead pmj7w).
    pub enricher: Arc<dyn SourceEnricher>,
}

impl TargetLocalSourceRuntime {
    pub fn with_artifact_candidate_sink(mut self, sink: Arc<dyn ArtifactCandidateSink>) -> Self {
        self.artifact_candidate_sink = sink;
        self
    }

    pub(crate) async fn source_adapter_registry(
        &self,
        ctx: &ServiceContext,
    ) -> anyhow::Result<&SourceAdapterRegistry> {
        self.source_adapters
            .get_or_try_init(|| {
                crate::source::adapter_registry::build_source_adapter_registry(ctx, self)
            })
            .await
    }

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
        let fetch_provider: Arc<dyn FetchProvider> =
            Arc::new(HttpFetchProvider::new(HttpFetchConfig::default()));
        let render_provider: Arc<dyn RenderProvider> =
            Arc::new(ChromeRenderProvider::new(ChromeRenderConfig::default()));
        let web_source_adapter: Arc<dyn SourceAdapter> = Arc::new(WebSourceAdapter::new(
            Arc::clone(&fetch_provider),
            Arc::clone(&render_provider),
        ));
        let db_stage_slots = Arc::new(Semaphore::new(1));
        Self {
            jobs,
            ledger: Arc::new(DbLimitedLedgerStore::new(
                ledger,
                Arc::clone(&db_stage_slots),
            )),
            embedding_provider,
            vector_store,
            vector_provider_id: ProviderId::new("target-local-vector"),
            embedding_scheduler: None,
            vector_scheduler: None,
            parse_scheduler: None,
            graph_scheduler: None,
            artifact_scheduler: None,
            sqlite_write_gate: SqliteWriteGate::default(),
            embedding_cache_store: None,
            embedding_provider_id,
            embedding_model: embedding_model.into(),
            embedding_dimensions,
            document_preparer: DocumentPreparer::default(),
            document_prepare_concurrency: 1,
            embed_pool_max_inputs: 512,
            embed_prepared_byte_budget: 128 * 1024 * 1024,
            db_stage_slots,
            artifact_store: Arc::new(axon_core::boundary::FakeCoreBoundaries::new()),
            document_cache: Arc::new(axon_core::boundary::FakeCoreBoundaries::new()),
            artifact_candidate_sink: Arc::new(NoopArtifactCandidateSink),
            artifact_candidate_outbox: None,
            source_adapters: Arc::new(OnceCell::new()),
            web_source_adapter,
            fetch_provider,
            render_provider,
            enricher: Arc::new(NoopSourceEnricher::new()),
        }
    }
}

impl ServiceContext {
    async fn build(
        cfg: Arc<Config>,
        spawn_workers: bool,
        hold_drain_lock: bool,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        if spawn_workers {
            axon_core::health::assert_workers_allowed_by_cutover(&cfg)
                .await
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.into() })?;
        }
        let jobs = resolve_runtime_with_workers(Arc::clone(&cfg), spawn_workers).await?;
        let target_local_source =
            Self::build_target_local_source(&cfg, &jobs, spawn_workers).await?;
        // A long-lived schedulers context (serve / HTTP mcp) owns the queue for
        // its whole lifetime, so it holds the drain lock to advertise that —
        // otherwise every detached CLI enqueue would auto-spawn a redundant
        // worker (axon_rust-x4gxr.2). Short-lived `--wait` worker contexts must
        // NOT hold it. Best-effort: if another worker already holds it, we start
        // anyway (claiming is transactional).
        let drain_lock = if hold_drain_lock {
            Self::acquire_drain_lock(&cfg).await
        } else {
            None
        };
        let mut context = Self {
            cfg: Arc::clone(&cfg),
            jobs: Arc::clone(&jobs),
            target_local_source,
            drain_lock,
            queue_summary: if spawn_workers {
                match spawn_queue_summary_logger(Arc::clone(&jobs), cfg.queue_summary_secs) {
                    Ok(task) => task,
                    Err(error) => {
                        tracing::warn!(%error, "queue summary monitor failed to start; continuing without it");
                        None
                    }
                }
            } else {
                None
            },
            cleanup_debt: None,
        };
        if spawn_workers && let Some(runtime) = context.target_local_source.as_deref() {
            let registry = runtime.source_adapter_registry(&context).await?.clone();
            context.cleanup_debt = Some(
                crate::reserved_call::spawn_cleanup_debt_worker(&context, runtime, registry)
                    .await?,
            );
        }
        Ok(context)
    }

    /// Best-effort acquire the worker drain lock for a long-lived schedulers
    /// context. Returns `None` (and logs) when another worker process already
    /// holds it or the lock DB can't be opened — the server still runs, it just
    /// doesn't advertise liveness, so a detached enqueue may auto-spawn one
    /// extra worker (correct, only mildly wasteful).
    async fn acquire_drain_lock(cfg: &Config) -> Option<Arc<crate::runtime::WorkerDrainLock>> {
        let lock_path = crate::runtime::drain_lock_path(&cfg.sqlite_path);
        match crate::runtime::WorkerDrainLock::try_hold(&lock_path).await {
            Ok(Some(lock)) => Some(Arc::new(lock)),
            Ok(None) => {
                tracing::info!(
                    "worker drain lock already held by another process; \
                     server runs without advertising liveness"
                );
                None
            }
            Err(error) => {
                tracing::warn!(%error, "failed to acquire worker drain lock; continuing");
                None
            }
        }
    }

    /// Construct the production target local-source runtime, when applicable.
    ///
    /// Only worker-bearing contexts (`spawn_workers`, i.e. `serve`/`mcp` and
    /// foreground `--wait`) attach it. Provider construction is lazy, so
    /// acquisition-only requests such as `map` remain operational without TEI
    /// or Qdrant; an embedding request still fails at the provider boundary
    /// when its configured endpoint is unavailable.
    async fn build_target_local_source(
        cfg: &Config,
        jobs: &Arc<dyn ServiceJobRuntime>,
        spawn_workers: bool,
    ) -> Result<Option<Arc<TargetLocalSourceRuntime>>, Box<dyn std::error::Error + Send + Sync>>
    {
        if !spawn_workers {
            return Ok(None);
        }
        let Some(pool) = jobs.sqlite_pool() else {
            return Ok(None);
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
        let runtime = TargetLocalSourceRuntime::from_config(cfg, store, (*pool).clone()).await?;
        crate::source::spawn_artifact_candidate_outbox_drain(&runtime);
        Ok(Some(Arc::new(runtime)))
    }

    /// Create a ServiceContext without in-process workers (enqueue-only in the SQLite runtime).
    ///
    /// This is the safe default for CLI commands that enqueue and exit.
    /// Use `new_with_workers()` for long-lived processes that should process jobs.
    pub async fn new(cfg: Arc<Config>) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Self::build(cfg, false, false).await
    }

    /// Create a ServiceContext with in-process workers (SQLite runtime only).
    ///
    /// Use for foreground CLI `--wait true` and the standalone `axon jobs
    /// worker` (which owns the drain lock itself). Does NOT hold the drain lock:
    /// a short-lived `--wait` context must not suppress auto-spawn.
    pub async fn new_with_workers(
        cfg: Arc<Config>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Self::build(cfg, true, false).await
    }

    /// Create a long-lived ServiceContext with in-process workers that holds the
    /// worker drain lock for its lifetime.
    ///
    /// Use for `axon serve`, MCP server, and web server runtimes — the running
    /// server advertises worker liveness so detached CLI enqueues don't
    /// auto-spawn a redundant worker (axon_rust-x4gxr.2).
    pub async fn new_with_workers_and_schedulers(
        cfg: Arc<Config>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Self::build(cfg, true, true).await
    }

    /// Factory for test helpers — inject a mock `ServiceJobRuntime`.
    pub fn from_runtime(cfg: Arc<Config>, jobs: Arc<dyn ServiceJobRuntime>) -> Self {
        Self {
            cfg,
            jobs,
            target_local_source: None,
            drain_lock: None,
            queue_summary: None,
            cleanup_debt: None,
        }
    }

    pub fn with_jobs_runtime(mut self, jobs: Arc<dyn ServiceJobRuntime>) -> Self {
        self.jobs = jobs;
        self
    }

    /// Inject the target source runtime.
    ///
    /// Used both by tests (with fakes via the `#[cfg(test)]`
    /// test-only `TargetLocalSourceRuntime` constructor) and by production startup (with the
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

    /// Shared SQLite scheduler/job pool for durable provider reservations.
    pub fn sqlite_pool(&self) -> Option<Arc<sqlx::SqlitePool>> {
        self.jobs.sqlite_pool()
    }

    pub fn foreground_event_store(
        &self,
    ) -> Option<crate::source::foreground_progress::ForegroundEventStore> {
        self.job_store()
            .map(crate::source::foreground_progress::ForegroundEventStore::new)
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

    /// Cancel and join background tasks owned by this context.
    pub async fn shutdown_background_tasks(&self) {
        if let Some(task) = &self.queue_summary {
            task.shutdown().await;
        }
        if let Some(task) = &self.cleanup_debt {
            task.shutdown().await;
        }
    }
}

#[cfg(test)]
fn spawn_adapter_cleanup_worker_with_runtime(
    ledger: Arc<dyn LedgerStore>,
    registry: SourceAdapterRegistry,
    runtime: std::io::Result<tokio::runtime::Runtime>,
    thread_name: &str,
) -> std::io::Result<Arc<QueueSummaryTask>> {
    let runtime = runtime?;
    if thread_name.as_bytes().contains(&0) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "background worker thread name contains a null byte",
        ));
    }
    let (stop, stopped) = std::sync::mpsc::channel();
    let thread = std::thread::Builder::new()
        .name(thread_name.into())
        .spawn(move || {
            let mut delay = Duration::from_millis(100);
            loop {
                match stopped.recv_timeout(delay) {
                    Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                }
                let next = runtime.block_on(crate::source::prune::drain_due_adapter_releases(
                    ledger.as_ref(),
                    &registry,
                    256,
                ));
                delay = next
                    .and_then(|next| chrono::DateTime::parse_from_rfc3339(&next.0).ok())
                    .map(|next| {
                        (next.with_timezone(&chrono::Utc) - chrono::Utc::now())
                            .to_std()
                            .unwrap_or(Duration::from_millis(100))
                    })
                    .unwrap_or(Duration::from_secs(30))
                    .max(Duration::from_millis(100));
            }
        })?;
    Ok(Arc::new(QueueSummaryTask::new(stop, thread)))
}

/// Periodic queue-depth summary logger for log-based monitoring.
///
/// Spawned only by worker-bearing contexts. Interval is `AXON_QUEUE_SUMMARY_SECS`
/// (default 30s).
fn spawn_queue_summary_logger(
    jobs: Arc<dyn ServiceJobRuntime>,
    secs: u64,
) -> std::io::Result<Option<Arc<QueueSummaryTask>>> {
    spawn_queue_summary_logger_with_runtime(
        jobs,
        secs,
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build(),
        "axon-queue-summary",
    )
}

fn spawn_queue_summary_logger_with_runtime(
    jobs: Arc<dyn ServiceJobRuntime>,
    secs: u64,
    runtime: std::io::Result<tokio::runtime::Runtime>,
    thread_name: &str,
) -> std::io::Result<Option<Arc<QueueSummaryTask>>> {
    if secs == 0 {
        return Ok(None);
    }
    if thread_name.as_bytes().contains(&0) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "background worker thread name contains a null byte",
        ));
    }
    let (stop, stopped) = std::sync::mpsc::channel();
    let (ready, startup) = std::sync::mpsc::sync_channel(1);
    let thread = std::thread::Builder::new()
        .name(thread_name.into())
        .spawn(move || {
            let runtime = match runtime {
                Ok(runtime) => runtime,
                Err(error) => {
                    let _ = ready.send(Err(error));
                    return;
                }
            };
            if ready.send(Ok(())).is_err() {
                return;
            }
            loop {
                match stopped.recv_timeout(Duration::from_secs(secs)) {
                    Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                }
                runtime.block_on(async {
                    let Some(source) = queue_depth(&jobs, JobKind::Source).await else {
                        return;
                    };
                    let Some(extract) = queue_depth(&jobs, JobKind::Extract).await else {
                        return;
                    };
                    let Some(watch) = queue_depth(&jobs, JobKind::Watch).await else {
                        return;
                    };
                    let Some(prune) = queue_depth(&jobs, JobKind::Prune).await else {
                        return;
                    };
                    tracing::info!(
                        source,
                        extract,
                        watch,
                        prune,
                        interval_secs = secs,
                        "job queue summary"
                    );
                });
            }
        })?;
    match startup.recv() {
        Ok(Ok(())) => Ok(Some(Arc::new(QueueSummaryTask::new(stop, thread)))),
        Ok(Err(error)) => {
            let _ = thread.join();
            Err(error)
        }
        Err(error) => {
            let _ = thread.join();
            Err(std::io::Error::other(format!(
                "queue summary startup handshake failed: {error}"
            )))
        }
    }
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

#[cfg(test)]
#[path = "context_tests.rs"]
mod tests;
