use crate::crates::core::config::{Config, RenderMode};
use crate::crates::core::http::validate_url;
use crate::crates::core::logging::{log_done, log_info, log_warn};
use crate::crates::crawl::engine::{CrawlSummary, run_crawl_once, should_fallback_to_chrome};
use crate::crates::jobs::common::{JobTable, mark_job_completed, spawn_heartbeat_task};
use crate::crates::jobs::status::JobStatus;
use redis::AsyncCommands;
use sqlx::PgPool;
use std::collections::HashSet;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing::Instrument as _;
use uuid::Uuid;

use super::super::{latest_completed_result_for_url, resolve_initial_mode, write_audit_diff};
use super::embed::maybe_enqueue_embed_job;
use super::job_context::{JobExecutionContext, load_job_execution_context};
use super::postprocess::{maybe_append_backfills, maybe_reconcile_stale_urls, update_latest_link};
use super::result_builder::{CompletedResultContext, build_completed_result};

const TABLE: JobTable = JobTable::Crawl;
const CRAWL_HEARTBEAT_INTERVAL_SECS: u64 = 30;

pub(super) async fn process_job(
    cfg: &Config,
    pool: &PgPool,
    id: Uuid,
) -> Result<(), Box<dyn Error>> {
    process_job_impl(cfg, pool, id).await
}

async fn process_job_impl(cfg: &Config, pool: &PgPool, id: Uuid) -> Result<(), Box<dyn Error>> {
    let Some(ctx) = load_job_execution_context(cfg, pool, id).await? else {
        return Ok(());
    };
    let _job_span = tracing::info_span!("crawl_job", job_id = %id, url = %ctx.url).entered();

    // Re-validate URL from DB before passing to engine — defense-in-depth against
    // stored injection via a compromised DB row.
    validate_url(&ctx.url)?;

    // Validate output_dir is not a path traversal attack.
    validate_output_dir(&ctx.job_cfg.output_dir, &cfg.output_dir)?;

    let job_start = std::time::Instant::now();
    log_info(&format!("crawl worker started job {} url={}", id, ctx.url));

    let (heartbeat_stop_tx, heartbeat_task) =
        spawn_heartbeat_task(pool.clone(), TABLE, id, CRAWL_HEARTBEAT_INTERVAL_SECS);

    if maybe_complete_cache_hit(pool, id, &ctx).await? {
        let _ = heartbeat_stop_tx.send(true);
        let _ = heartbeat_task.await;
        return Ok(());
    }

    // Convert Box<dyn Error> to String before the match so no !Send type
    // is held across any await inside the match arms (tokio::spawn Send bound).
    let result = run_active_crawl_job(pool, id, &ctx)
        .await
        .map_err(|e| e.to_string());

    let _ = heartbeat_stop_tx.send(true);
    let _ = heartbeat_task.await;

    match result {
        Ok(result_json) => {
            mark_job_completed(pool, TABLE, id, Some(&result_json)).await?;
            log_done(&format!(
                "worker completed crawl job {id} duration_ms={}",
                job_start.elapsed().as_millis()
            ));
        }
        Err(err) => {
            let is_canceled = err.contains(CANCEL_SENTINEL);
            let status = if is_canceled {
                JobStatus::Canceled
            } else {
                JobStatus::Failed
            };
            sqlx::query(
                "UPDATE axon_crawl_jobs SET status=$2, updated_at=NOW(), finished_at=NOW(), error_text=$3 WHERE id=$1 AND status=$4",
            )
            .bind(id)
            .bind(status.as_str())
            .bind(err.to_string())
            .bind(JobStatus::Running.as_str())
            .execute(pool)
            .await?;
            if is_canceled {
                log_info(&format!("worker canceled crawl job {id}"));
            } else {
                log_warn(&format!("worker failed crawl job {id}"));
            }
        }
    }
    Ok(())
}

