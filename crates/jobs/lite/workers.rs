use crate::crates::core::config::Config;
use crate::crates::jobs::backend::JobPayload;
use crate::crates::jobs::lite::cancel::CancelStore;
use crate::crates::jobs::lite::ops::{
    claim_next_pending, enqueue_job, mark_completed, mark_failed,
};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;

const POLL_INTERVAL: Duration = Duration::from_secs(5);

/// Handles to wake specific worker types when new jobs are enqueued.
pub struct WorkerHandles {
    pub crawl: Arc<Notify>,
    pub embed: Arc<Notify>,
    pub extract: Arc<Notify>,
    pub ingest: Arc<Notify>,
    pub refresh: Arc<Notify>,
    pub graph: Arc<Notify>,
}

/// Spawn in-process worker tasks for all 6 job types.
pub fn spawn_workers(
    pool: Arc<SqlitePool>,
    cfg: Arc<Config>,
    cancel_store: Arc<CancelStore>,
) -> WorkerHandles {
    let crawl_notify = Arc::new(Notify::new());
    let embed_notify = Arc::new(Notify::new());
    let extract_notify = Arc::new(Notify::new());
    let ingest_notify = Arc::new(Notify::new());
    let refresh_notify = Arc::new(Notify::new());
    let graph_notify = Arc::new(Notify::new());

    tokio::spawn(crawl_worker(
        Arc::clone(&pool),
        Arc::clone(&cfg),
        Arc::clone(&cancel_store),
        Arc::clone(&crawl_notify),
    ));
    tokio::spawn(embed_worker(
        Arc::clone(&pool),
        Arc::clone(&cfg),
        Arc::clone(&embed_notify),
    ));
    tokio::spawn(extract_worker(
        Arc::clone(&pool),
        Arc::clone(&cfg),
        Arc::clone(&extract_notify),
    ));
    tokio::spawn(ingest_worker(
        Arc::clone(&pool),
        Arc::clone(&cfg),
        Arc::clone(&ingest_notify),
    ));
    tokio::spawn(refresh_worker(
        Arc::clone(&pool),
        Arc::clone(&cfg),
        Arc::clone(&refresh_notify),
    ));
    tokio::spawn(graph_worker(
        Arc::clone(&pool),
        Arc::clone(&cfg),
        Arc::clone(&graph_notify),
    ));

    WorkerHandles {
        crawl: crawl_notify,
        embed: embed_notify,
        extract: extract_notify,
        ingest: ingest_notify,
        refresh: refresh_notify,
        graph: graph_notify,
    }
}

type JobResult = Result<Option<serde_json::Value>, Box<dyn std::error::Error + Send + Sync>>;

