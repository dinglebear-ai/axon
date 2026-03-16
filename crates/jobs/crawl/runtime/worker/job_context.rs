use crate::crates::core::config::Config;
use crate::crates::crawl::manifest::{ManifestEntry, read_manifest_data, read_manifest_urls};
use redis::AsyncCommands;
use sqlx::PgPool;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

use super::super::{CrawlJobConfig, latest_completed_result_for_url};

pub(super) struct JobExecutionContext {
    pub(super) url: String,
    pub(super) job_cfg: Config,
    pub(super) extraction_prompt: Option<String>,
    pub(super) previous_urls: HashSet<String>,
    pub(super) previous_manifest: Arc<HashMap<String, ManifestEntry>>,
    pub(super) cache_source: Option<String>,
}

async fn fetch_job_row(
    pool: &PgPool,
    id: Uuid,
) -> Result<Option<(String, serde_json::Value)>, Box<dyn Error>> {
    let row = sqlx::query_as::<_, (String, serde_json::Value)>(
        "SELECT url, config_json FROM axon_crawl_jobs WHERE id=$1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

async fn maybe_cancel_job_before_start(
    redis_conn: &mut redis::aio::MultiplexedConnection,
    pool: &PgPool,
    id: Uuid,
) -> Result<bool, Box<dyn Error>> {
    let cancel_key = format!("axon:crawl:cancel:{id}");
    let cancel_before: Option<String> = redis_conn
        .get(&cancel_key)
        .await
        .map_err(|e| format!("failed to check crawl cancellation key {cancel_key}: {e}"))?;
    if cancel_before.is_none() {
        return Ok(false);
    }

    sqlx::query("UPDATE axon_crawl_jobs SET status='canceled', updated_at=NOW(), finished_at=NOW() WHERE id=$1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(true)
}

use crate::crates::core::content::url_to_domain;

fn build_job_config(cfg: &Config, parsed: &CrawlJobConfig, id: Uuid, url: &str) -> Config {
    let mut job_cfg = cfg.clone();
    job_cfg.max_pages = parsed.max_pages;
    job_cfg.max_depth = parsed.max_depth;
    job_cfg.include_subdomains = parsed.include_subdomains;
    // An empty stored list means "use defaults" — not "exclude nothing".
    // Jobs serialized before locale-prefix defaults were added would otherwise
    // silently bypass all locale filtering.
    job_cfg.exclude_path_prefix = if parsed.exclude_path_prefix.is_empty() {
        crate::crates::core::config::parse::excludes::default_exclude_prefixes()
    } else {
        parsed.exclude_path_prefix.clone()
    };
    job_cfg.respect_robots = parsed.respect_robots;
    job_cfg.min_markdown_chars = parsed.min_markdown_chars;
    job_cfg.drop_thin_markdown = parsed.drop_thin_markdown;
    job_cfg.discover_sitemaps = parsed.discover_sitemaps;
    job_cfg.sitemap_since_days = parsed.sitemap_since_days;
    job_cfg.embed = parsed.embed;
    job_cfg.render_mode = parsed.render_mode;
    job_cfg.collection = parsed.collection.clone();
    job_cfg.crawl_concurrency_limit = parsed.crawl_concurrency_limit;
    job_cfg.backfill_concurrency_limit = parsed.backfill_concurrency_limit;
    job_cfg.delay_ms = parsed.delay_ms;
    job_cfg.request_timeout_ms = parsed.request_timeout_ms;
    job_cfg.fetch_retries = parsed.fetch_retries;
    job_cfg.retry_backoff_ms = parsed.retry_backoff_ms;
    job_cfg.shared_queue = parsed.shared_queue;
    job_cfg.query = parsed.extraction_prompt.clone();
    job_cfg.cache = parsed.cache;
    job_cfg.cache_skip_browser = parsed.cache_skip_browser;

    // Always use the worker's own base output dir (cfg.output_dir), not the
    // stored job path.  The job config was serialized by the submitter (e.g.
    // the MCP server on the host), which may have a different filesystem root
    // than the worker (e.g. running inside Docker at /app).
    let domain = url_to_domain(url);
    job_cfg.output_dir = cfg
        .output_dir
        .join("domains")
        .join(domain)
        .join(id.to_string());
    job_cfg
}

async fn load_previous_urls_for_cache(
    pool: &PgPool,
    id: Uuid,
    url: &str,
    job_cfg: &Config,
) -> Result<
    (
        HashSet<String>,
        Arc<HashMap<String, ManifestEntry>>,
        Option<String>,
    ),
    Box<dyn Error>,
> {
    let mut previous_urls = HashSet::new();
    let mut previous_manifest = Arc::new(HashMap::new());
    let mut cache_source: Option<String> = None;

    if !job_cfg.cache {
        return Ok((previous_urls, previous_manifest, cache_source));
    }

    if let Some((previous_job_id, previous_result_json)) =
        latest_completed_result_for_url(pool, url, id).await?
    {
        let previous_output_dir = previous_result_json
            .get("output_dir")
            .and_then(|value| value.as_str())
            .map(PathBuf::from);
        if let Some(previous_output_dir) = previous_output_dir {
            let previous_manifest_path = previous_output_dir.join("manifest.jsonl");
            previous_manifest = Arc::new(read_manifest_data(&previous_manifest_path).await?);
            // Resolve relative paths against the previous output directory so downstream
            // consumers (collector.rs hardlink/reflink) don't resolve against CWD.
            if let Some(manifest) = Arc::get_mut(&mut previous_manifest) {
                for entry in manifest.values_mut() {
                    let p = std::path::Path::new(&entry.relative_path);
                    if p.is_relative() {
                        entry.relative_path =
                            previous_output_dir.join(p).to_string_lossy().into_owned();
                    }
                }
            }
            // Also read URLs from legacy manifest entries that may lack full ManifestEntry
            // fields (backward compat: older manifests may have different JSON shapes).
            let legacy_urls = read_manifest_urls(&previous_manifest_path).await?;
            previous_urls = previous_manifest.keys().cloned().collect();
            previous_urls.extend(legacy_urls);
            if !previous_urls.is_empty() {
                cache_source = Some(format!(
                    "job:{} manifest:{}",
                    previous_job_id,
                    previous_manifest_path.to_string_lossy()
                ));
            }
        }
    }

    Ok((previous_urls, previous_manifest, cache_source))
}

pub(super) async fn load_job_execution_context(
    cfg: &Config,
    pool: &PgPool,
    id: Uuid,
) -> Result<Option<JobExecutionContext>, Box<dyn Error>> {
    let row = fetch_job_row(pool, id).await?;
    let Some((url, cfg_json)) = row else {
        return Ok(None);
    };

    let redis_client = redis::Client::open(cfg.redis_url.clone())?;
    let mut redis_conn = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        redis_client.get_multiplexed_async_connection(),
    )
    .await
    .map_err(|_| "redis connect timeout (3s) in job context")?
    .map_err(|e| format!("redis connect failed in job context: {e}"))?;

    if maybe_cancel_job_before_start(&mut redis_conn, pool, id).await? {
        return Ok(None);
    }

    let parsed: CrawlJobConfig = serde_json::from_value(cfg_json)?;
    let extraction_prompt = parsed.extraction_prompt.clone();
    let job_cfg = build_job_config(cfg, &parsed, id, &url);
    let (previous_urls, previous_manifest, cache_source) =
        load_previous_urls_for_cache(pool, id, &url, &job_cfg).await?;

    Ok(Some(JobExecutionContext {
        url,
        job_cfg,
        extraction_prompt,
        previous_urls,
        previous_manifest,
        cache_source,
    }))
}