/// Lexically normalize a path by collapsing `.` and `..` components without
/// hitting the filesystem. Used as a safe fallback when `canonicalize()` fails
/// (e.g., the directory does not yet exist).
fn normalize_path_lexically(p: &Path) -> PathBuf {
    use std::path::Component;
    let mut components: Vec<Component<'_>> = Vec::new();
    for c in p.components() {
        match c {
            Component::ParentDir => {
                // Only pop if the last component is a normal segment (not root)
                match components.last() {
                    Some(Component::Normal(_)) => {
                        components.pop();
                    }
                    _ => components.push(c),
                }
            }
            Component::CurDir => {}
            other => components.push(other),
        }
    }
    components.iter().collect()
}

/// Make a path absolute by prepending CWD if it is relative.
fn make_absolute(p: &Path) -> PathBuf {
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(p)
    }
}

/// Validate that `output_dir` does not escape the expected base directory.
fn validate_output_dir(output_dir: &Path, base_dir: &Path) -> Result<(), Box<dyn Error>> {
    // Prefer canonicalize() (resolves symlinks + normalizes). If the path does
    // not yet exist, fall back to making it absolute + lexical normalization so
    // that a relative path like `.cache/foo/bar` is correctly compared against
    // the canonicalized (absolute) base directory.
    let canonical = output_dir
        .canonicalize()
        .unwrap_or_else(|_| normalize_path_lexically(&make_absolute(output_dir)));
    let canonical_base = base_dir
        .canonicalize()
        .unwrap_or_else(|_| normalize_path_lexically(&make_absolute(base_dir)));
    if !canonical.starts_with(&canonical_base) {
        return Err(format!(
            "output_dir path traversal rejected: {:?} is outside {:?}",
            canonical, canonical_base
        )
        .into());
    }
    Ok(())
}

const DEFAULT_CACHE_TTL_SECS: u64 = 24 * 60 * 60;

/// Sentinel string embedded in errors produced by the cancel path.
/// Used by `save_job_result` to distinguish user cancellations from real failures.
/// Must not appear in any error message from external libraries.
const CANCEL_SENTINEL: &str = "AXON_JOB_CANCELED";

fn sorted_urls(values: &HashSet<String>) -> Vec<String> {
    let mut urls: Vec<String> = values.iter().cloned().collect();
    urls.sort();
    urls
}

async fn maybe_complete_cache_hit(
    pool: &PgPool,
    id: Uuid,
    ctx: &JobExecutionContext,
) -> Result<bool, Box<dyn Error>> {
    if !ctx.job_cfg.cache || ctx.previous_urls.is_empty() {
        return Ok(false);
    }

    // Check TTL of previous job result
    if let Some((_, previous_result_json)) =
        latest_completed_result_for_url(pool, &ctx.url, id).await?
    {
        let Some(previous_manifest_path) = previous_result_json
            .get("output_dir")
            .and_then(|v| v.as_str())
            .map(|d| PathBuf::from(d).join("manifest.jsonl"))
        else {
            // No output_dir in previous result — treat as stale, can't verify freshness
            log_info(&format!(
                "crawl job {id} previous result has no output_dir — starting fresh crawl"
            ));
            return Ok(false);
        };

        if crate::crates::crawl::manifest::manifest_cache_is_stale(
            &previous_manifest_path,
            DEFAULT_CACHE_TTL_SECS,
        )
        .await
        {
            log_info(&format!(
                "crawl job {id} cache source is stale (TTL {}s) — starting fresh crawl",
                DEFAULT_CACHE_TTL_SECS
            ));
            return Ok(false);
        }
    }

    let (report_path, diff_report) = write_audit_diff(
        &ctx.job_cfg.output_dir,
        &ctx.url,
        &ctx.previous_urls,
        &ctx.previous_urls,
        true,
        ctx.cache_source.clone(),
    )
    .await?;
    let result_json = serde_json::json!({
        "phase": "completed",
        "cache_hit": true,
        "cache_skip_browser": ctx.job_cfg.cache_skip_browser,
        "md_created": ctx.previous_urls.len(),
        "thin_md": 0,
        "filtered_urls": 0,
        "pages_crawled": 0,
        "pages_discovered": ctx.previous_urls.len(),
        "crawl_stream_pages": 0,
        "elapsed_ms": 0,
        "output_dir": ctx.job_cfg.output_dir.to_string_lossy(),
        "output_files": [report_path.to_string_lossy().to_string()],
        "audit_diff": diff_report,
        "audit_report_path": report_path.to_string_lossy(),
    });
    mark_job_completed(pool, TABLE, id, Some(&result_json)).await?;
    log_done(&format!("worker completed crawl job {id} (cache hit)"));
    Ok(true)
}

