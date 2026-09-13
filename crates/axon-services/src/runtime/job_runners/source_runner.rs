//! [`SourceRunner`]: executes a claimed unified `Source` job.
//!
//! `Source` is the target clean-break job kind for "acquire, normalize, embed,
//! publish one source" (see `docs/pipeline-unification/runtime/job-contract.md`
//! Job Kinds table). Every live entrypoint today (CLI `axon source`, MCP
//! `handlers_source`, `POST /v1/sources`) calls
//! [`crate::source::index_source_with_auth`] inline and blocks on the result —
//! none of them enqueues a detached `Source` row today. This runner is the
//! missing consumer side: it makes a `JobKind::Source` row that *is* enqueued
//! directly against the unified store (today or by a future caller honoring
//! `SourceRequest.execution.mode == Background`) actually run to completion
//! instead of pending forever (audit gap C4-02 / bead `axon_rust-mijoc`).
//!
//! `claimed.request_json` carries `{"source_request": <SourceRequest JSON>}`.
//! The claimed job's own `auth_snapshot` (recorded at enqueue time — never
//! re-derived) is threaded through to `index_source_with_auth` exactly like
//! every other unified runner threads its auth snapshot forward.
//!
//! Building a [`ServiceContext`] here is a deliberate second, lightweight
//! composition: `crate::runtime::job_runners::build_registry` runs *before*
//! the outer `ServiceContext` exists (it is itself an input to constructing
//! the job runtime that becomes part of that context), so this runner cannot
//! borrow the real one. It reuses the claimed worker store's migrated SQLx
//! pool to build an enqueue-only service runtime plus a
//! [`TargetLocalSourceRuntime`] when `qdrant_url`/`tei_url` are configured.
//! Reopening the same SQLite file here would create a competing writer domain
//! outside the provider scheduler's admission gate. The composed context is
//! cached (`tokio::sync::OnceCell`) for subsequent source jobs.

use std::sync::Arc;

use async_trait::async_trait;
use axon_api::source::{
    ApiError, AuthSnapshot, ErrorStage, LifecycleStatus, PipelinePhase, SourceRequest, SourceResult,
};
use axon_core::config::Config;
use axon_jobs::scheduler::SqliteWriteGate;
use axon_jobs::unified::SqliteUnifiedJobStore;
use axon_jobs::workers::unified::UnifiedClaimedJob;
use axon_jobs::workers::{UnifiedJobOutcome, UnifiedJobRunner};
use tokio::sync::OnceCell;
use tokio_util::sync::CancellationToken;

use crate::context::{ServiceContext, TargetLocalSourceRuntime};
use crate::runtime::job_runners::{heartbeat_running, heartbeat_running_preserving_progress};

/// How long a canceled run may keep executing to reach its cooperative
/// cancellation checkpoint and finish failed-generation cleanup before the
/// runner gives up and drops the pipeline future (pre-M3 behavior).
const CANCEL_CLEANUP_GRACE: std::time::Duration = std::time::Duration::from_secs(10);

pub(super) struct SourceRunner {
    cfg: Arc<Config>,
    write_gate: SqliteWriteGate,
    ctx: OnceCell<ServiceContext>,
}

impl SourceRunner {
    #[cfg(test)]
    pub(super) fn new(cfg: Arc<Config>) -> Self {
        Self::new_with_write_gate(cfg, SqliteWriteGate::default())
    }

    pub(super) fn new_with_write_gate(cfg: Arc<Config>, write_gate: SqliteWriteGate) -> Self {
        Self {
            cfg,
            write_gate,
            ctx: OnceCell::new(),
        }
    }

    async fn service_context(
        &self,
        store: &SqliteUnifiedJobStore,
    ) -> Result<&ServiceContext, ApiError> {
        self.ctx
            .get_or_try_init(|| {
                build_service_context_with_write_gate(&self.cfg, store, self.write_gate.clone())
            })
            .await
    }
}

