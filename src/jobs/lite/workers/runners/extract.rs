use std::sync::Arc;

use sqlx::SqlitePool;
use tokio_util::sync::CancellationToken;

use crate::core::config::Config;
use crate::jobs::backend::{JobKind, lift_err};
use crate::jobs::lite::config_snapshot::apply_lite_config_snapshot;
use crate::jobs::lite::ops::update_result_json_for_attempt;

use super::JobResult;

pub async fn run_extract_job_lite(
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
    let attempt_id: Option<String> =
        sqlx::query_scalar("SELECT active_attempt_id FROM axon_extract_jobs WHERE id=?")
            .bind(id.to_string())
            .fetch_optional(pool)
            .await?
            .flatten();
    let effective_cfg = apply_lite_config_snapshot(cfg, &config_json).map_err(lift_err)?;

    let urls: Vec<String> = serde_json::from_str(&urls_json).map_err(
        |e| -> Box<dyn std::error::Error + Send + Sync> {
            format!("invalid urls_json in extract job {id}: {e}").into()
        },
    )?;

    let engine =
        Arc::new(crate::core::content::DeterministicExtractionEngine::with_default_parsers());
    let mut total_items = 0usize;
    for (idx, url) in urls.iter().enumerate() {
        if cancel_token
            .as_ref()
            .is_some_and(CancellationToken::is_cancelled)
        {
            return Err("extract canceled".into());
        }
        let wcfg = crate::core::content::ExtractWebConfig {
            start_url: url.clone(),
            prompt: effective_cfg.query.clone().unwrap_or_default(),
            limit: effective_cfg.max_pages,
            openai_base_url: effective_cfg.openai_base_url.clone(),
            openai_api_key: effective_cfg.openai_api_key.clone(),
            openai_model: effective_cfg.openai_model.clone(),
            llm_backend: crate::services::llm_backend::LlmBackendConfig::from_config(
                &effective_cfg,
            ),
            custom_headers: effective_cfg.custom_headers.clone(),
            render_mode: effective_cfg.render_mode,
            chrome_remote_url: effective_cfg.chrome_remote_url.clone(),
            chrome_stealth: effective_cfg.chrome_stealth,
            chrome_anti_bot: effective_cfg.chrome_anti_bot,
            chrome_intercept: effective_cfg.chrome_intercept,
            bypass_csp: effective_cfg.bypass_csp,
            accept_invalid_certs: effective_cfg.accept_invalid_certs,
            request_timeout_ms: effective_cfg.request_timeout_ms,
            fetch_retries: effective_cfg.fetch_retries,
            user_agent: effective_cfg.chrome_user_agent.clone(),
            chrome_network_idle_timeout_secs: effective_cfg.chrome_network_idle_timeout_secs,
        };
        let extract_fut = crate::core::content::run_extract_with_engine(wcfg, Arc::clone(&engine));
        let run = match cancel_token.as_ref() {
            Some(token) => tokio::select! {
                _ = token.cancelled() => return Err("extract canceled".into()),
                r = extract_fut => r.map_err(lift_err)?,
            },
            None => extract_fut.await.map_err(lift_err)?,
        };
        total_items += run.results.len();
        let progress = serde_json::json!({
            "urls": idx + 1,
            "total_items": total_items,
        });
        if let Err(e) = update_result_json_for_attempt(
            pool,
            JobKind::Extract,
            id,
            attempt_id.as_deref(),
            &progress,
        )
        .await
        {
            tracing::warn!(job_id = %id, error = %e, "failed to persist extract progress");
        }
    }

    Ok(Some(serde_json::json!({
        "urls": urls.len(),
        "total_items": total_items,
    })))
}
