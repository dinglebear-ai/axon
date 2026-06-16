use std::collections::HashMap;
use std::sync::Arc;

use sqlx::SqlitePool;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use super::super::progress::spawn_crawl_progress_persister;
use crate::core::config::Config;
use crate::core::logging::log_warn;
mod result_json;

use result_json::build_crawl_result_json;

use crate::core::ui::{accent, symbol_for_status};
use crate::jobs::backend::{JobPayload, lift_err};
use crate::jobs::config_snapshot::apply_config_snapshot_for_container;
use crate::jobs::config_snapshot::{apply_config_snapshot, config_snapshot_json};
use crate::jobs::error::JobError;
use crate::jobs::ops::enqueue_job;
use crate::jobs::query::job_status_row;
use crate::jobs::status::JobStatus;

use super::JobResult;

pub async fn run_crawl_job(
    pool: &SqlitePool,
    cfg: &Config,
    id: uuid::Uuid,
    embed_notify: Option<Arc<Notify>>,
    cancel_token: Option<CancellationToken>,
) -> JobResult {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT url, config_json FROM axon_crawl_jobs WHERE id=?")
            .bind(id.to_string())
            .fetch_optional(pool)
            .await?;
    let Some((url, config_json)) = row else {
        tracing::warn!(id = %id, table = "axon_crawl_jobs", "job row not found at execution time, may have been deleted mid-run");
        return Ok(None);
    };
    let caller_cfg =
        apply_config_snapshot_for_container(cfg, &config_json, false).map_err(lift_err)?;
    let mut effective_cfg = apply_config_snapshot(cfg, &config_json).map_err(lift_err)?;
    // Stamp this crawl's start URL as the origin so the downstream embed job's
    // config snapshot carries it and every chunk records `seed_url` = start URL.
    effective_cfg.seed_url = Some(url.clone());

    validate_crawl_job_url(&url, cancel_token.as_ref()).await?;

    let job_output_dir = crate::services::crawl::predict_crawl_output_dir(
        &effective_cfg.output_dir,
        &url,
        &id.to_string(),
    );
    let caller_output_dir = crate::services::crawl::predict_crawl_output_dir(
        &caller_cfg.output_dir,
        &url,
        &id.to_string(),
    );

    let attempt_id = current_attempt_id(pool, id, "axon_crawl_jobs").await?;
    let (progress_tx, progress_task) =
        spawn_crawl_progress_persister(pool, id, attempt_id, job_output_dir.clone());
    let id_str = id.to_string();
    let crawl_fut = async {
        crate::crawl::engine::run_crawl_once(
            &effective_cfg,
            &url,
            effective_cfg.render_mode,
            &job_output_dir,
            Some(progress_tx),
            effective_cfg.discover_sitemaps,
            Arc::new(HashMap::new()),
            Some(id_str.as_str()),
        )
        .await
        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })
    };
    let (mut summary, seen_urls) = match cancel_token.as_ref() {
        Some(token) => tokio::select! {
            _ = token.cancelled() => {
                request_spider_crawl_shutdown(&id_str, &url).await;
                return Err("crawl canceled".into());
            }
            r = crawl_fut => r?,
        },
        None => crawl_fut.await?,
    };
    if let Err(e) = progress_task.await {
        tracing::warn!(job_id = %id, error = %e, "crawl progress persister task failed");
    }

    ensure_crawl_not_cancelled(pool, id, cancel_token.as_ref(), &id_str, &url).await?;

    let sitemap_backfill_error = maybe_append_sitemap_backfill(
        pool,
        &effective_cfg,
        id,
        &url,
        &id_str,
        &job_output_dir,
        &seen_urls,
        &mut summary,
        cancel_token.as_ref(),
    )
    .await?;

    let (embed_job_id, embed_deferred) = try_enqueue_embed_handoff(
        pool,
        &effective_cfg,
        &job_output_dir,
        &summary,
        embed_notify,
    )
    .await?;

    print_crawl_completion(
        &effective_cfg,
        id,
        &url,
        &job_output_dir,
        &summary,
        embed_job_id.as_deref(),
        embed_deferred.as_deref(),
    );

    Ok(Some(build_crawl_result_json(
        &url,
        &job_output_dir,
        &caller_output_dir,
        &summary,
        embed_job_id.as_deref(),
        embed_deferred.as_deref(),
        sitemap_backfill_error.as_deref(),
    )))
}

async fn current_attempt_id(
    pool: &SqlitePool,
    id: uuid::Uuid,
    table: &str,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar(&format!("SELECT active_attempt_id FROM {table} WHERE id=?"))
        .bind(id.to_string())
        .fetch_optional(pool)
        .await
        .map(Option::flatten)
}

