use super::state::load_target_states;
use super::url_processor::{RefreshUrlContext, validate_output_dir};
use super::{RefreshJobConfig, RefreshPageResult, RefreshRunSummary, RefreshTargetState, TABLE};
use crate::crates::core::config::Config;
use crate::crates::core::content::to_markdown;
use crate::crates::core::http::{http_client, validate_url};
use crate::crates::core::logging::{log_done, log_warn};
use crate::crates::jobs::common::{mark_job_completed, mark_job_failed};
use crate::crates::jobs::status::JobStatus;
use reqwest::StatusCode;
use reqwest::header::{ETAG, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::error::Error;
use std::time::Instant;
use tokio::io::AsyncWriteExt;
use tokio::time::Duration;
use uuid::Uuid;

pub(crate) async fn refresh_one_url(
    client: &reqwest::Client,
    url: &str,
    previous: Option<&RefreshTargetState>,
) -> Result<RefreshPageResult, Box<dyn Error>> {
    validate_url(url)?;
    fetch_and_process_url(client, url, previous).await
}

/// Core fetch-and-hash logic without SSRF validation (for testability).
async fn fetch_and_process_url(
    client: &reqwest::Client,
    url: &str,
    previous: Option<&RefreshTargetState>,
) -> Result<RefreshPageResult, Box<dyn Error>> {
    let mut request = client.get(url);
    if let Some(prev) = previous {
        if let Some(etag) = prev.etag.as_deref() {
            request = request.header(IF_NONE_MATCH, etag);
        }
        if let Some(last_modified) = prev.last_modified.as_deref() {
            request = request.header(IF_MODIFIED_SINCE, last_modified);
        }
    }

    let response = request.send().await?;
    let status = response.status();
    let status_code = status.as_u16();

    if status == StatusCode::NOT_MODIFIED {
        return Ok(RefreshPageResult {
            status_code,
            etag: None,
            last_modified: None,
            content_hash: previous.and_then(|s| s.content_hash.clone()),
            markdown_chars: None,
            markdown: None,
            changed: false,
            not_modified: true,
        });
    }

    let headers = response.headers().clone();
    let etag = headers
        .get(ETAG)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_string());
    let last_modified = headers
        .get(LAST_MODIFIED)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_string());

    if !status.is_success() {
        return Ok(RefreshPageResult {
            status_code,
            etag,
            last_modified,
            content_hash: previous.and_then(|s| s.content_hash.clone()),
            markdown_chars: None,
            markdown: None,
            changed: false,
            not_modified: false,
        });
    }

    let body = response.text().await?;
    let markdown = to_markdown(&body, None);
    let trimmed = markdown.trim().to_string();
    let mut hasher = Sha256::new();
    hasher.update(trimmed.as_bytes());
    let content_hash = hex::encode(hasher.finalize());
    let markdown_chars = trimmed.chars().count();
    let changed = previous
        .and_then(|s| s.content_hash.as_deref())
        .is_none_or(|hash| hash != content_hash);

    Ok(RefreshPageResult {
        status_code,
        etag,
        last_modified,
        content_hash: Some(content_hash),
        markdown_chars: Some(markdown_chars),
        markdown: Some(trimmed),
        changed,
        not_modified: false,
    })
}

