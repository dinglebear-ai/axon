use sqlx::SqlitePool;

use super::super::progress::spawn_embed_progress_persister;
use crate::crates::core::config::Config;
use crate::crates::jobs::backend::lift_err;
use crate::crates::jobs::lite::config_snapshot::apply_lite_config_snapshot;

use super::JobResult;

pub async fn run_embed_job_lite(pool: &SqlitePool, cfg: &Config, id: uuid::Uuid) -> JobResult {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT input_text, config_json FROM axon_embed_jobs WHERE id=?")
            .bind(id.to_string())
            .fetch_optional(pool)
            .await?;
    let Some((input, config_json)) = row else {
        tracing::warn!(id = %id, table = "axon_embed_jobs", "job row not found at execution time, may have been deleted mid-run");
        return Ok(None);
    };

    let mut worker_cfg = apply_lite_config_snapshot(cfg, &config_json).map_err(lift_err)?;
    worker_cfg.json_output = false;
    let (progress_tx, progress_task) = spawn_embed_progress_persister(pool, id);
    let summary = crate::crates::vector::ops::embed_path_native_with_progress(
        &worker_cfg,
        &input,
        Some(progress_tx),
        None,
    )
    .await
    .map_err(lift_err)?;
    if let Err(e) = progress_task.await {
        tracing::warn!(job_id = %id, error = %e, "embed progress persister task failed");
    }

    Ok(Some(serde_json::json!({
        "input": input,
        "collection": worker_cfg.collection,
        "docs_embedded": summary.docs_embedded,
        "chunks_embedded": summary.chunks_embedded,
    })))
}