async fn validate_crawl_job_url(
    url: &str,
    cancel_token: Option<&CancellationToken>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match cancel_token {
        Some(token) => {
            tokio::select! {
                _ = token.cancelled() => Err("crawl canceled".into()),
                result = crate::core::http::validate_url_with_dns(url) => {
                    result.map_err(lift_err)
                }
            }
        }
        None => crate::core::http::validate_url_with_dns(url)
            .await
            .map_err(lift_err),
    }
}

/// Post-crawl backfill fires if EITHER sitemap OR llms.txt discovery is enabled. This is an
/// OR gate: `discover_sitemaps=false` + `discover_llms_txt=true` must still run backfill.
fn backfill_enabled(cfg: &Config) -> bool {
    cfg.discover_sitemaps || cfg.discover_llms_txt
}

/// Sitemap discovery for the backfill merge. Gated on `discover_sitemaps`; logs a warning
/// (and returns the empty set) on discovery failure instead of swallowing the error, and
/// surfaces `failed_fetches` before dropping the diagnostics to the bare URL list.
async fn discover_sitemap_for_backfill(cfg: &Config, url: &str) -> Vec<String> {
    if !cfg.discover_sitemaps {
        return Vec::new();
    }
    match crate::crawl::engine::discover_sitemap_urls(cfg, url).await {
        Ok(discovery) => {
            if discovery.failed_fetches > 0 {
                log_warn(&format!(
                    "command=sitemap discovery failed_fetches={} discovered_urls={} url={url}",
                    discovery.failed_fetches, discovery.discovered_urls
                ));
            }
            discovery.urls
        }
        Err(e) => {
            log_warn(&format!("command=sitemap discovery failed url={url}: {e}"));
            Vec::new()
        }
    }
}

/// llms.txt discovery for the backfill merge. Gated on `discover_llms_txt`; logs a warning
/// (and returns the empty set) on discovery failure instead of swallowing the error.
async fn discover_llms_for_backfill(cfg: &Config, url: &str) -> Vec<String> {
    if !cfg.discover_llms_txt {
        return Vec::new();
    }
    match crate::crawl::engine::discover_llms_txt_urls(cfg, url).await {
        Ok(urls) => urls,
        Err(e) => {
            log_warn(&format!("command=llms_txt discovery failed url={url}: {e}"));
            Vec::new()
        }
    }
}

/// Union sitemap + llms.txt candidates, canonicalize + dedupe (sitemap wins on collision —
/// chained first). NO blanket truncation: sitemap volume is bounded upstream by
/// `max_sitemaps` and the llms fan-out is capped at its source (`max_llms_txt_urls`).
/// Capping here would shrink sitemap backfill below `main`'s — a regression.
fn merge_candidates(sitemap: Vec<String>, llms: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    sitemap
        .into_iter()
        .chain(llms)
        .filter_map(|u| crate::crawl::engine::canonicalize_url_for_dedupe(&u))
        .filter(|u| seen.insert(u.clone()))
        .collect()
}

#[expect(
    clippy::too_many_arguments,
    reason = "keeps cancellation context explicit"
)]
async fn maybe_append_sitemap_backfill(
    pool: &SqlitePool,
    effective_cfg: &Config,
    id: uuid::Uuid,
    url: &str,
    crawl_id: &str,
    job_output_dir: &std::path::Path,
    seen_urls: &std::collections::HashSet<String>,
    summary: &mut crate::crawl::engine::CrawlSummary,
    cancel_token: Option<&CancellationToken>,
) -> Result<Option<String>, Box<dyn std::error::Error + Send + Sync>> {
    if !backfill_enabled(effective_cfg) {
        return Ok(None);
    }

    let backfill_fut = async {
        // Discover both sources concurrently (each gated on its flag), union + dedupe,
        // then run a single fetch/convert/manifest pass over the merged candidate set.
        let (sitemap_res, llms_res) = tokio::join!(
            discover_sitemap_for_backfill(effective_cfg, url),
            discover_llms_for_backfill(effective_cfg, url),
        );

        let merged = merge_candidates(sitemap_res, llms_res);
        if merged.is_empty() {
            return Ok(());
        }
        let stats = crate::crawl::engine::append_candidate_backfill(
            effective_cfg,
            job_output_dir,
            seen_urls,
            merged,
            summary,
        )
        .await
        .map_err(|e| e.to_string())?;
        if stats.0.failed > 0 {
            log_warn(&format!(
                "command=backfill candidate fetch/write failures={} candidates={} written={} url={url}",
                stats.0.failed, stats.0.candidates, stats.0.written
            ));
        }
        Ok(())
    };
    let backfill_result = match cancel_token {
        Some(token) => tokio::select! {
            _ = token.cancelled() => {
                request_spider_crawl_shutdown(crawl_id, url).await;
                return Err("crawl canceled".into());
            }
            result = backfill_fut => result,
        },
        None => backfill_fut.await,
    };
    let sitemap_backfill_error = if let Err(e) = backfill_result {
        tracing::warn!(
            job_id = %id,
            url,
            error = %e,
            "crawl sitemap backfill failed after primary crawl; continuing to embed primary output"
        );
        Some(e)
    } else {
        None
    };
    ensure_crawl_not_cancelled(pool, id, cancel_token, crawl_id, url).await?;
    Ok(sitemap_backfill_error)
}

