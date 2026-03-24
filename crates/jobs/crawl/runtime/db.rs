use crate::crates::core::config::Config;
use crate::crates::core::health::redis_healthy;
use crate::crates::core::logging::{log_info, log_warn};
#[allow(deprecated)] // open_amqp_channel used for short-lived health checks only
use crate::crates::jobs::common::open_amqp_channel;
use crate::crates::jobs::common::{
    batch_enqueue_jobs, enqueue_job, make_pool, purge_queue_safe, sort_rows_for_status_view,
};
use redis::AsyncCommands;
use std::error::Error;
use tokio::time::Duration;
use uuid::Uuid;

use super::{CrawlJob, ensure_schema, reclaim_stale_running_jobs, to_job_config};

pub async fn doctor(cfg: &Config) -> Result<serde_json::Value, Box<dyn Error>> {
    let pg_ok = match make_pool(cfg).await {
        Ok(p) => ensure_schema(&p).await.is_ok(),
        Err(_) => false,
    };

    #[allow(deprecated)] // Short-lived health check — Connection drop is acceptable here.
    let amqp_result = open_amqp_channel(cfg, &cfg.crawl_queue).await;
    let amqp_ok = amqp_result.is_ok();
    let amqp_error = amqp_result.err().map(|e| e.to_string());

    let redis_ok = redis_healthy(&cfg.redis_url).await;

    Ok(serde_json::json!({
        "postgres_ok": pg_ok,
        "amqp_ok": amqp_ok,
        "amqp_error": amqp_error,
        "redis_ok": redis_ok,
        "all_ok": pg_ok && amqp_ok && redis_ok
    }))
}