/// Build a lightweight [`ServiceContext`] scoped to this runner from the
/// worker's existing migrated pool: an enqueue-only job runtime (no nested
/// unified worker loop — this runner already *is* the worker executing under
/// one) plus the real
/// [`TargetLocalSourceRuntime`] when the data plane is configured. Absence of
/// `qdrant_url`/`tei_url` is not an error here — `index_source_with_auth`
/// itself degrades cleanly to a `Failed` `SourceResult` when the runtime has
/// no target local-source runtime attached.
async fn build_service_context_with_write_gate(
    cfg: &Arc<Config>,
    store: &SqliteUnifiedJobStore,
    write_gate: SqliteWriteGate,
) -> Result<ServiceContext, ApiError> {
    let pool = Arc::new(store.sqlite_pool().clone());
    let jobs: Arc<dyn crate::runtime::ServiceJobRuntime> = Arc::new(
        crate::runtime::SqliteServiceRuntime::new_for_migrated_pool_with_write_gate(
            Arc::clone(cfg),
            Arc::clone(&pool),
            write_gate.clone(),
        ),
    );
    let mut ctx = ServiceContext::from_runtime(Arc::clone(cfg), Arc::clone(&jobs));

    if cfg.qdrant_url.trim().is_empty() || cfg.tei_url.trim().is_empty() {
        return Ok(ctx);
    }
    let job_store: Arc<dyn axon_jobs::boundary::JobStore> = Arc::new(store.clone());
    match TargetLocalSourceRuntime::from_config_with_write_gate(
        cfg,
        job_store,
        pool.as_ref().clone(),
        write_gate,
    )
    .await
    {
        Ok(runtime) => {
            crate::source::spawn_artifact_candidate_outbox_drain(&runtime);
            ctx = ctx.with_target_local_source_runtime(runtime);
        }
        Err(error) => {
            tracing::warn!(
                error = %error,
                "source runner: failed to construct target local-source runtime; \
                 continuing degraded (source jobs will fail with data_plane_unconfigured)"
            );
        }
    }
    Ok(ctx)
}

#[async_trait]
impl UnifiedJobRunner for SourceRunner {
    async fn run(
        &self,
        claimed: &UnifiedClaimedJob,
        store: &SqliteUnifiedJobStore,
        shutdown: &CancellationToken,
    ) -> Result<UnifiedJobOutcome, ApiError> {
        heartbeat_running(store, claimed, PipelinePhase::Fetching).await;
        if shutdown.is_cancelled() {
            return Err(source_error("source canceled before running"));
        }

        let request_json = claimed
            .request_json
            .as_ref()
            .ok_or_else(|| source_error("source job has no request payload"))?;
        let source_request: SourceRequest = request_json
            .get("source_request")
            .cloned()
            .ok_or_else(|| source_error("source job request is missing `source_request`"))
            .and_then(|value| {
                serde_json::from_value(value)
                    .map_err(|error| source_error(format!("malformed source_request: {error}")))
            })?;

        let ctx = self.service_context(store).await?;
        let heartbeat_interval = super::job_heartbeat_interval(&ctx.cfg);
        let run_fut = run_source_request_with_cancellation(
            claimed,
            source_request,
            ctx,
            Some(shutdown.clone()),
        );
        let result = drive_source_with_heartbeat(run_fut, shutdown, heartbeat_interval, || async {
            heartbeat_running_preserving_progress(store, claimed).await;
            super::attempt_remains_active(store, claimed).await
        })
        .await?;

        match result {
            Ok(source_result) => {
                store
                    .record_job_artifacts_for_attempt(
                        claimed.job_id,
                        claimed.attempt,
                        &source_result.artifacts,
                    )
                    .await?;
                outcome_from_result(source_result)
            }
            Err(error) => Err(source_error(error.to_string())),
        }
    }
}

