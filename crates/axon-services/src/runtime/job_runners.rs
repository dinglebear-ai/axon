//! Concrete [`UnifiedJobRunner`] implementations for unified `JobKind`s whose
//! real domain logic lives in `axon-services`.
//!
//! `axon-jobs` cannot depend on `axon-services` (layering rule enforced by
//! `cargo xtask check-layering`), so the unified worker's claim/dispatch loop
//! executes job kinds through an injected [`JobRunnerRegistry`] trait-object
//! seam instead of calling into this crate directly. This module builds the
//! concrete runners and the registry that carries them, and
//! [`super::resolve_runtime_with_workers`] hands the registry to
//! `SqliteJobBackend::new_with_workers_and_registry` at composition time.
//!
//! Scope: this wave wires `ProviderProbe` (backed by the real
//! `system::doctor::doctor` connectivity check) and `Memory` (backed by real
//! `SqliteMemoryStore::compact`/`import` calls — see [`MemoryCompactionRunner`]).
//! `GraphMutation`/`Prune`/`Watch` are intentionally left unregistered — they
//! run as sub-steps of a parent operation or have their own scheduler, and
//! forcing them through this seam here risks a rushed, wrong implementation
//! of the trickiest cases.

use std::sync::Arc;

use async_trait::async_trait;
use axon_api::source::{
    ApiError, ErrorStage, JobHeartbeat, JobKind, LifecycleStatus, PipelinePhase, Timestamp,
};
use axon_core::config::Config;
use axon_core::logging::log_warn;
use axon_jobs::boundary::JobStore;
use axon_jobs::config_snapshot::apply_config_snapshot;
use axon_jobs::unified::SqliteUnifiedJobStore;
use axon_jobs::workers::unified::UnifiedClaimedJob;
use axon_jobs::workers::{JobRunnerRegistry, UnifiedJobRunner};
use axon_memory::record::SystemClock;
use axon_memory::sqlite::SqliteMemoryStore;
use axon_memory::store::MemoryStore;
use tokio_util::sync::CancellationToken;

mod crawl_runner;
mod ingest_runner;
mod source_runner;
use crawl_runner::CrawlRunner;
use ingest_runner::IngestRunner;
use source_runner::SourceRunner;

/// Build the [`JobRunnerRegistry`] handed to the unified worker at
/// composition time. Additive by design — any kind not registered here keeps
/// falling back to `job_runner.unsupported_stage`, so this function can only
/// ever make more kinds executable, never fewer.
///
/// Returns an error only if opening the memory store fails outright (bad
/// path, unwritable directory, …) — callers should treat that as fatal for
/// the `Memory` runner rather than silently registering a broken one.
pub fn build_registry(cfg: &Arc<Config>) -> Result<JobRunnerRegistry, ApiError> {
    let mut registry = JobRunnerRegistry::new();
    registry.register(
        JobKind::ProviderProbe,
        Arc::new(ProviderProbeRunner {
            cfg: Arc::clone(cfg),
        }),
    );
    registry.register(
        JobKind::Extract,
        Arc::new(ExtractRunner {
            cfg: Arc::clone(cfg),
        }),
    );
    registry.register(
        JobKind::Embed,
        Arc::new(EmbedRunner {
            cfg: Arc::clone(cfg),
        }),
    );
    registry.register(
        JobKind::Crawl,
        Arc::new(CrawlRunner {
            cfg: Arc::clone(cfg),
        }),
    );
    registry.register(
        JobKind::Ingest,
        Arc::new(IngestRunner {
            cfg: Arc::clone(cfg),
        }),
    );
    registry.register(
        JobKind::Source,
        Arc::new(SourceRunner::new(Arc::clone(cfg))),
    );

    // Open once and reuse: `SqliteMemoryStore::open` runs a schema migration
    // via a bare `rusqlite::Connection` with no busy-timeout configured. Doing
    // this on every job execution races the shared sqlx pool that already
    // holds this same SQLite file open for the unified job store, producing
    // intermittent "database is locked" failures under concurrent load.
    // Opening once at registry-build time and reusing the connection avoids
    // the repeated open/migrate race entirely.
    let path = cfg.sqlite_path.to_string_lossy().to_string();
    let memory_store = SqliteMemoryStore::open(&path, Arc::new(SystemClock))
        .map_err(|error| compaction_error(format!("open memory store: {}", error.message)))?;
    registry.register(
        JobKind::Memory,
        Arc::new(MemoryCompactionRunner {
            memory_store: Arc::new(memory_store),
        }),
    );

    Ok(registry)
}

