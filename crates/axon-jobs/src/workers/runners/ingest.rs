use std::future::Future;

use sqlx::SqlitePool;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::backend::{JobKind, lift_err};
use crate::config_snapshot::decode_ingest_job_config;
use crate::ingest::IngestSource;
use crate::ingest::types::{source_type_label, target_label};
use crate::ops::update_progress_json_for_attempt;
use axon_api::job_dto::IngestResult;
use axon_core::config::Config;

use super::JobResult;

mod merge;
use merge::*;

pub async fn run_ingest_job(
    pool: &SqlitePool,
    cfg: &Config,
    id: uuid::Uuid,
    cancel_token: Option<CancellationToken>,
) -> JobResult {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT config_json FROM axon_ingest_jobs WHERE id=?")
            .bind(id.to_string())
            .fetch_optional(pool)
            .await?;
    let Some((config_json,)) = row else {
        tracing::warn!(id = %id, table = "axon_ingest_jobs", "job row not found at execution time, may have been deleted mid-run");
        return Ok(None);
    };

    let (source, mut effective_cfg) = decode_ingest_job_config(cfg, &config_json)
        .map_err(|e| format!("ingest job {id}: malformed config_json: {e}"))?;

    let attempt_id: Option<String> =
        sqlx::query_scalar("SELECT active_attempt_id FROM axon_ingest_jobs WHERE id=?")
            .bind(id.to_string())
            .fetch_optional(pool)
            .await?
            .flatten();
    let source_type = source_type_label(&source).to_string();
    let target = target_label(&source);
    // Stamp the ingest target as the origin so every chunk records `seed_url`
    // = the re-ingestable target (e.g. "owner/repo", "r/rust"). The ingest
    // services clone this cfg before embedding, carrying the marker through.
    effective_cfg.seed_url = Some(target.clone());
    let (progress_tx, progress_task) =
        spawn_ingest_progress_persister(pool, id, attempt_id, source_type.clone(), target.clone());

    let result = match source {
        IngestSource::PreparedSessions {} => {
            execute_prepared_sessions_ingest(
                pool,
                id,
                &effective_cfg,
                progress_tx.clone(),
                cancel_token.clone(),
            )
            .await?
        }
        source => {
            execute_ingest_source(
                source,
                &effective_cfg,
                progress_tx.clone(),
                cancel_token.clone(),
            )
            .await?
        }
    };
    drop(progress_tx);
    if let Err(e) = progress_task.await {
        tracing::warn!(job_id = %id, error = %e, "ingest progress persister task failed");
    }

    let progress_json: Option<String> =
        sqlx::query_scalar("SELECT progress_json FROM axon_ingest_jobs WHERE id=?")
            .bind(id.to_string())
            .fetch_optional(pool)
            .await?
            .flatten();
    let current_progress =
        current_progress_from_result_json(id, &source_type, &target, progress_json);

    Ok(Some(merge_final_payload(current_progress, result.payload)))
}

async fn execute_prepared_sessions_ingest(
    pool: &SqlitePool,
    id: uuid::Uuid,
    cfg: &Config,
    progress_tx: mpsc::Sender<serde_json::Value>,
    cancel_token: Option<CancellationToken>,
) -> Result<IngestResult, Box<dyn std::error::Error + Send + Sync>> {
    let payload_json: Option<String> = sqlx::query_scalar(
        "SELECT payload_json FROM axon_ingest_payloads \
         WHERE job_id = ? AND payload_kind = 'prepared_sessions'",
    )
    .bind(id.to_string())
    .fetch_optional(pool)
    .await?;
    let payload_json =
        payload_json.ok_or_else(|| format!("prepared sessions payload missing for job {id}"))?;
    if cancel_token
        .as_ref()
        .is_some_and(CancellationToken::is_cancelled)
    {
        return Err("ingest canceled".into());
    }
    let request: axon_ingest::sessions::IngestSessionsPreparedRequest =
        serde_json::from_str(&payload_json)?;
    let fut = axon_ingest::orchestrate::ingest_sessions_prepared_with_progress(
        cfg,
        request,
        None,
        Some(progress_tx),
    );
    let result = cancelable(fut, cancel_token.as_ref()).await?;

    if let Err(e) = sqlx::query("DELETE FROM axon_ingest_payloads WHERE job_id = ?")
        .bind(id.to_string())
        .execute(pool)
        .await
    {
        tracing::warn!(
            job_id = %id,
            error = %e,
            "prepared sessions sidecar cleanup failed after successful ingest"
        );
    }
    Ok(result)
}

