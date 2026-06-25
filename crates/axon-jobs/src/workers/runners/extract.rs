use sqlx::SqlitePool;
use tokio_util::sync::CancellationToken;

use crate::backend::lift_err;
use crate::config_snapshot::apply_config_snapshot;
use axon_core::config::Config;

use super::JobResult;

pub async fn run_extract_job(
    pool: &SqlitePool,
    cfg: &Config,
    id: uuid::Uuid,
    cancel_token: Option<CancellationToken>,
) -> JobResult {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT urls_json, config_json FROM axon_extract_jobs WHERE id=?")
            .bind(id.to_string())
            .fetch_optional(pool)
            .await?;
    let Some((urls_json, config_json)) = row else {
        tracing::warn!(id = %id, table = "axon_extract_jobs", "job row not found at execution time, may have been deleted mid-run");
        return Ok(None);
    };
    let mut effective_cfg = apply_config_snapshot(cfg, &config_json).map_err(lift_err)?;
    effective_cfg.output_dir = effective_cfg
        .output_dir
        .join("extract-jobs")
        .join(id.to_string());
    effective_cfg.output_path = None;

    let urls: Vec<String> = serde_json::from_str(&urls_json).map_err(
        |e| -> Box<dyn std::error::Error + Send + Sync> {
            format!("invalid urls_json in extract job {id}: {e}").into()
        },
    )?;

    if cancel_token
        .as_ref()
        .is_some_and(CancellationToken::is_cancelled)
    {
        return Err("extract canceled".into());
    }

    let prompt = effective_cfg.query.clone().unwrap_or_default();
    let extract_fut = axon_extract::sync::extract_sync(&effective_cfg, &urls, &prompt);
    let result = match cancel_token.as_ref() {
        Some(token) => tokio::select! {
            _ = token.cancelled() => return Err("extract canceled".into()),
            r = extract_fut => r.map_err(lift_err)?,
        },
        None => extract_fut.await.map_err(lift_err)?,
    };

    let final_result = build_extract_job_result_json(&result);

    Ok(Some(final_result))
}

fn build_extract_job_result_json(
    result: &axon_api::job_dto::ExtractSyncResult,
) -> serde_json::Value {
    let mut payload = result.summary.clone();
    if let Some(object) = payload.as_object_mut() {
        object.insert(
            "summary_path".to_string(),
            serde_json::Value::String(result.summary_path.clone()),
        );
        object.insert(
            "items_path".to_string(),
            serde_json::Value::String(result.items_path.clone()),
        );
        object.insert(
            "duration_ms".to_string(),
            serde_json::json!(result.duration_ms.min(u64::MAX as u128) as u64),
        );
        object.insert(
            "artifacts".to_string(),
            serde_json::json!({
                "summary": {
                    "path": result.summary_path,
                    "format": "json",
                },
                "items": {
                    "path": result.items_path,
                    "format": "ndjson",
                }
            }),
        );
    }
    payload
}