pub(crate) async fn heartbeat_running(
    store: &SqliteUnifiedJobStore,
    claimed: &UnifiedClaimedJob,
    phase: PipelinePhase,
) {
    if let Err(error) = store
        .heartbeat(JobHeartbeat {
            job_id: claimed.job_id,
            attempt: claimed.attempt,
            worker_id: Some("unified-local-worker".to_string()),
            phase,
            status: LifecycleStatus::Running,
            stage_id: None,
            heartbeat_at: Timestamp::from(chrono::Utc::now()),
            sequence: 0,
            last_progress_at: None,
            last_event_sequence: None,
            counts: None,
            provider_reservations: Vec::new(),
        })
        .await
    {
        // Swallowed by design (heartbeats are best-effort), but a silent
        // failure here makes stale-job reclaim undebuggable — log it.
        log_warn(&format!(
            "heartbeat failed for job {} attempt {} phase {:?}: {error}",
            claimed.job_id.0, claimed.attempt, phase
        ));
    }
}

/// Runs the real Qdrant/TEI/LLM connectivity check (`system::doctor::doctor`)
/// for a `ProviderProbe` job. Safe and idempotent — it only reads service
/// health, never mutates state.
struct ProviderProbeRunner {
    cfg: Arc<Config>,
}

#[async_trait]
impl UnifiedJobRunner for ProviderProbeRunner {
    async fn run(
        &self,
        claimed: &UnifiedClaimedJob,
        store: &SqliteUnifiedJobStore,
        shutdown: &CancellationToken,
    ) -> Result<(), ApiError> {
        heartbeat_running(store, claimed, PipelinePhase::Evaluating).await;
        if shutdown.is_cancelled() {
            return Err(probe_error("provider probe canceled before running"));
        }
        // Call the untracked inner check directly -- this runner already
        // executes inside an already-tracked unified `provider_probe` job,
        // so going through the public `doctor()` (which wraps itself in a
        // *second* job_tracking::track_operation_job call) would create a
        // duplicate, nested job row for the same probe.
        crate::system::doctor_inner(&self.cfg)
            .await
            .map(|_result| ())
            .map_err(|error| probe_error(error.to_string()))
    }
}

fn probe_error(message: impl Into<String>) -> ApiError {
    ApiError::new(
        "job_runner.provider_probe_failed",
        ErrorStage::Observing,
        message.into(),
    )
}

/// Runs a claimed `Memory` unified job by dispatching on
/// `request_json.operation`:
/// - `"memory_compaction"` — deserializes `request_json.payload` as a
///   [`axon_api::source::MemoryCompactRequest`] and calls the real
///   `SqliteMemoryStore::compact`.
/// - `"memory_import"` — deserializes `request_json.payload` as a
///   [`axon_api::source::MemoryImportRequest`] and calls the real
///   `SqliteMemoryStore::import`.
///
/// `crates/axon-services/src/memory/compact.rs::compact` and
/// `.../import_export.rs::import` embed exactly this `{operation, payload}`
/// shape when they job-track a foreground call (contract R3-16: memory jobs
/// pollable via `job_id`), so a job claimed here — whether created by that
/// foreground path or enqueued directly against the unified store for
/// detached execution — runs the same real domain call either way.
///
/// This runner opens a plain `SqliteMemoryStore` (not the graph/vector-
/// decorated composition `crate::memory::store::memory_store` builds, which
/// needs an async `ServiceContext` this registry-build seam does not have
/// available) — so a compaction executed through this detached path mirrors
/// into the graph and embeds into the vector store only when it *also* runs
/// through the foreground `axon-services::memory` call. A job with no
/// recognized `operation`/`payload` (e.g. a bare smoke-test job) falls back
/// to a safe, idempotent `capabilities()` call rather than failing, so the
/// registry seam itself stays provable independent of a real payload.
struct MemoryCompactionRunner {
    memory_store: Arc<SqliteMemoryStore>,
}

#[async_trait]
impl UnifiedJobRunner for MemoryCompactionRunner {
    async fn run(
        &self,
        claimed: &UnifiedClaimedJob,
        store: &SqliteUnifiedJobStore,
        shutdown: &CancellationToken,
    ) -> Result<(), ApiError> {
        heartbeat_running(store, claimed, PipelinePhase::Preparing).await;
        if shutdown.is_cancelled() {
            return Err(compaction_error(
                "memory compaction canceled before running",
            ));
        }

        let operation = claimed
            .request_json
            .as_ref()
            .and_then(|json| json.get("operation"))
            .and_then(|v| v.as_str());
        let payload = claimed
            .request_json
            .as_ref()
            .and_then(|json| json.get("payload"));

        match (operation, payload) {
            (Some("memory_compaction"), Some(payload)) => {
                let request: axon_api::source::MemoryCompactRequest =
                    serde_json::from_value(payload.clone()).map_err(|error| {
                        compaction_error(format!("invalid memory_compaction payload: {error}"))
                    })?;
                self.memory_store
                    .compact(request)
                    .await
                    .map(|_result| ())
                    .map_err(|error| compaction_error(error.message))
            }
            (Some("memory_import"), Some(payload)) => {
                let request: axon_api::source::MemoryImportRequest =
                    serde_json::from_value(payload.clone()).map_err(|error| {
                        compaction_error(format!("invalid memory_import payload: {error}"))
                    })?;
                self.memory_store
                    .import(request)
                    .await
                    .map(|_result| ())
                    .map_err(|error| compaction_error(error.message))
            }
            _ => self
                .memory_store
                .capabilities()
                .await
                .map(|_capability| ())
                .map_err(|error| compaction_error(error.message)),
        }
    }
}

