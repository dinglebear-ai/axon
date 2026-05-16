use sqlx::SqlitePool;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::core::config::Config;
use crate::jobs::backend::{JobKind, lift_err};
use crate::jobs::lite::config_snapshot::decode_ingest_job_config;
use crate::jobs::lite::ops::update_result_json_for_attempt;

use super::JobResult;

pub async fn run_ingest_job_lite(
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

    let (source, effective_cfg) = decode_ingest_job_config(cfg, &config_json).map_err(|e| {
        let preview: String = config_json.chars().take(120).collect();
        format!("ingest job {id}: malformed config_json: {e} (preview: {preview:?})")
    })?;

    let attempt_id: Option<String> =
        sqlx::query_scalar("SELECT active_attempt_id FROM axon_ingest_jobs WHERE id=?")
            .bind(id.to_string())
            .fetch_optional(pool)
            .await?
            .flatten();
    let (progress_tx, progress_task) = spawn_ingest_progress_persister(pool, id, attempt_id);

    // The ingest service functions return `Box<dyn Error>` (not Send+Sync), so we
    // can't easily wrap them in a generic helper. Each branch races its own
    // source-specific future against the cancel token. Reddit consumes the token
    // natively (mid-loop), the others are cooperatively canceled at this
    // boundary — any in-flight HTTP request continues but its result is dropped.
    let result = match source {
        crate::jobs::ingest::IngestSource::Github {
            repo,
            include_source,
        } => {
            let (owner, repo_name) = crate::ingest::github::parse_github_repo(&repo).ok_or_else(
                || -> Box<dyn std::error::Error + Send + Sync> {
                    format!("invalid github target: {repo}").into()
                },
            )?;
            let mut github_cfg = effective_cfg.clone();
            github_cfg.github_include_source = include_source;
            let fut = crate::services::ingest::ingest_github_with_progress(
                &github_cfg,
                &owner,
                &repo_name,
                None,
                Some(progress_tx.clone()),
            );
            match cancel_token.as_ref() {
                Some(token) => tokio::select! {
                    _ = token.cancelled() => return Err("ingest canceled".into()),
                    r = fut => r.map_err(lift_err)?,
                },
                None => fut.await.map_err(lift_err)?,
            }
        }
        crate::jobs::ingest::IngestSource::Reddit { target } => {
            let options = cancel_token
                .clone()
                .map(crate::ingest::reddit::RedditIngestOptions::with_cancel_token)
                .unwrap_or_default();
            crate::services::ingest::ingest_reddit_with_progress_and_options(
                &effective_cfg,
                &target,
                None,
                Some(progress_tx.clone()),
                &options,
            )
            .await
            .map_err(lift_err)?
        }
        crate::jobs::ingest::IngestSource::Youtube { target } => {
            let fut = crate::services::ingest::ingest_youtube_with_progress(
                &effective_cfg,
                &target,
                None,
                Some(progress_tx.clone()),
            );
            match cancel_token.as_ref() {
                Some(token) => tokio::select! {
                    _ = token.cancelled() => return Err("ingest canceled".into()),
                    r = fut => r.map_err(lift_err)?,
                },
                None => fut.await.map_err(lift_err)?,
            }
        }
        crate::jobs::ingest::IngestSource::Sessions {
            sessions_claude,
            sessions_codex,
            sessions_gemini,
            sessions_project,
        } => {
            let mut sessions_cfg = effective_cfg.clone();
            sessions_cfg.sessions_claude = sessions_claude;
            sessions_cfg.sessions_codex = sessions_codex;
            sessions_cfg.sessions_gemini = sessions_gemini;
            sessions_cfg.sessions_project = sessions_project;
            let fut = crate::services::ingest::ingest_sessions_with_progress(
                &sessions_cfg,
                None,
                Some(progress_tx.clone()),
            );
            match cancel_token.as_ref() {
                Some(token) => tokio::select! {
                    _ = token.cancelled() => return Err("ingest canceled".into()),
                    r = fut => r.map_err(lift_err)?,
                },
                None => fut.await.map_err(lift_err)?,
            }
        }
    };
    drop(progress_tx);
    if let Err(e) = progress_task.await {
        tracing::warn!(job_id = %id, error = %e, "ingest progress persister task failed");
    }

    Ok(Some(result.payload))
}

fn spawn_ingest_progress_persister(
    pool: &SqlitePool,
    id: uuid::Uuid,
    attempt_id: Option<String>,
) -> (mpsc::Sender<serde_json::Value>, tokio::task::JoinHandle<()>) {
    let pool = pool.clone();
    let (tx, mut rx) = mpsc::channel::<serde_json::Value>(128);
    let task = tokio::spawn(async move {
        let mut current = serde_json::Value::Object(serde_json::Map::new());
        while let Some(progress) = rx.recv().await {
            merge_progress(&mut current, progress);
            if let Err(e) = update_result_json_for_attempt(
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

fn merge_progress(current: &mut serde_json::Value, progress: serde_json::Value) {
    if let serde_json::Value::Object(progress) = progress
        && let Some(current) = current.as_object_mut()
    {
        for (key, value) in progress {
            current.insert(key, value);
        }
        return;
    }
    *current = serde_json::Value::Object(serde_json::Map::new());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_progress_overlays_object_fields() {
        let mut current = serde_json::json!({
            "phase": "cloning",
            "files_done": 1,
            "chunks_embedded": 0,
        });

        merge_progress(
            &mut current,
            serde_json::json!({
                "phase": "embedding_batch",
                "chunks_embedded": 42,
            }),
        );

        assert_eq!(current["phase"], "embedding_batch");
        assert_eq!(current["files_done"], 1);
        assert_eq!(current["chunks_embedded"], 42);
    }
}