async fn drive_source_with_heartbeat<F, H, HF>(
    run_fut: F,
    shutdown: &CancellationToken,
    heartbeat_interval: std::time::Duration,
    mut send_heartbeat: H,
) -> Result<F::Output, ApiError>
where
    F: std::future::Future,
    H: FnMut() -> HF,
    HF: std::future::Future<Output = bool>,
{
    tokio::pin!(run_fut);
    let mut heartbeat = tokio::time::interval_at(
        tokio::time::Instant::now() + heartbeat_interval,
        heartbeat_interval,
    );
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // A heartbeat can wait for a writer held (or already queued) by the
    // source future. Keep polling both futures until the source settles;
    // awaiting the heartbeat inside a select branch deadlocks that writer.
    let result = {
        let heartbeat_loop = async {
            loop {
                heartbeat.tick().await;
                if !send_heartbeat().await {
                    break;
                }
            }
        };
        tokio::pin!(heartbeat_loop);
        tokio::select! {
            _ = shutdown.cancelled() => None,
            result = &mut run_fut => Some(result),
            _ = &mut heartbeat_loop => None,
        }
    };
    // Drop the heartbeat future before cleanup: it may itself hold a writer
    // that the source needs to settle its unpublished generation.
    match result {
        Some(result) => Ok(result),
        None => tokio::time::timeout(CANCEL_CLEANUP_GRACE, &mut run_fut)
            .await
            .map_err(|_| source_error("source canceled")),
    }
}

#[cfg(test)]
pub(crate) async fn run_source_request_with_context(
    claimed: &UnifiedClaimedJob,
    source_request: SourceRequest,
    ctx: &ServiceContext,
) -> anyhow::Result<SourceResult> {
    run_source_request_with_cancellation(claimed, source_request, ctx, None).await
}

async fn run_source_request_with_cancellation(
    claimed: &UnifiedClaimedJob,
    source_request: SourceRequest,
    ctx: &ServiceContext,
    cancellation: Option<CancellationToken>,
) -> anyhow::Result<SourceResult> {
    let auth_snapshot: Option<AuthSnapshot> = Some(claimed.auth_snapshot.clone());
    let mut execution = crate::source::SourceExecutionContext::existing_job(
        claimed.job_id,
        source_request.clone(),
        auth_snapshot,
        claimed.attempt,
    );
    if let Some(cancellation) = cancellation {
        execution = execution.with_cancellation(cancellation);
    }
    crate::source::index_source_with_execution(source_request, ctx, execution).await
}

/// Preserve the source pipeline's authoritative terminal status and counts
/// across the runner boundary so the worker can persist them atomically with
/// its terminal transition.
fn outcome_from_result(result: SourceResult) -> Result<UnifiedJobOutcome, ApiError> {
    let result_json = serde_json::to_string(&result)
        .map_err(|error| source_error(format!("source result serialization failed: {error}")))?;
    let counts = axon_api::source::StageCounts {
        items_total: Some(result.counts.items_total),
        items_done: result.counts.items_total,
        documents_total: Some(result.counts.documents_total),
        documents_done: result.counts.documents_total,
        chunks_total: Some(result.counts.chunks_total),
        chunks_done: result.counts.chunks_total,
        bytes_total: Some(result.counts.bytes_total),
        bytes_done: result.counts.bytes_total,
    };
    match result.status {
        LifecycleStatus::Completed => {
            Ok(UnifiedJobOutcome::completed(counts).with_result_json(result_json))
        }
        LifecycleStatus::CompletedDegraded => {
            Ok(UnifiedJobOutcome::completed_degraded(counts).with_result_json(result_json))
        }
        _ => {
            let detail = result
                .warnings
                .first()
                .map(|warning| warning.message.clone())
                .or_else(|| result.errors.first().map(|error| error.message.clone()))
                .unwrap_or_else(|| format!("source indexing ended in status {:?}", result.status));
            Err(source_error(detail))
        }
    }
}

fn source_error(message: impl Into<String>) -> ApiError {
    ApiError::new(
        "job_runner.source_failed",
        ErrorStage::Fetching,
        message.into(),
    )
}

#[cfg(test)]
#[path = "source_runner_tests.rs"]
mod tests;
