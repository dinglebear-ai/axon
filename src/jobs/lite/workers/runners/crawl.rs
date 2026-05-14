use std::collections::HashMap;
use std::sync::Arc;

use sqlx::SqlitePool;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use super::super::progress::spawn_crawl_progress_persister;
use crate::core::config::Config;
use crate::core::ui::{accent, symbol_for_status};
use crate::jobs::backend::{JobPayload, lift_err};
use crate::jobs::error::JobError;
use crate::jobs::lite::config_snapshot::{apply_lite_config_snapshot, lite_config_snapshot_json};
use crate::jobs::lite::ops::enqueue_job;
use crate::jobs::lite::query::job_status_row;
use crate::jobs::status::JobStatus;

use super::JobResult;

pub async fn run_crawl_job_lite(
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
    let effective_cfg = apply_lite_config_snapshot(cfg, &config_json).map_err(lift_err)?;

    crate::core::http::validate_url_with_dns(&url)
        .await
        .map_err(lift_err)?;

    let job_output_dir = crate::services::crawl::predict_crawl_output_dir(
        &effective_cfg.output_dir,
        &url,
        &id.to_string(),
    );

    let (progress_tx, progress_task) =
        spawn_crawl_progress_persister(pool, id, job_output_dir.clone());
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

    maybe_append_sitemap_backfill(
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
        &summary,
        embed_job_id.as_deref(),
        embed_deferred.as_deref(),
    )))
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
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if !effective_cfg.discover_sitemaps {
        return Ok(());
    }

    let backfill_fut = async {
        crate::crawl::engine::append_sitemap_backfill(
            effective_cfg,
            url,
            job_output_dir,
            seen_urls,
            summary,
        )
        .await
        .map_err(|e| e.to_string())
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
    if let Err(e) = backfill_result {
        tracing::warn!(
            job_id = %id,
            url,
            error = %e,
            "crawl sitemap backfill failed after primary crawl; continuing to embed primary output"
        );
    }
    ensure_crawl_not_cancelled(pool, id, cancel_token, crawl_id, url).await
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
        "lite crawl cancel: requesting spider shutdown"
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
        config_json: lite_config_snapshot_json(effective_cfg).map_err(lift_err)?,
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
            tracing::warn!("lite crawl worker: failed to enqueue embed job: {e}");
            Ok((None, Some(format!("enqueue error: {e}"))))
        }
    }
}

/// Builds the canonical result JSON written to `axon_crawl_jobs.result_json`.
/// Required keys are locked by `crawl_result_json_required_keys`. The optional
/// `embed_deferred` key is only present when the embed enqueue was rejected
/// (typically due to the embed queue cap) — its presence signals that markdown
/// is on disk but not yet indexed.
fn build_crawl_result_json(
    url: &str,
    job_output_dir: &std::path::Path,
    summary: &crate::crawl::engine::CrawlSummary,
    embed_job_id: Option<&str>,
    embed_deferred: Option<&str>,
) -> serde_json::Value {
    let mut value = serde_json::json!({
        "url": url,
        "output_dir": job_output_dir,
        "output_path": job_output_dir.join("markdown"),
        "pages_crawled": summary.pages_seen,
        "md_created": summary.markdown_files,
        "pages_discovered": summary.pages_discovered,
        "thin_md": summary.thin_pages,
        "error_pages": summary.error_pages,
        "waf_blocked_pages": summary.waf_blocked_pages,
        "elapsed_ms": summary.elapsed_ms,
        "embed_job_id": embed_job_id,
    });
    if let (Some(reason), Some(obj)) = (embed_deferred, value.as_object_mut()) {
        obj.insert(
            "embed_deferred".to_string(),
            serde_json::Value::String(reason.to_string()),
        );
    }
    value
}

fn format_elapsed_ms(elapsed_ms: u128) -> String {
    if elapsed_ms >= 1_000 {
        format!("{:.1}s", elapsed_ms as f64 / 1_000.0)
    } else {
        format!("{elapsed_ms}ms")
    }
}

#[cfg(test)]
mod tests {
    use super::build_crawl_result_json;
    use crate::crawl::engine::CrawlSummary;
    use std::path::Path;