async fn ensure_crawl_not_cancelled(
    pool: &SqlitePool,
    id: uuid::Uuid,
    cancel_token: Option<&CancellationToken>,
    crawl_id: &str,
    url: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if cancel_token.is_some_and(CancellationToken::is_cancelled)
        || job_status_row(pool, crate::jobs::backend::JobKind::Crawl, id)
            .await?
            .is_some_and(|row| row.status == JobStatus::Canceled)
    {
        request_spider_crawl_shutdown(crawl_id, url).await;
        return Err("crawl canceled".into());
    }
    Ok(())
}

async fn request_spider_crawl_shutdown(crawl_id: &str, url: &str) {
    let target = format!("{crawl_id}{url}");
    tracing::info!(
        crawl_id,
        url,
        target,
        "job crawl cancel: requesting spider shutdown"
    );
    spider::utils::shutdown(&target).await;
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
}

fn print_crawl_completion(
    effective_cfg: &Config,
    id: uuid::Uuid,
    url: &str,
    job_output_dir: &std::path::Path,
    summary: &crate::crawl::engine::CrawlSummary,
    embed_job_id: Option<&str>,
    embed_deferred: Option<&str>,
) {
    if effective_cfg.json_output || effective_cfg.quiet {
        return;
    }

    eprintln!(
        "{} crawl completed {} pages={} markdown={} thin={} errors={} elapsed={} job={} output={}",
        symbol_for_status("completed"),
        accent(url),
        summary.pages_seen,
        summary.markdown_files,
        summary.thin_pages,
        summary.error_pages,
        format_elapsed_ms(summary.elapsed_ms),
        id,
        job_output_dir.join("markdown").display()
    );
    if let Some(embed_job_id) = embed_job_id {
        eprintln!("  embed queued job={embed_job_id}");
    } else if let Some(reason) = embed_deferred {
        eprintln!("  ⚠ embed DEFERRED: {reason}");
        eprintln!(
            "    markdown is on disk at {} but NOT yet indexed; query/ask will not see it.",
            job_output_dir.join("markdown").display()
        );
    } else if effective_cfg.embed {
        eprintln!("  embed skipped no markdown output");
    } else {
        eprintln!("  embed disabled");
    }
}

/// Enqueue an embed job for the freshly-crawled markdown directory. Returns
/// `(Some(embed_job_id), None)` on success, `(None, Some(reason))` when the
/// embed queue is at cap or the enqueue fails, or `(None, None)` when no
/// markdown was produced or auto-embed is disabled.
async fn try_enqueue_embed_handoff(
    pool: &SqlitePool,
    effective_cfg: &Config,
    job_output_dir: &std::path::Path,
    summary: &crate::crawl::engine::CrawlSummary,
    embed_notify: Option<Arc<Notify>>,
) -> Result<(Option<String>, Option<String>), Box<dyn std::error::Error + Send + Sync>> {
    if !effective_cfg.embed || summary.markdown_files == 0 {
        return Ok((None, None));
    }
    let markdown_dir = job_output_dir
        .join("markdown")
        .to_string_lossy()
        .to_string();
    let payload = JobPayload::Embed {
        input: markdown_dir,
        config_json: config_snapshot_json(effective_cfg).map_err(lift_err)?,
    };
    match enqueue_job(pool, &payload, effective_cfg).await {
        Ok(eid) => {
            if let Some(notify) = &embed_notify {
                notify.notify_one();
            }
            Ok((Some(eid.to_string()), None))
        }
        Err(JobError::QueueCapacityExceeded { kind, cap, current }) => {
            // Loud: capacity-bounded queues must not silently drop work. Markdown is on
            // disk, but query/ask cannot see it until the queue drains and embedding is
            // retried (out of band). Surface via tracing::error AND result_json so the
            // CLI/MCP/web layer can see this without parsing log streams.
            let msg = format!("embed queue at capacity: {current}/{cap} pending {kind} jobs");
            tracing::error!(
                queue = %kind,
                cap,
                current,
                markdown_files = summary.markdown_files,
                "crawl auto-embed deferred — {msg}; markdown on disk but unindexed"
            );
            Ok((None, Some(msg)))
        }
        Err(e) => {
            tracing::error!("job crawl worker: failed to enqueue embed job: {e}");
            Err(Box::new(e))
        }
    }
}

fn format_elapsed_ms(elapsed_ms: u128) -> String {
    if elapsed_ms >= 1_000 {
        format!("{:.1}s", elapsed_ms as f64 / 1_000.0)
    } else {
        format!("{elapsed_ms}ms")
    }
}

#[cfg(test)]
#[path = "crawl_tests.rs"]
mod tests;