fn spawn_progress_task(
    pool: &PgPool,
    id: Uuid,
) -> (
    tokio::sync::mpsc::Sender<CrawlSummary>,
    tokio::task::JoinHandle<()>,
) {
    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel::<CrawlSummary>(256);
    let progress_pool = pool.clone();
    let progress_job_id = id;
    let progress_task = tokio::spawn(
        async move {
            let mut last_update = std::time::Instant::now();
            while let Some(progress) = progress_rx.recv().await {
                if last_update.elapsed() < Duration::from_millis(500) {
                    continue; // drain channel, skip DB write
                }
                let pages_crawled = progress.pages_seen as u64;
                let filtered_urls = pages_crawled.saturating_sub(progress.markdown_files as u64);

                let progress_json = serde_json::json!({
                    "phase": "crawling",
                    "md_created": progress.markdown_files,
                    "thin_md": progress.thin_pages,
                    "filtered_urls": filtered_urls,
                    "pages_crawled": pages_crawled,
                    "pages_discovered": progress.pages_discovered,
                    "crawl_stream_pages": progress.pages_seen,
                });
                let _ = sqlx::query(
                    "UPDATE axon_crawl_jobs SET updated_at=NOW(), result_json=$2 WHERE id=$1 AND status=$3",
                )
                .bind(progress_job_id)
                .bind(progress_json)
                .bind(JobStatus::Running.as_str())
                .execute(&progress_pool)
                .await;
                last_update = std::time::Instant::now();
            }
        }
        .instrument(tracing::Span::current()),
    );

    (progress_tx, progress_task)
}

/// Maximum number of reconnect attempts before giving up on cancel polling.
const CANCEL_POLL_MAX_RECONNECTS: u32 = 5;

/// Polls Redis until the cancel key is found, then returns.
/// Does an immediate first poll (no sleep before the first check), then polls
/// every 3 seconds. On connection failure, retries with bounded exponential
/// backoff (up to `CANCEL_POLL_MAX_RECONNECTS` attempts). After exhausting
/// retries, parks forever — tokio::select! will still complete via the crawl future.
///
/// Fail-safe: never false-cancels; if Redis is unreachable the crawl continues.
async fn poll_cancel_key(cfg: &Config, id: Uuid) {
    let key = format!("axon:crawl:cancel:{id}");
    let mut conn = match connect_cancel_redis(cfg, id).await {
        Some(c) => c,
        None => {
            std::future::pending::<()>().await;
            unreachable!("pending() never resolves");
        }
    };

    // Immediate first poll — don't wait 3s before checking.
    if poll_cancel_once(&mut conn, &key).await {
        return;
    }

    loop {
        tokio::time::sleep(Duration::from_secs(3)).await;
        let result =
            tokio::time::timeout(Duration::from_secs(3), conn.get::<_, Option<String>>(&key)).await;
        match result {
            Ok(Ok(Some(_))) => return,
            Ok(Ok(None)) => {}
            Ok(Err(e)) => {
                log_warn(&format!(
                    "crawl cancel poll: Redis GET failed for job {id}: {e}; attempting reconnect"
                ));
                match reconnect_cancel_redis(cfg, id).await {
                    Some(new_conn) => conn = new_conn,
                    None => {
                        std::future::pending::<()>().await;
                        unreachable!("pending() never resolves");
                    }
                }
            }
            Err(_) => {
                log_warn(&format!(
                    "crawl cancel poll: Redis GET timed out for job {id}; attempting reconnect"
                ));
                match reconnect_cancel_redis(cfg, id).await {
                    Some(new_conn) => conn = new_conn,
                    None => {
                        std::future::pending::<()>().await;
                        unreachable!("pending() never resolves");
                    }
                }
            }
        }
    }
}

