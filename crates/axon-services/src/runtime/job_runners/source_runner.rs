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
//! borrow the real one. It lazily builds its own enqueue-only job runtime
//! (`crate::runtime::resolve_runtime`, no nested worker loop) plus a
//! [`TargetLocalSourceRuntime`] when `qdrant_url`/`tei_url` are configured,
//! mirroring `ServiceContext::build_target_local_source`. Built once and
//! cached (`tokio::sync::OnceCell`) so repeated `Source` jobs do not reopen a
//! pool/TEI probe per run.

use std::sync::Arc;

use async_trait::async_trait;
use axon_api::source::{
    ApiError, AuthSnapshot, ErrorStage, LifecycleStatus, PipelinePhase, SourceRequest, SourceResult,
};
use axon_core::config::Config;
use axon_jobs::unified::SqliteUnifiedJobStore;
use axon_jobs::workers::UnifiedJobRunner;
use axon_jobs::workers::unified::UnifiedClaimedJob;
use tokio::sync::OnceCell;
use tokio_util::sync::CancellationToken;

use crate::context::{ServiceContext, TargetLocalSourceRuntime};
use crate::runtime::job_runners::heartbeat_running;

pub(super) struct SourceRunner {
    cfg: Arc<Config>,
    ctx: OnceCell<ServiceContext>,
}

impl SourceRunner {
    pub(super) fn new(cfg: Arc<Config>) -> Self {
        Self {
            cfg,
            ctx: OnceCell::new(),
        }
    }

    async fn service_context(&self) -> Result<&ServiceContext, ApiError> {
        self.ctx
            .get_or_try_init(|| build_service_context(&self.cfg))
            .await
    }
}

/// Build a lightweight [`ServiceContext`] scoped to this runner: an
/// enqueue-only job runtime (no nested unified worker loop — this runner
/// already *is* the worker executing under one) plus the real
/// [`TargetLocalSourceRuntime`] when the data plane is configured. Absence of
/// `qdrant_url`/`tei_url` is not an error here — `index_source_with_auth`
/// itself degrades cleanly to a `Failed` `SourceResult` when the runtime has
/// no target local-source runtime attached.
async fn build_service_context(cfg: &Arc<Config>) -> Result<ServiceContext, ApiError> {
    let jobs = crate::runtime::resolve_runtime(Arc::clone(cfg))
        .await
        .map_err(|error| source_error(format!("failed to resolve job runtime: {error}")))?;
    let mut ctx = ServiceContext::from_runtime(Arc::clone(cfg), Arc::clone(&jobs));

    if cfg.qdrant_url.trim().is_empty() || cfg.tei_url.trim().is_empty() {
        return Ok(ctx);
    }
    let (Some(pool), Some(store)) = (jobs.sqlite_pool(), jobs.unified_job_store()) else {
        return Ok(ctx);
    };
    match TargetLocalSourceRuntime::from_config(cfg, store, (*pool).clone()).await {
        Ok(runtime) => ctx = ctx.with_target_local_source_runtime(runtime),
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
    ) -> Result<(), ApiError> {
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

        let ctx = self.service_context().await?;
        let auth_snapshot: Option<AuthSnapshot> = Some(claimed.auth_snapshot.clone());

        let run_fut = crate::source::index_source_with_auth(source_request, ctx, auth_snapshot);
        let result = tokio::select! {
            _ = shutdown.cancelled() => return Err(source_error("source canceled")),
            result = run_fut => result,
        };

        match result {
            Ok(source_result) => outcome_from_result(source_result),
            Err(error) => Err(source_error(error.to_string())),
        }
    }
}

/// Map the terminal [`SourceResult::status`] to the runner's `Result`.
/// `Completed`/`CompletedDegraded` both count as a successful unified job
/// (the unified worker's `Ok(())` path always marks `Completed` today — see
/// `crates/axon-jobs/src/workers/unified.rs::run_unified_claimed` — so a
/// finer-grained `CompletedDegraded` distinction is not yet plumbed through
/// this trait; degradation is still visible via `SourceResult.warnings` on
/// the job's own result payload). Any other terminal status is a real
/// failure and must fail the job with a descriptive `ApiError`.
fn outcome_from_result(result: SourceResult) -> Result<(), ApiError> {
    match result.status {
        LifecycleStatus::Completed | LifecycleStatus::CompletedDegraded => Ok(()),
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