fn compaction_error(message: impl Into<String>) -> ApiError {
    ApiError::new(
        "job_runner.memory_compaction_failed",
        ErrorStage::Preparing,
        message.into(),
    )
}

/// Runs a claimed `Extract` unified job via `crate::extract::extract_sync`.
///
/// Replaces the old special-cased dispatch (`axon-jobs` calling directly
/// into the now-removed `axon-extract` crate — Phase 12 clean break) with the
/// same dependency-inversion seam every other axon-services-backed job kind
/// uses. `claimed.request_json` carries `{"urls": [...], "config_json": "..."}`.
struct ExtractRunner {
    cfg: Arc<Config>,
}

#[async_trait]
impl UnifiedJobRunner for ExtractRunner {
    async fn run(
        &self,
        claimed: &UnifiedClaimedJob,
        store: &SqliteUnifiedJobStore,
        shutdown: &CancellationToken,
    ) -> Result<(), ApiError> {
        heartbeat_running(store, claimed, PipelinePhase::Parsing).await;
        if shutdown.is_cancelled() {
            return Err(extract_error("extract canceled before running"));
        }

        let request = claimed
            .request_json
            .as_ref()
            .ok_or_else(|| extract_error("extract job has no request payload"))?;
        let urls: Vec<String> = request
            .get("urls")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .ok_or_else(|| extract_error("extract job request is missing a `urls` array"))?;
        let config_json = request
            .get("config_json")
            .and_then(|v| v.as_str())
            .unwrap_or_default();

        let mut effective_cfg = apply_config_snapshot(&self.cfg, config_json).map_err(|error| {
            ApiError::new(
                "job_runner.invalid_config_snapshot",
                ErrorStage::Planning,
                error.to_string(),
            )
        })?;
        effective_cfg.output_dir = effective_cfg
            .output_dir
            .join("extract-jobs")
            .join(claimed.job_id.0.to_string());
        effective_cfg.output_path = None;

        let prompt = effective_cfg.query.clone().unwrap_or_default();
        let extract_fut = crate::extract::extract_sync(&effective_cfg, &urls, &prompt);
        tokio::select! {
            _ = shutdown.cancelled() => Err(extract_error("extract canceled")),
            result = extract_fut => result.map(|_summary| ()).map_err(|error| extract_error(error.to_string())),
        }
    }
}

fn extract_error(message: impl Into<String>) -> ApiError {
    ApiError::new(
        "job_runner.extract_failed",
        ErrorStage::ParsingContent,
        message.into(),
    )
}

/// Runs a claimed `Embed` unified job via `crate::embed::local_write::embed_local_path`
/// (the ledger-tracked `local_source` pipeline — `axon-document` +
/// `axon-embedding` + `axon-vectors`).
///
/// `claimed.request_json` carries `{"input": "...", "config_json": "..."}`
/// (see `embed_start_with_context` in `crates/axon-services/src/embed.rs`).
struct EmbedRunner {
    cfg: Arc<Config>,
}

#[async_trait]
impl UnifiedJobRunner for EmbedRunner {
    async fn run(
        &self,
        claimed: &UnifiedClaimedJob,
        store: &SqliteUnifiedJobStore,
        shutdown: &CancellationToken,
    ) -> Result<(), ApiError> {
        heartbeat_running(store, claimed, PipelinePhase::Embedding).await;
        if shutdown.is_cancelled() {
            return Err(embed_error("embed canceled before running"));
        }
        let request = claimed
            .request_json
            .as_ref()
            .ok_or_else(|| embed_error("embed job has no request payload"))?;
        let input = request
            .get("input")
            .and_then(|v| v.as_str())
            .ok_or_else(|| embed_error("embed job request is missing `input`"))?
            .to_string();
        let config_json = request
            .get("config_json")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let effective_cfg = apply_config_snapshot(&self.cfg, config_json).map_err(|error| {
            ApiError::new(
                "job_runner.invalid_config_snapshot",
                ErrorStage::Planning,
                error.to_string(),
            )
        })?;
        let embed_fut = crate::embed::local_write::embed_local_path(&effective_cfg, &input, None);
        tokio::select! {
            _ = shutdown.cancelled() => Err(embed_error("embed canceled")),
            result = embed_fut => result.map(|_output| ()).map_err(|error| embed_error(error.to_string())),
        }
    }
}

fn embed_error(message: impl Into<String>) -> ApiError {
    ApiError::new(
        "job_runner.embed_failed",
        ErrorStage::Embedding,
        message.into(),
    )
}

#[cfg(test)]
#[path = "job_runners_tests.rs"]
mod tests;