/// Generic worker loop: wait for Notify or poll timeout, then claim + run pending jobs.
async fn worker_loop<F, Fut>(
    pool: Arc<SqlitePool>,
    table: &'static str,
    notify: Arc<Notify>,
    run_job: F,
) where
    F: Fn(Arc<SqlitePool>, uuid::Uuid) -> Fut + Send + 'static,
    Fut: Future<Output = JobResult> + Send,
{
    loop {
        tokio::select! {
            _ = notify.notified() => {}
            _ = tokio::time::sleep(POLL_INTERVAL) => {}
        }

        loop {
            match claim_next_pending(&pool, table).await {
                Ok(Some(id)) => {
                    let result = run_job(Arc::clone(&pool), id).await;
                    match result {
                        Ok(result_json) => {
                            let _ = mark_completed(&pool, table, id, result_json.as_ref()).await;
                        }
                        Err(e) => {
                            let _ = mark_failed(&pool, table, id, &e.to_string()).await;
                        }
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    tracing::error!("worker claim error (table={}): {}", table, e);
                    break;
                }
            }
        }
    }
}

async fn crawl_worker(
    pool: Arc<SqlitePool>,
    cfg: Arc<Config>,
    _cancel_store: Arc<CancelStore>,
    notify: Arc<Notify>,
) {
    worker_loop(pool, "axon_crawl_jobs", notify, move |pool, id| {
        let cfg = Arc::clone(&cfg);
        async move { run_crawl_job_lite(&pool, &cfg, id).await }
    })
    .await;
}

async fn embed_worker(pool: Arc<SqlitePool>, cfg: Arc<Config>, notify: Arc<Notify>) {
    worker_loop(pool, "axon_embed_jobs", notify, move |pool, id| {
        let cfg = Arc::clone(&cfg);
        async move { run_embed_job_lite(&pool, &cfg, id).await }
    })
    .await;
}

async fn extract_worker(pool: Arc<SqlitePool>, cfg: Arc<Config>, notify: Arc<Notify>) {
    worker_loop(pool, "axon_extract_jobs", notify, move |pool, id| {
        let cfg = Arc::clone(&cfg);
        async move { run_extract_job_lite(&pool, &cfg, id).await }
    })
    .await;
}

async fn ingest_worker(pool: Arc<SqlitePool>, cfg: Arc<Config>, notify: Arc<Notify>) {
    worker_loop(pool, "axon_ingest_jobs", notify, move |pool, id| {
        let cfg = Arc::clone(&cfg);
        async move { run_ingest_job_lite(&pool, &cfg, id).await }
    })
    .await;
}

async fn refresh_worker(pool: Arc<SqlitePool>, cfg: Arc<Config>, notify: Arc<Notify>) {
    worker_loop(pool, "axon_refresh_jobs", notify, move |pool, id| {
        let cfg = Arc::clone(&cfg);
        async move { run_refresh_job_lite(&pool, &cfg, id).await }
    })
    .await;
}

async fn graph_worker(pool: Arc<SqlitePool>, cfg: Arc<Config>, notify: Arc<Notify>) {
    worker_loop(pool, "axon_graph_jobs", notify, move |pool, id| {
        let cfg = Arc::clone(&cfg);
        async move { run_graph_job_lite(&pool, &cfg, id).await }
    })
    .await;
}

// ── Lite job runners ──────────────────────────────────────────────────────────

async fn run_crawl_job_lite(pool: &SqlitePool, cfg: &Config, id: uuid::Uuid) -> JobResult {
    let row: Option<(String,)> = sqlx::query_as("SELECT url FROM axon_crawl_jobs WHERE id=?")
        .bind(id.to_string())
        .fetch_optional(pool)
        .await?;
    let Some((url,)) = row else {
        return Ok(None);
    };

    crate::crates::core::http::validate_url(&url)
        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })?;

    let (summary, _) = crate::crates::crawl::engine::run_crawl_once(
        cfg,
        &url,
        cfg.render_mode,
        &cfg.output_dir,
        None,
        cfg.discover_sitemaps,
        Arc::new(HashMap::new()),
        Some(&id.to_string()),
    )
    .await
    .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })?;

    // Auto-enqueue embed job for the crawled output when embedding is enabled.
    let embed_job_id = if cfg.embed && summary.markdown_files > 0 {
        let markdown_dir = cfg
            .output_dir
            .join("markdown")
            .to_string_lossy()
            .to_string();
        match enqueue_job(
            pool,
            &JobPayload::Embed {
                input: markdown_dir,
                config_json: "{}".into(),
            },
        )
        .await
        {
            Ok(eid) => Some(eid.to_string()),
            Err(e) => {
                tracing::warn!("lite crawl worker: failed to enqueue embed job: {e}");
                None
            }
        }
    } else {
        None
    };

    Ok(Some(serde_json::json!({
        "url": url,
        "pages_seen": summary.pages_seen,
        "markdown_files": summary.markdown_files,
        "elapsed_ms": summary.elapsed_ms,
        "embed_job_id": embed_job_id,
    })))
}

async fn run_embed_job_lite(pool: &SqlitePool, cfg: &Config, id: uuid::Uuid) -> JobResult {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT input_text FROM axon_embed_jobs WHERE id=?")
            .bind(id.to_string())
            .fetch_optional(pool)
            .await?;
    let Some((input,)) = row else {
        return Ok(None);
    };

    let summary = crate::crates::vector::ops::embed_path_native(cfg, &input)
        .await
        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })?;

    Ok(Some(serde_json::json!({
        "input": input,
        "collection": cfg.collection,
        "docs_embedded": summary.docs_embedded,
        "chunks_embedded": summary.chunks_embedded,
    })))
}

async fn run_extract_job_lite(pool: &SqlitePool, cfg: &Config, id: uuid::Uuid) -> JobResult {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT urls_json FROM axon_extract_jobs WHERE id=?")
            .bind(id.to_string())
            .fetch_optional(pool)
            .await?;
    let Some((urls_json,)) = row else {
        return Ok(None);
    };

    let urls: Vec<String> = serde_json::from_str(&urls_json).map_err(
        |e| -> Box<dyn std::error::Error + Send + Sync> {
            format!("invalid urls_json in extract job {id}: {e}").into()
        },
    )?;

    let engine = Arc::new(
        crate::crates::core::content::DeterministicExtractionEngine::with_default_parsers(),
    );
    let mut total_items = 0usize;
    for url in &urls {
        let wcfg = crate::crates::core::content::ExtractWebConfig {
            start_url: url.clone(),
            prompt: cfg.query.clone().unwrap_or_default(),
            limit: cfg.max_pages,
            openai_base_url: cfg.openai_base_url.clone(),
            openai_api_key: cfg.openai_api_key.clone(),
            openai_model: cfg.openai_model.clone(),
            acp_adapter_cmd: cfg.acp_adapter_cmd.clone(),
            acp_adapter_args: cfg.acp_adapter_args.clone(),
            custom_headers: cfg.custom_headers.clone(),
            render_mode: cfg.render_mode,
            chrome_remote_url: cfg.chrome_remote_url.clone(),
            chrome_stealth: cfg.chrome_stealth,
            chrome_anti_bot: cfg.chrome_anti_bot,
            chrome_intercept: cfg.chrome_intercept,
            bypass_csp: cfg.bypass_csp,
            accept_invalid_certs: cfg.accept_invalid_certs,
            request_timeout_ms: cfg.request_timeout_ms,
            fetch_retries: cfg.fetch_retries,
            user_agent: cfg.chrome_user_agent.clone(),
            chrome_network_idle_timeout_secs: cfg.chrome_network_idle_timeout_secs,
        };
        let run = crate::crates::core::content::run_extract_with_engine(wcfg, Arc::clone(&engine))
            .await
            .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })?;
        total_items += run.results.len();
    }

    Ok(Some(serde_json::json!({
        "urls": urls.len(),
        "total_items": total_items,
    })))
}