/// Set up context for a refresh job: load config and create directories.
/// Returns `None` if setup failed (job already marked failed).
async fn setup_refresh_job_context(
    pool: &PgPool,
    cfg: &Config,
    id: Uuid,
) -> Option<(
    RefreshJobConfig,
    std::path::PathBuf,
    tokio::io::BufWriter<tokio::fs::File>,
)> {
    let cfg_row = sqlx::query_scalar::<_, serde_json::Value>(
        "SELECT config_json FROM axon_refresh_jobs WHERE id=$1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await;

    let job_cfg: RefreshJobConfig = match cfg_row {
        Ok(Some(v)) => match serde_json::from_value(v) {
            Ok(c) => c,
            Err(e) => {
                if let Err(e2) =
                    mark_job_failed(pool, TABLE, id, &format!("invalid config_json: {e}")).await
                {
                    log_warn(&format!("mark_job_failed failed job_id={id} error={e2}"));
                }
                return None;
            }
        },
        Ok(None) => {
            if let Err(e) = mark_job_failed(pool, TABLE, id, "job not found in DB").await {
                log_warn(&format!("mark_job_failed failed job_id={id} error={e}"));
            }
            return None;
        }
        Err(e) => {
            if let Err(e2) = mark_job_failed(pool, TABLE, id, &format!("DB read error: {e}")).await
            {
                log_warn(&format!("mark_job_failed failed job_id={id} error={e2}"));
            }
            return None;
        }
    };

    // SEC-M-5: Validate output_dir against path traversal before creating directories.
    let output_base = &cfg.output_dir;
    let run_dir = std::path::PathBuf::from(&job_cfg.output_dir)
        .join("refresh")
        .join(id.to_string());

    if let Err(e) = validate_output_dir(&run_dir, output_base).await {
        if let Err(e2) =
            mark_job_failed(pool, TABLE, id, &format!("output_dir rejected: {e}")).await
        {
            log_warn(&format!("mark_job_failed failed job_id={id} error={e2}"));
        }
        return None;
    }

    let markdown_dir = run_dir.join("markdown");

    if let Err(err) = tokio::fs::create_dir_all(&markdown_dir).await {
        if let Err(e2) = mark_job_failed(
            pool,
            TABLE,
            id,
            &format!("create refresh output dir failed: {err}"),
        )
        .await
        {
            log_warn(&format!("mark_job_failed failed job_id={id} error={e2}"));
        }
        return None;
    }

    let manifest_path = run_dir.join("manifest.jsonl");
    let manifest_file = match tokio::fs::File::create(&manifest_path).await {
        Ok(file) => file,
        Err(err) => {
            if let Err(e2) = mark_job_failed(
                pool,
                TABLE,
                id,
                &format!("create refresh manifest failed: {err}"),
            )
            .await
            {
                log_warn(&format!("mark_job_failed failed job_id={id} error={e2}"));
            }
            return None;
        }
    };
    let manifest = tokio::io::BufWriter::new(manifest_file);

    Some((job_cfg, run_dir, manifest))
}

/// Finalize a refresh job: write final result to DB.
#[allow(clippy::too_many_arguments)]
async fn finalize_refresh_job(
    pool: &PgPool,
    id: Uuid,
    summary: &RefreshRunSummary,
    run_dir: &std::path::Path,
    manifest_path: &std::path::Path,
    total: usize,
) {
    let final_result = serde_json::json!({
        "phase": "completed",
        "checked": summary.checked,
        "changed": summary.changed,
        "unchanged": summary.unchanged,
        "not_modified": summary.not_modified,
        "failed": summary.failed,
        "embedded_chunks": summary.embedded_chunks,
        "total": total,
        "output_dir": run_dir.to_string_lossy(),
        "manifest_path": manifest_path.to_string_lossy(),
    });

    match mark_job_completed(pool, TABLE, id, Some(&final_result)).await {
        Ok(false) => {
            log_warn(&format!(
                "command=refresh_worker completion_update_skipped job_id={id} reason=not_running_state"
            ));
        }
        Ok(true) => {
            log_done(&format!("refresh complete job_id={id}"));
        }
        Err(err) => {
            if let Err(e2) =
                mark_job_failed(pool, TABLE, id, &format!("mark completed failed: {err}")).await
            {
                log_warn(&format!("mark_job_failed failed job_id={id} error={e2}"));
            }
        }
    }
}

/// Flush progress to DB if enough URLs have been checked or enough time has elapsed.
async fn maybe_flush_progress(
    pool: &PgPool,
    id: Uuid,
    summary: &RefreshRunSummary,
    total: usize,
    run_dir: &std::path::Path,
    last_flush: &mut Instant,
) {
    const FLUSH_INTERVAL_SECS: u64 = 10;
    const FLUSH_EVERY_N: usize = 25;

    if last_flush.elapsed() < Duration::from_secs(FLUSH_INTERVAL_SECS)
        && !summary.checked.is_multiple_of(FLUSH_EVERY_N)
    {
        return;
    }

    let _ = sqlx::query(&format!(
        "UPDATE axon_refresh_jobs SET updated_at=NOW(), result_json=$2 WHERE id=$1 AND status='{running}'",
        running = JobStatus::Running.as_str(),
    ))
    .bind(id)
    .bind(serde_json::json!({
        "phase": "refreshing",
        "checked": summary.checked,
        "changed": summary.changed,
        "unchanged": summary.unchanged,
        "not_modified": summary.not_modified,
        "failed": summary.failed,
        "embedded_chunks": summary.embedded_chunks,
        "total": total,
        "output_dir": run_dir.to_string_lossy(),
    }))
    .execute(pool)
    .await;

    *last_flush = Instant::now();
}

pub(crate) async fn process_refresh_job(cfg: Config, pool: PgPool, id: Uuid) {
    let Some((job_cfg, run_dir, mut manifest)) = setup_refresh_job_context(&pool, &cfg, id).await
    else {
        return;
    };

    let mut summary = RefreshRunSummary::default();
    let client = match http_client() {
        Ok(c) => c,
        Err(err) => {
            if let Err(e2) =
                mark_job_failed(&pool, TABLE, id, &format!("http client unavailable: {err}")).await
            {
                log_warn(&format!("mark_job_failed failed job_id={id} error={e2}"));
            }
            return;
        }
    };

    let states = match load_target_states(&pool, &job_cfg.urls).await {
        Ok(s) => s,
        Err(err) => {
            if let Err(e2) = mark_job_failed(
                &pool,
                TABLE,
                id,
                &format!("load refresh target state failed: {err}"),
            )
            .await
            {
                log_warn(&format!("mark_job_failed failed job_id={id} error={e2}"));
            }
            return;
        }
    };

    let markdown_dir = run_dir.join("markdown");
    let mut changed_idx: u32 = 0;
    let mut last_progress_flush = Instant::now();

    let mut ctx = RefreshUrlContext {
        cfg: &cfg,
        pool: &pool,
        client,
        markdown_dir: &markdown_dir,
        manifest: &mut manifest,
        job_id: id,
        embed: job_cfg.embed,
    };

    for url in &job_cfg.urls {
        summary.checked += 1;
        let previous = states.get(url);

        super::url_processor::process_single_refresh_url(
            &mut ctx,
            url,
            previous,
            &mut summary,
            &mut changed_idx,
        )
        .await;

        maybe_flush_progress(
            &pool,
            id,
            &summary,
            job_cfg.urls.len(),
            &run_dir,
            &mut last_progress_flush,
        )
        .await;
    }

    if let Err(e) = manifest.flush().await {
        log_warn(&format!("manifest flush failed job_id={id} error={e}"));
    }

    let manifest_path = run_dir.join("manifest.jsonl");
    finalize_refresh_job(
        &pool,
        id,
        &summary,
        &run_dir,
        &manifest_path,
        job_cfg.urls.len(),
    )
    .await;
}

#[cfg(test)]
#[path = "processor_tests.rs"]
mod tests;
