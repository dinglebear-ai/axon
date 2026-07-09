//! [`IngestRunner`]: executes a claimed unified `Ingest` job.
//!
//! Mirrors the legacy `run_ingest_job` (`crates/axon-jobs/src/workers/runners/
//! ingest.rs`) but drops the legacy-table-specific bits (row lookup by table
//! name, progress persistence to `axon_ingest_jobs`) since the unified worker
//! already supplies the claimed job's request payload and the unified store
//! owns progress/heartbeat persistence.
//!
//! `claimed.request_json` carries `{"source": <IngestSource>, "source_type":
//! "...", "target": "...", "config_json": "..."}` (see
//! `ingest_start_with_context` in `crates/axon-services/src/ingest.rs`).
//!
//! Only `IngestSource::Sessions` actually executes today — every other
//! variant (`Github`/`Gitlab`/`Gitea`/`GenericGit`/`Reddit`/`Youtube`/`Rss`)
//! was deleted outright from `axon-ingest` in the Phase 12 clean break
//! (issue #298) and returns a clean "no longer supported" error at execution
//! time, matching the legacy runner's `execute_ingest_source`.
//! `IngestSource::PreparedSessions` is intentionally NOT handled here — it is
//! enqueued exclusively via `ingest_sessions_prepared_start_with_context`
//! (a sidecar-payload path with zero real callers today), which remains on
//! the legacy job store; it is out of this cutover's scope.

use std::sync::Arc;

use async_trait::async_trait;
use axon_api::ingest::IngestSource;
use axon_api::source::{ApiError, ErrorStage, PipelinePhase};
use axon_core::config::Config;
use axon_jobs::config_snapshot::apply_config_snapshot;
use axon_jobs::unified::SqliteUnifiedJobStore;
use axon_jobs::workers::UnifiedJobRunner;
use axon_jobs::workers::unified::UnifiedClaimedJob;
use tokio_util::sync::CancellationToken;

use crate::runtime::job_runners::heartbeat_running;

pub(super) struct IngestRunner {
    pub(super) cfg: Arc<Config>,
}

#[async_trait]
impl UnifiedJobRunner for IngestRunner {
    async fn run(
        &self,
        claimed: &UnifiedClaimedJob,
        store: &SqliteUnifiedJobStore,
        shutdown: &CancellationToken,
    ) -> Result<(), ApiError> {
        heartbeat_running(store, claimed, PipelinePhase::Parsing).await;
        if shutdown.is_cancelled() {
            return Err(ingest_error("ingest canceled before running"));
        }
        let request = claimed
            .request_json
            .as_ref()
            .ok_or_else(|| ingest_error("ingest job has no request payload"))?;
        let source: IngestSource = request
            .get("source")
            .cloned()
            .ok_or_else(|| ingest_error("ingest job request is missing `source`"))
            .and_then(|value| {
                serde_json::from_value(value)
                    .map_err(|error| ingest_error(format!("malformed ingest source: {error}")))
            })?;
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

        let run_fut = execute_ingest_source(source, &effective_cfg);
        tokio::select! {
            _ = shutdown.cancelled() => Err(ingest_error("ingest canceled")),
            result = run_fut => result,
        }
    }
}

/// Dispatch on `IngestSource`, matching the legacy runner's
/// `execute_ingest_source`: only `Sessions` executes; every other variant is
/// a clean "no longer supported" error left over from the Phase 12
/// axon-ingest provider-orchestration removal.
async fn execute_ingest_source(source: IngestSource, cfg: &Config) -> Result<(), ApiError> {
    match source {
        IngestSource::Github { repo, .. } => Err(ingest_error(format!(
            "github ingest is no longer supported (target: {repo})"
        ))),
        IngestSource::Gitlab { target, .. } => Err(ingest_error(format!(
            "gitlab ingest is no longer supported (target: {target})"
        ))),
        IngestSource::Gitea { target, .. } => Err(ingest_error(format!(
            "gitea ingest is no longer supported (target: {target})"
        ))),
        IngestSource::GenericGit { target, .. } => Err(ingest_error(format!(
            "generic git ingest is no longer supported (target: {target})"
        ))),
        IngestSource::Reddit { target } => Err(ingest_error(format!(
            "reddit ingest is no longer supported (target: {target})"
        ))),
        IngestSource::Youtube { target } => Err(ingest_error(format!(
            "youtube ingest is no longer supported (target: {target})"
        ))),
        IngestSource::Rss { target } => Err(ingest_error(format!(
            "rss ingest is no longer supported (target: {target})"
        ))),
        IngestSource::Sessions {
            sessions_claude,
            sessions_codex,
            sessions_gemini,
            sessions_project,
        } => {
            let mut sessions_cfg = cfg.clone();
            sessions_cfg.sessions_claude = sessions_claude;
            sessions_cfg.sessions_codex = sessions_codex;
            sessions_cfg.sessions_gemini = sessions_gemini;
            sessions_cfg.sessions_project = sessions_project;
            axon_ingest::orchestrate::ingest_sessions_with_progress(&sessions_cfg, None, None)
                .await
                .map(|_result| ())
                .map_err(|error| ingest_error(error.to_string()))
        }
        IngestSource::PreparedSessions { .. } => Err(ingest_error(
            "prepared sessions must be executed through the sidecar loader, not the unified ingest runner",
        )),
    }
}

fn ingest_error(message: impl Into<String>) -> ApiError {
    ApiError::new(
        "job_runner.ingest_failed",
        ErrorStage::ParsingContent,
        message.into(),
    )
}