/// Single non-blocking cancel key check. Returns `true` if cancel key is set.
async fn poll_cancel_once(conn: &mut redis::aio::MultiplexedConnection, key: &str) -> bool {
    matches!(
        tokio::time::timeout(Duration::from_secs(3), conn.get::<_, Option<String>>(key)).await,
        Ok(Ok(Some(_)))
    )
}

/// Open a Redis connection for cancel polling.
async fn connect_cancel_redis(cfg: &Config, id: Uuid) -> Option<redis::aio::MultiplexedConnection> {
    let Ok(client) = redis::Client::open(cfg.redis_url.clone()) else {
        log_warn(&format!(
            "crawl cancel poll: failed to open Redis client for job {id}; cancellation disabled"
        ));
        return None;
    };
    match tokio::time::timeout(
        Duration::from_secs(3),
        client.get_multiplexed_async_connection(),
    )
    .await
    {
        Ok(Ok(conn)) => Some(conn),
        _ => {
            log_warn(&format!(
                "crawl cancel poll: Redis connect failed for job {id}; cancellation disabled"
            ));
            None
        }
    }
}

/// Reconnect to Redis with bounded exponential backoff.
/// Returns `None` after exhausting `CANCEL_POLL_MAX_RECONNECTS` attempts.
async fn reconnect_cancel_redis(
    cfg: &Config,
    id: Uuid,
) -> Option<redis::aio::MultiplexedConnection> {
    for attempt in 0..CANCEL_POLL_MAX_RECONNECTS {
        let backoff = Duration::from_secs(1 << attempt.min(4)); // 1s, 2s, 4s, 8s, 16s
        tokio::time::sleep(backoff).await;
        if let Some(conn) = connect_cancel_redis(cfg, id).await {
            log_info(&format!(
                "crawl cancel poll: Redis reconnected for job {id} after {} attempt(s)",
                attempt + 1
            ));
            return Some(conn);
        }
    }
    log_warn(&format!(
        "crawl cancel poll: Redis reconnect failed after {CANCEL_POLL_MAX_RECONNECTS} attempts for job {id}; cancellation disabled"
    ));
    None
}

/// Save partial results for a gracefully-canceled crawl job.
/// Called when spider drains in-flight requests after a shutdown signal.
async fn save_partial_cancel_result(
    pool: &PgPool,
    id: Uuid,
    summary: &CrawlSummary,
    ctx: &JobExecutionContext,
) {
    log_info(&format!(
        "crawl job {id} shutdown complete (partial pages={})",
        summary.pages_seen
    ));
    let partial_json = serde_json::json!({
        "phase": "canceled",
        "md_created": summary.markdown_files,
        "thin_md": summary.thin_pages,
        "error_pages": summary.error_pages,
        "waf_blocked_pages": summary.waf_blocked_pages,
        "thin_urls": sorted_urls(&summary.thin_urls),
        "waf_blocked_urls": sorted_urls(&summary.waf_blocked_urls),
        "pages_crawled": summary.pages_seen,
        "elapsed_ms": summary.elapsed_ms,
        "output_dir": ctx.job_cfg.output_dir.to_string_lossy(),
        "graceful_shutdown": true,
    });
    let _ = sqlx::query(
        "UPDATE axon_crawl_jobs SET result_json=$2, updated_at=NOW() WHERE id=$1 AND status=$3",
    )
    .bind(id)
    .bind(&partial_json)
    .bind(JobStatus::Running.as_str())
    .execute(pool)
    .await;
}