/// Check whether the pending crawl job count is at or above the configured cap.
///
/// Reads `AXON_MAX_PENDING_CRAWL_JOBS` from the environment (default 100, 0 = unlimited).
/// Returns `Err` with a human-readable message when the queue is full.
async fn check_pending_cap(pool: &sqlx::PgPool) -> Result<(), Box<dyn Error>> {
    let limit: u64 = std::env::var("AXON_MAX_PENDING_CRAWL_JOBS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100);
    if limit == 0 {
        return Ok(());
    }
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM axon_crawl_jobs WHERE status = 'pending'")
            .fetch_one(pool)
            .await?;
    if count as u64 >= limit {
        return Err(format!(
            "crawl queue full: {count} pending jobs (limit {limit}); wait for workers to drain or raise AXON_MAX_PENDING_CRAWL_JOBS"
        ).into());
    }
    Ok(())
}

pub async fn start_crawl_job(cfg: &Config, start_url: &str) -> Result<Uuid, Box<dyn Error>> {
    let pool = make_pool(cfg).await?;
    ensure_schema(&pool).await?;

    let cfg_json = serde_json::to_value(to_job_config(cfg))?;
    // Dedup by URL only — config differences are immaterial for an active crawl.
    // Avoids JSONB blob equality which prevents prepared-statement plan caching.
    if let Some(existing_id) = sqlx::query_scalar::<_, Uuid>(
        r#"
        SELECT id
        FROM axon_crawl_jobs
        WHERE status IN ('pending','running')
          AND url = $1
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(start_url)
    .fetch_optional(&pool)
    .await?
    {
        log_info(&format!(
            "crawl dedupe hit: reusing active job {} for {}",
            existing_id, start_url
        ));
        return Ok(existing_id);
    }
    check_pending_cap(&pool).await?;
    let id = Uuid::new_v4();

    sqlx::query(
        r#"
        INSERT INTO axon_crawl_jobs (id, url, status, config_json)
        VALUES ($1, $2, 'pending', $3)
        "#,
    )
    .bind(id)
    .bind(start_url)
    .bind(cfg_json)
    .execute(&pool)
    .await?;

    if let Err(err) = enqueue_job(cfg, &cfg.crawl_queue, id).await {
        log_warn(&format!(
            "amqp enqueue failed for {id}; worker fallback polling will pick it up: {err}"
        ));
    }
    Ok(id)
}

/// Insert and AMQP-enqueue multiple crawl jobs using a single Postgres pool and
/// a single AMQP connection (one TCP handshake for N publishes).
///
/// Returns a `Vec<(url, job_id)>` in the same order as `start_urls`.
/// Duplicate-active jobs reuse the existing ID without a new AMQP publish.
pub async fn start_crawl_jobs_batch(
    cfg: &Config,
    start_urls: &[&str],
) -> Result<Vec<(String, Uuid)>, Box<dyn Error>> {
    if start_urls.is_empty() {
        return Ok(Vec::new());
    }

    let pool = make_pool(cfg).await?;
    ensure_schema(&pool).await?;

    let cfg_json = serde_json::to_value(to_job_config(cfg))?;
    let mut seen = std::collections::HashSet::new();
    let mut url_strings = Vec::with_capacity(start_urls.len());
    for url in start_urls {
        if seen.insert(*url) {
            url_strings.push((*url).to_string());
        }
    }

    // 1. Find existing active jobs for all URLs in a single query.
    // Dedup by URL only — config differences are immaterial for an active crawl.
    let existing_rows = sqlx::query_as::<_, (String, Uuid)>(
        r#"
        SELECT DISTINCT ON (url) url, id
        FROM axon_crawl_jobs
        WHERE status IN ('pending','running')
          AND url = ANY($1)
        ORDER BY url, created_at DESC
        "#,
    )
    .bind(&url_strings)
    .fetch_all(&pool)
    .await?;

    let existing_map: std::collections::HashMap<String, Uuid> = existing_rows.into_iter().collect();

    // 2. Collect URLs that need new jobs (not already active).
    let new_urls: Vec<String> = url_strings
        .iter()
        .filter(|u| !existing_map.contains_key(*u))
        .cloned()
        .collect();

    // 3. Bulk INSERT new jobs using unnest, skipping any that acquired an active
    //    status between step 1 and now (race guard).
    let mut new_map: std::collections::HashMap<String, Uuid> = std::collections::HashMap::new();
    if !new_urls.is_empty() {
        check_pending_cap(&pool).await?;
        let inserted_rows = sqlx::query_as::<_, (Uuid, String)>(
            r#"
            WITH new_urls AS (
                SELECT DISTINCT u FROM unnest($1::text[]) AS u
                WHERE NOT EXISTS (
                    SELECT 1 FROM axon_crawl_jobs
                    WHERE url = u AND status IN ('pending','running')
                )
            )
            INSERT INTO axon_crawl_jobs (id, url, status, config_json, created_at, updated_at)
            SELECT gen_random_uuid(), u, 'pending', $2::jsonb, now(), now()
            FROM new_urls
            RETURNING id, url
            "#,
        )
        .bind(&new_urls)
        .bind(cfg_json)
        .fetch_all(&pool)
        .await?;

        for (id, url) in &inserted_rows {
            new_map.insert(url.clone(), *id);
        }
    }

    // Aggregate log to avoid per-URL String allocation on large batches.
    if !existing_map.is_empty() {
        log_info(&format!(
            "crawl dedupe: reusing {} active job(s) for {} url(s)",
            existing_map.len(),
            existing_map.len()
        ));
    }

    // 4. Fill race-guarded gaps by re-reading active jobs for unresolved URLs.
    //    These are already-active jobs that appeared between step 1 and step 3 —
    //    they must NOT be enqueued again (they already have an AMQP message).
    let mut race_gap_map: std::collections::HashMap<String, Uuid> =
        std::collections::HashMap::new();
    let unresolved_urls: Vec<String> = url_strings
        .iter()
        .filter(|url| !existing_map.contains_key(*url) && !new_map.contains_key(*url))
        .cloned()
        .collect();
    if !unresolved_urls.is_empty() {
        let raced_rows = sqlx::query_as::<_, (String, Uuid)>(
            r#"
            SELECT DISTINCT ON (url) url, id
            FROM axon_crawl_jobs
            WHERE status IN ('pending','running')
              AND url = ANY($1)
            ORDER BY url, created_at DESC
            "#,
        )
        .bind(&unresolved_urls)
        .fetch_all(&pool)
        .await?;
        for (url, id) in raced_rows {
            race_gap_map.insert(url, id);
        }
    }

    // 5. Build results in original input order, preserving cardinality for
    //    deduplicated inputs (all duplicates map to the same job ID).
    let mut results: Vec<(String, Uuid)> = Vec::with_capacity(start_urls.len());
    for url_str in start_urls {
        let url = (*url_str).to_string();
        if let Some(&id) = existing_map.get(&url) {
            results.push((url, id));
        } else if let Some(&id) = new_map.get(&url) {
            results.push((url, id));
        } else if let Some(&id) = race_gap_map.get(&url) {
            results.push((url, id));
        }
    }

    // 6. Enqueue only genuinely new inserts — NOT race-gap recoveries which
    //    already have active AMQP messages from a concurrent caller.
    let new_ids: Vec<Uuid> = new_map.values().copied().collect();
    if !new_ids.is_empty()
        && let Err(err) = batch_enqueue_jobs(cfg, &cfg.crawl_queue, &new_ids).await
    {
        log_warn(&format!(
            "amqp batch enqueue failed; worker fallback polling will pick up {} jobs: {err}",
            new_ids.len()
        ));
    }

    Ok(results)
}

pub async fn get_job(cfg: &Config, id: Uuid) -> Result<Option<CrawlJob>, Box<dyn Error>> {
    let pool = make_pool(cfg).await?;
    ensure_schema(&pool).await?;
    let row = sqlx::query_as::<_, CrawlJob>(
        r#"
        SELECT id, url, status, created_at, updated_at, started_at, finished_at, error_text
            , result_json
        FROM axon_crawl_jobs
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&pool)
    .await?;
    Ok(row)
}

pub async fn list_jobs(
    cfg: &Config,
    limit: i64,
    offset: i64,
) -> Result<Vec<CrawlJob>, Box<dyn Error>> {
    let pool = make_pool(cfg).await?;
    ensure_schema(&pool).await?;
    let mut rows = sqlx::query_as::<_, CrawlJob>(
        r#"
        SELECT id, url, status, created_at, updated_at, started_at, finished_at, error_text
            , result_json
        FROM axon_crawl_jobs
        ORDER BY
            CASE status
                WHEN 'running' THEN 0
                WHEN 'pending' THEN 1
                WHEN 'completed' THEN 2
                WHEN 'failed' THEN 3
                WHEN 'canceled' THEN 4
                ELSE 5
            END,
            created_at DESC,
            updated_at DESC
        LIMIT $1 OFFSET $2
        "#,
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&pool)
    .await?;
    sort_rows_for_status_view(
        &mut rows,
        |job| job.status.as_str(),
        |job| &job.created_at,
        |job| &job.updated_at,
    );
    Ok(rows)
}

#[allow(dead_code)]
pub async fn count_jobs(cfg: &Config) -> Result<i64, Box<dyn Error>> {
    let pool = make_pool(cfg).await?;
    ensure_schema(&pool).await?;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM axon_crawl_jobs")
        .fetch_one(&pool)
        .await?;
    Ok(count)
}

pub async fn cancel_job(cfg: &Config, id: Uuid) -> Result<bool, Box<dyn Error>> {
    let pool = make_pool(cfg).await?;
    ensure_schema(&pool).await?;

    let rows = sqlx::query(
        "UPDATE axon_crawl_jobs SET status='canceled', updated_at=NOW(), finished_at=NOW() WHERE id=$1 AND status IN ('pending','running')",
    )
    .bind(id)
    .execute(&pool)
    .await?
    .rows_affected();

    if rows > 0 {
        match redis::Client::open(cfg.redis_url.clone()) {
            Ok(redis_client) => match tokio::time::timeout(
                Duration::from_secs(3),
                redis_client.get_multiplexed_async_connection(),
            )
            .await
            {
                Ok(Ok(mut conn)) => {
                    let key = format!("axon:crawl:cancel:{id}");
                    if let Err(err) = conn.set_ex::<_, _, ()>(key, "1", 24 * 60 * 60).await {
                        log_warn(&format!("crawl cancel signal failed for job {id}: {err}"));
                    }
                }
                Ok(Err(err)) => {
                    log_warn(&format!(
                        "crawl cancel signal skipped for job {id}: redis connect failed: {err}"
                    ));
                }
                Err(_) => {
                    log_warn(&format!(
                        "crawl cancel signal skipped for job {id}: redis connect timeout after 3s"
                    ));
                }
            },
            Err(err) => {
                log_warn(&format!(
                    "crawl cancel signal skipped for job {id}: redis client open failed: {err}"
                ));
            }
        }
    }

    Ok(rows > 0)
}

pub async fn cleanup_jobs(cfg: &Config) -> Result<u64, Box<dyn Error>> {
    let pool = make_pool(cfg).await?;
    ensure_schema(&pool).await?;

    let deleted = sqlx::query(
        "DELETE FROM axon_crawl_jobs
            WHERE status IN ('failed','canceled')
               OR (status='pending' AND created_at < NOW() - INTERVAL '1 day')",
    )
    .execute(&pool)
    .await?
    .rows_affected();

    // Also prune completed jobs older than 30 days to prevent unbounded table growth.
    let completed_rows = sqlx::query(
        "DELETE FROM axon_crawl_jobs WHERE status = 'completed' AND (finished_at IS NULL OR finished_at < NOW() - INTERVAL '30 days')"
    )
    .execute(&pool)
    .await?
    .rows_affected();

    Ok(deleted + completed_rows)
}

pub async fn clear_jobs(cfg: &Config) -> Result<u64, Box<dyn Error>> {
    let pool = make_pool(cfg).await?;
    ensure_schema(&pool).await?;
    let rows = sqlx::query("DELETE FROM axon_crawl_jobs")
        .execute(&pool)
        .await?
        .rows_affected();

    if let Err(err) = purge_queue_safe(cfg, &cfg.crawl_queue).await {
        log_warn(&format!("crawl clear: queue purge failed: {err}"));
    }

    Ok(rows)
}

pub async fn recover_stale_crawl_jobs(cfg: &Config) -> Result<u64, Box<dyn Error>> {
    let pool = make_pool(cfg).await?;
    ensure_schema(&pool).await?;
    let stats = reclaim_stale_running_jobs(
        &pool,
        0,
        cfg.watchdog_stale_timeout_secs,
        cfg.watchdog_confirm_secs,
    )
    .await?;
    Ok(stats.reclaimed_jobs)
}

/// Re-enqueue pending crawl jobs that have been waiting longer than the stale
/// timeout with no worker picking them up — jobs orphaned by a broker restart
/// or a worker crash before the AMQP publish completed.
///
/// Called once at worker startup when AMQP is available, so the first worker
/// online immediately drains any backlog rather than waiting for manual recovery.
pub async fn reenqueue_orphaned_pending_jobs(
    cfg: &Config,
    pool: &sqlx::PgPool,
) -> Result<u64, Box<dyn Error>> {
    let threshold_secs = cfg.watchdog_stale_timeout_secs.max(60);
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM axon_crawl_jobs \
         WHERE status = 'pending' \
           AND updated_at < NOW() - make_interval(secs => $1::int) \
         ORDER BY created_at ASC",
    )
    .bind(threshold_secs as i32)
    .fetch_all(pool)
    .await?;

    if ids.is_empty() {
        return Ok(0);
    }
    batch_enqueue_jobs(cfg, &cfg.crawl_queue, &ids).await?;
    Ok(ids.len() as u64)
}