async fn execute_ingest_source(
    source: IngestSource,
    cfg: &Config,
    progress_tx: mpsc::Sender<serde_json::Value>,
    cancel_token: Option<CancellationToken>,
) -> Result<IngestResult, Box<dyn std::error::Error + Send + Sync>> {
    match source {
        // Phase 12 clean break (issue #298): github/gitlab/gitea/generic_git/
        // reddit/youtube/rss provider orchestration was deleted outright from
        // axon-ingest. `classify_target` still classifies these origins (for
        // `axon refresh`), but nothing can execute them anymore.
        IngestSource::Github { repo, .. } => {
            Err(format!("github ingest is no longer supported (target: {repo})").into())
        }
        IngestSource::Gitlab { target, .. } => {
            Err(format!("gitlab ingest is no longer supported (target: {target})").into())
        }
        IngestSource::Gitea { target, .. } => {
            Err(format!("gitea ingest is no longer supported (target: {target})").into())
        }
        IngestSource::GenericGit { target, .. } => {
            Err(format!("generic git ingest is no longer supported (target: {target})").into())
        }
        IngestSource::Reddit { target } => {
            Err(format!("reddit ingest is no longer supported (target: {target})").into())
        }
        IngestSource::Youtube { target } => {
            Err(format!("youtube ingest is no longer supported (target: {target})").into())
        }
        IngestSource::Rss { target } => {
            Err(format!("rss ingest is no longer supported (target: {target})").into())
        }
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
            let fut = axon_ingest::orchestrate::ingest_sessions_with_progress(
                &sessions_cfg,
                None,
                Some(progress_tx),
            );
            cancelable(fut, cancel_token.as_ref()).await
        }
        IngestSource::PreparedSessions { .. } => {
            Err("prepared sessions must be executed through sidecar loader".into())
        }
    }
}

async fn cancelable<F>(
    fut: F,
    cancel_token: Option<&CancellationToken>,
) -> Result<IngestResult, Box<dyn std::error::Error + Send + Sync>>
where
    F: Future<Output = Result<IngestResult, Box<dyn std::error::Error>>>,
{
    match cancel_token {
        Some(token) => tokio::select! {
            _ = token.cancelled() => Err("ingest canceled".into()),
            r = fut => r.map_err(lift_err),
        },
        None => fut.await.map_err(lift_err),
    }
}

fn spawn_ingest_progress_persister(
    pool: &SqlitePool,
    id: uuid::Uuid,
    attempt_id: Option<String>,
    source_type: String,
    target: String,
) -> (mpsc::Sender<serde_json::Value>, tokio::task::JoinHandle<()>) {
    let pool = pool.clone();
    let (tx, mut rx) = mpsc::channel::<serde_json::Value>(128);
    let task = tokio::spawn(async move {
        let mut current = serde_json::Value::Object(serde_json::Map::new());
        while let Some(progress) = rx.recv().await {
            merge_progress(&mut current, progress, id, &source_type, &target);
            if let Err(e) = update_progress_json_for_attempt(
                &pool,
                JobKind::Ingest,
                id,
                attempt_id.as_deref(),
                &current,
            )
            .await
            {
                tracing::warn!(job_id = %id, error = %e, "failed to persist ingest progress");
            }
        }
    });
    (tx, task)
}

#[cfg(test)]
#[path = "ingest_tests.rs"]
mod tests;