async fn run_primary_with_optional_chrome_fallback(
    ctx: &JobExecutionContext,
    id: Uuid,
    progress_tx: tokio::sync::mpsc::Sender<CrawlSummary>,
    crawl_id: &str,
) -> Result<(CrawlSummary, HashSet<String>), Box<dyn Error>> {
    let initial_mode =
        resolve_initial_mode(ctx.job_cfg.render_mode, ctx.job_cfg.cache_skip_browser);
    let (http_summary, http_seen_urls) = run_crawl_once(
        &ctx.job_cfg,
        &ctx.url,
        initial_mode,
        &ctx.job_cfg.output_dir,
        Some(progress_tx.clone()),
        false, // HTTP probe: sitemap runs in the final pass only
        ctx.previous_manifest.clone(),
        Some(crawl_id),
    )
    .await?;

    if !matches!(ctx.job_cfg.render_mode, RenderMode::AutoSwitch)
        || !should_fallback_to_chrome(&http_summary, ctx.job_cfg.max_pages, &ctx.job_cfg)
    {
        return Ok((http_summary, http_seen_urls));
    }

    log_warn(&format!(
        "crawl job {id}: HTTP yielded thin results (pages={} md={}); retrying with Chrome",
        http_summary.pages_seen, http_summary.markdown_files
    ));
    match run_crawl_once(
        &ctx.job_cfg,
        &ctx.url,
        RenderMode::Chrome,
        &ctx.job_cfg.output_dir,
        Some(progress_tx),
        ctx.job_cfg.discover_sitemaps, // Chrome final pass: run sitemap if enabled
        ctx.previous_manifest.clone(),
        Some(crawl_id),
    )
    .await
    {
        Ok((chrome_summary, chrome_urls)) => {
            log_info(&format!(
                "crawl job {id}: Chrome fallback complete (pages={} md={})",
                chrome_summary.pages_seen, chrome_summary.markdown_files
            ));
            Ok((chrome_summary, chrome_urls))
        }
        Err(err) => {
            log_warn(&format!(
                "crawl job {id}: Chrome fallback failed ({err}), using HTTP result"
            ));
            Ok((http_summary, http_seen_urls))
        }
    }
}