async fn run_ingest_job_lite(pool: &SqlitePool, cfg: &Config, id: uuid::Uuid) -> JobResult {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT source_type, target FROM axon_ingest_jobs WHERE id=?")
            .bind(id.to_string())
            .fetch_optional(pool)
            .await?;
    let Some((source_type, target)) = row else {
        return Ok(None);
    };

    let result = match source_type.as_str() {
        "github" => {
            let (owner, repo) = crate::crates::ingest::github::parse_github_repo(&target)
                .ok_or_else(|| format!("invalid github target: {target}"))?;
            crate::crates::services::ingest::ingest_github(cfg, &owner, &repo, None)
                .await
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })?
        }
        "reddit" => crate::crates::services::ingest::ingest_reddit(cfg, &target, None)
            .await
            .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })?,
        "youtube" => crate::crates::services::ingest::ingest_youtube(cfg, &target, None)
            .await
            .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })?,
        other => return Err(format!("unknown source_type '{other}' in ingest job {id}").into()),
    };

    Ok(Some(result.payload))
}

async fn run_refresh_job_lite(pool: &SqlitePool, cfg: &Config, id: uuid::Uuid) -> JobResult {
    let row: Option<(String,)> = sqlx::query_as("SELECT url FROM axon_refresh_jobs WHERE id=?")
        .bind(id.to_string())
        .fetch_optional(pool)
        .await?;
    let Some((url,)) = row else {
        return Ok(None);
    };

    crate::crates::core::http::validate_url(&url)
        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })?;

    let (summary, _) = crate::crates::crawl::engine::run_crawl_once(
        cfg,
        &url,
        cfg.render_mode,
        &cfg.output_dir,
        None,
        cfg.discover_sitemaps,
        Arc::new(HashMap::new()),
        None,
    )
    .await
    .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })?;

    Ok(Some(serde_json::json!({
        "url": url,
        "pages_seen": summary.pages_seen,
        "markdown_files": summary.markdown_files,
    })))
}

async fn run_graph_job_lite(pool: &SqlitePool, cfg: &Config, id: uuid::Uuid) -> JobResult {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT config_json FROM axon_graph_jobs WHERE id=?")
            .bind(id.to_string())
            .fetch_optional(pool)
            .await?;
    let Some((config_json,)) = row else {
        return Ok(None);
    };

    let job_cfg: serde_json::Value = serde_json::from_str(&config_json).unwrap_or_default();
    let url = job_cfg["url"]
        .as_str()
        .ok_or("graph job missing 'url' in config_json — enqueue via services::graph::graph_build")?
        .to_string();
    let source_type = job_cfg["source_type"]
        .as_str()
        .unwrap_or("crawl")
        .to_string();

    let neo4j = crate::crates::core::neo4j::Neo4jClient::from_config(cfg)
        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })?
        .ok_or("graph jobs require AXON_NEO4J_URL")?;

    let taxonomy =
        crate::crates::jobs::graph::taxonomy::Taxonomy::resolve(&cfg.graph_taxonomy_path)
            .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })?;

    let result = crate::crates::jobs::graph::worker::process_graph_url(
        cfg,
        &neo4j,
        &taxonomy,
        &url,
        &source_type,
    )
    .await
    .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })?;

    Ok(Some(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::jobs::backend::JobPayload;
    use crate::crates::jobs::lite::ops::enqueue_job;
    use crate::crates::jobs::lite::store::open_sqlite_pool;

    #[tokio::test]
    async fn worker_picks_up_job_via_notify() {
        let pool = Arc::new(open_sqlite_pool(":memory:").await.unwrap());
        let notify = Arc::new(Notify::new());

        let id = enqueue_job(
            &pool,
            &JobPayload::Embed {
                input: "test content".into(),
                config_json: "{}".into(),
            },
        )
        .await
        .unwrap();

        let pool2 = Arc::clone(&pool);
        let notify2 = Arc::clone(&notify);
        tokio::spawn(async move {
            if let Some(claimed_id) = claim_next_pending(&pool2, "axon_embed_jobs").await.unwrap() {
                assert_eq!(claimed_id, id);
                notify2.notify_one();
            }
        });

        notify.notify_one();
        tokio::time::sleep(Duration::from_millis(100)).await;

        let row: (String,) = sqlx::query_as("SELECT status FROM axon_embed_jobs WHERE id=?")
            .bind(id.to_string())
            .fetch_one(pool.as_ref())
            .await
            .unwrap();
        assert_ne!(row.0, "pending", "job should have been claimed");
    }
}