    fn make_summary() -> CrawlSummary {
        CrawlSummary {
            pages_seen: 7,
            markdown_files: 5,
            pages_discovered: 9,
            thin_pages: 2,
            error_pages: 1,
            waf_blocked_pages: 0,
            elapsed_ms: 1234,
            ..CrawlSummary::default()
        }
    }

    #[test]
    fn crawl_result_json_uses_canonical_keys() {
        let json = build_crawl_result_json(
            "https://example.com",
            Path::new("/tmp/axon-crawl"),
            &make_summary(),
            Some("embed-job-id"),
            None,
        );
        let obj = json.as_object().expect("json is an object");

        // Canonical keys are present
        assert_eq!(
            obj.get("url").and_then(|v| v.as_str()),
            Some("https://example.com")
        );
        assert_eq!(obj.get("pages_crawled").and_then(|v| v.as_u64()), Some(7));
        assert_eq!(obj.get("md_created").and_then(|v| v.as_u64()), Some(5));
        assert_eq!(
            obj.get("pages_discovered").and_then(|v| v.as_u64()),
            Some(9)
        );
        assert_eq!(obj.get("thin_md").and_then(|v| v.as_u64()), Some(2));
        assert_eq!(obj.get("error_pages").and_then(|v| v.as_u64()), Some(1));
        assert_eq!(
            obj.get("waf_blocked_pages").and_then(|v| v.as_u64()),
            Some(0)
        );
        assert_eq!(obj.get("elapsed_ms").and_then(|v| v.as_u64()), Some(1234));
        assert_eq!(
            obj.get("embed_job_id").and_then(|v| v.as_str()),
            Some("embed-job-id")
        );
        assert_eq!(
            obj.get("output_dir").and_then(|v| v.as_str()),
            Some("/tmp/axon-crawl")
        );
        assert_eq!(
            obj.get("output_path").and_then(|v| v.as_str()),
            Some("/tmp/axon-crawl/markdown")
        );
    }

    #[test]
    fn crawl_result_json_omits_legacy_aliases() {
        let json = build_crawl_result_json(
            "https://example.com",
            Path::new("/tmp/axon-crawl"),
            &make_summary(),
            None,
            None,
        );
        let obj = json.as_object().expect("json is an object");

        // Legacy aliases removed (axon_rust-pkl.8)
        assert!(
            !obj.contains_key("pages_seen"),
            "legacy alias pages_seen must not appear in crawl result JSON"
        );
        assert!(
            !obj.contains_key("markdown_files"),
            "legacy alias markdown_files must not appear in crawl result JSON"
        );
    }

    #[test]
    fn crawl_result_json_required_keys() {
        let json = build_crawl_result_json(
            "https://example.com",
            Path::new("/tmp/axon-crawl"),
            &make_summary(),
            None,
            None,
        );
        let obj = json.as_object().expect("json is an object");
        for key in [
            "url",
            "output_dir",
            "output_path",
            "pages_crawled",
            "md_created",
            "pages_discovered",
            "thin_md",
            "error_pages",
            "waf_blocked_pages",
            "elapsed_ms",
            "embed_job_id",
        ] {
            assert!(obj.contains_key(key), "required key missing: {key}");
        }
        assert!(
            !obj.contains_key("embed_deferred"),
            "embed_deferred must be absent when embed was not deferred"
        );
    }

    #[test]
    fn crawl_result_json_includes_embed_deferred_when_capacity_exceeded() {
        let json = build_crawl_result_json(
            "https://example.com",
            Path::new("/tmp/axon-crawl"),
            &make_summary(),
            None,
            Some("embed queue at capacity: 50/50 pending embed jobs"),
        );
        let obj = json.as_object().expect("json is an object");
        assert_eq!(obj.get("embed_job_id").and_then(|v| v.as_str()), None);
        assert_eq!(
            obj.get("embed_deferred").and_then(|v| v.as_str()),
            Some("embed queue at capacity: 50/50 pending embed jobs"),
            "capacity-deferred embed must surface a reason in result_json"
        );
    }
}