async fn run_active_crawl_job(
    pool: &PgPool,
    id: Uuid,
    ctx: &JobExecutionContext,
) -> Result<serde_json::Value, Box<dyn Error>> {
    let manifest_path = ctx.job_cfg.output_dir.join("manifest.jsonl");
    let (progress_tx, progress_task) = spawn_progress_task(pool, id);

    // Build the spider control target: crawl_id + url. spider::utils::shutdown()
    // matches against this composite key to signal the correct crawl instance.
    let crawl_id = id.to_string();
    let control_target = format!("{crawl_id}{}", ctx.url);

    let final_prompt = ctx.extraction_prompt.clone();
    let result = async {
        // Race the crawl against the Redis cancel poller. When cancel fires,
        // signal spider's in-process control thread for an immediate graceful
        // stop — spider drains in-flight requests and returns partial results
        // instead of the future being abruptly dropped.
        let crawl_fut = run_primary_with_optional_chrome_fallback(ctx, id, progress_tx, &crawl_id);
        tokio::pin!(crawl_fut);

        let (summary, seen_urls) = tokio::select! {
            result = &mut crawl_fut => result?,
            _ = poll_cancel_key(&ctx.job_cfg, id) => {
                log_info(&format!("crawl job {id} canceled — signaling graceful shutdown"));
                spider::utils::shutdown(&control_target).await;
                // The crawl future is still alive — await it so spider finishes
                // draining in-flight requests and returns partial results.
                // Timeout: if spider doesn't stop within 30s, give up and hard-cancel.
                let drain_result = tokio::time::timeout(
                    Duration::from_secs(30),
                    crawl_fut,
                ).await;
                match drain_result {
                    Ok(Ok((summary, _seen_urls))) => {
                        // Save partial results before marking canceled — the data
                        // is already on disk and worth preserving.
                        save_partial_cancel_result(pool, id, &summary, ctx).await;
                    }
                    Ok(Err(e)) => {
                        log_warn(&format!(
                            "crawl job {id} shutdown drain failed: {e}"
                        ));
                    }
                    Err(_) => {
                        log_warn(&format!(
                            "crawl job {id} shutdown drain timed out after 30s"
                        ));
                    }
                }
                return Err(format!("crawl job {id} {CANCEL_SENTINEL}").into());
            }
        };

        let mut final_summary = summary.clone();
        let (robots_backfill_stats, robots_discovery_stats) =
            maybe_append_backfills(ctx, &seen_urls, &mut final_summary).await?;

        let stale_deleted = maybe_reconcile_stale_urls(ctx, &manifest_path)
            .await
            .unwrap_or_else(|err| {
                log_warn(&format!("crawl reconcile stale URLs failed: {err}"));
                0
            });

        maybe_enqueue_embed_job(pool, ctx, id).await?;

        update_latest_link(ctx).await;

        let mut result_json = build_completed_result(
            ctx,
            manifest_path.as_path(),
            CompletedResultContext {
                summary,
                final_summary,
                robots_backfill_stats,
                robots_discovery_stats,
                final_prompt,
            },
        )
        .await?;
        if let Some(obj) = result_json.as_object_mut() {
            obj.insert("stale_urls_deleted".to_string(), stale_deleted.into());
        }
        Ok(result_json)
    }
    .await;

    if let Err(err) = progress_task.await {
        log_warn(&format!(
            "progress_task panicked while serializing progress for crawl job {id}: {err:?}"
        ));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::poll_cancel_key;
    use crate::crates::jobs::common::{resolve_test_redis_url, test_config};
    use redis::AsyncCommands;
    use std::error::Error;
    use std::time::Duration;
    use uuid::Uuid;

    #[tokio::test]
    async fn cancel_key_set_triggers_poll_completion() -> Result<(), Box<dyn Error>> {
        let Some(redis_url) = resolve_test_redis_url() else {
            return Ok(());
        };
        let id = Uuid::new_v4();
        let key = format!("axon:crawl:cancel:{id}");

        let client = redis::Client::open(redis_url.clone())?;
        let mut conn = client.get_multiplexed_async_connection().await?;
        conn.set_ex::<_, _, ()>(&key, "1", 60).await?;

        let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
        cfg.redis_url = redis_url;

        // Immediate first-poll finds the key → future completes well under 5s.
        let result = tokio::time::timeout(Duration::from_secs(5), poll_cancel_key(&cfg, id)).await;
        assert!(
            result.is_ok(),
            "set cancel key must trigger poll completion"
        );

        let _: () = conn.del(&key).await?;
        Ok(())
    }

    #[tokio::test]
    async fn cancel_key_absent_parks_poll() -> Result<(), Box<dyn Error>> {
        let Some(redis_url) = resolve_test_redis_url() else {
            return Ok(());
        };
        let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
        cfg.redis_url = redis_url;
        let id = Uuid::new_v4();

        // No key set — after the immediate first-poll misses, the loop sleeps 3s.
        // 200ms timeout fires before the 3s sleep completes.
        let result =
            tokio::time::timeout(Duration::from_millis(200), poll_cancel_key(&cfg, id)).await;
        assert!(result.is_err(), "absent cancel key must park the poller");
        Ok(())
    }

    #[tokio::test]
    async fn cancel_key_unreachable_redis_fails_safe() -> Result<(), Box<dyn Error>> {
        // No env var needed — port 1 is always unreachable (ECONNREFUSED).
        let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
        cfg.redis_url = "redis://127.0.0.1:1".to_string();
        let id = Uuid::new_v4();

        // connect_cancel_redis returns None → poll_cancel_key calls pending() → parks forever.
        let result = tokio::time::timeout(Duration::from_secs(5), poll_cancel_key(&cfg, id)).await;
        assert!(result.is_err(), "unreachable Redis must park, never cancel");
        Ok(())
    }
}
