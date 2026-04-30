use std::collections::HashMap;
use std::sync::Arc;

use sqlx::SqlitePool;
use tokio::sync::Notify;

use crate::crates::core::config::Config;
use crate::crates::jobs::backend::{JobPayload, lift_err};
use crate::crates::jobs::lite::ops::enqueue_job;

pub(super) type JobResult =
    Result<Option<serde_json::Value>, Box<dyn std::error::Error + Send + Sync>>;

pub(super) async fn run_crawl_job_lite(
    pool: &SqlitePool,
    cfg: &Config,
    id: uuid::Uuid,
    embed_notify: Option<Arc<Notify>>,
) -> JobResult {
    let row: Option<(String,)> = sqlx::query_as("SELECT url FROM axon_crawl_jobs WHERE id=?")
        .bind(id.to_string())
        .fetch_optional(pool)
        .await?;
    let Some((url,)) = row else {
        tracing::warn!(id = %id, table = "axon_crawl_jobs", "job row not found at execution time, may have been deleted mid-run");
        return Ok(None);
    };

    crate::crates::core::http::validate_url(&url).map_err(lift_err)?;

    // Derive a per-job output directory to prevent concurrent crawls from clobbering each other.
    let job_output_dir = crate::crates::services::crawl::predict_crawl_output_dir(
        &cfg.output_dir,
        &url,
        &id.to_string(),
    );

    let (summary, _) = crate::crates::crawl::engine::run_crawl_once(
        cfg,
        &url,
        cfg.render_mode,
        &job_output_dir,
        None,
        cfg.discover_sitemaps,
        Arc::new(HashMap::new()),
        Some(&id.to_string()),
    )
    .await
    .map_err(lift_err)?;

    // Auto-enqueue embed job for the crawled output when embedding is enabled.
    let embed_job_id = if cfg.embed && summary.markdown_files > 0 {
        let markdown_dir = job_output_dir
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
            Ok(eid) => {
                if let Some(notify) = &embed_notify {
                    notify.notify_one();
                }
                Some(eid.to_string())
            }
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

pub(super) async fn run_embed_job_lite(
    pool: &SqlitePool,
    cfg: &Config,
    id: uuid::Uuid,
) -> JobResult {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT input_text FROM axon_embed_jobs WHERE id=?")
            .bind(id.to_string())
            .fetch_optional(pool)
            .await?;
    let Some((input,)) = row else {
        tracing::warn!(id = %id, table = "axon_embed_jobs", "job row not found at execution time, may have been deleted mid-run");
        return Ok(None);
    };

    let mut worker_cfg = cfg.clone();
    worker_cfg.json_output = false;
    let summary = crate::crates::vector::ops::embed_path_native(&worker_cfg, &input)
        .await
        .map_err(lift_err)?;

    Ok(Some(serde_json::json!({
        "input": input,
        "collection": cfg.collection,
        "docs_embedded": summary.docs_embedded,
        "chunks_embedded": summary.chunks_embedded,
    })))
}

pub(super) async fn run_extract_job_lite(
    pool: &SqlitePool,
    cfg: &Config,
    id: uuid::Uuid,
) -> JobResult {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT urls_json FROM axon_extract_jobs WHERE id=?")
            .bind(id.to_string())
            .fetch_optional(pool)
            .await?;
    let Some((urls_json,)) = row else {
        tracing::warn!(id = %id, table = "axon_extract_jobs", "job row not found at execution time, may have been deleted mid-run");
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
            .map_err(lift_err)?;
        total_items += run.results.len();
    }

    Ok(Some(serde_json::json!({
        "urls": urls.len(),
        "total_items": total_items,
    })))
}

pub(super) async fn run_ingest_job_lite(
    pool: &SqlitePool,
    cfg: &Config,
    id: uuid::Uuid,
) -> JobResult {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT config_json FROM axon_ingest_jobs WHERE id=?")
            .bind(id.to_string())
            .fetch_optional(pool)
            .await?;
    let Some((config_json,)) = row else {
        tracing::warn!(id = %id, table = "axon_ingest_jobs", "job row not found at execution time, may have been deleted mid-run");
        return Ok(None);
    };

    let source: crate::crates::jobs::ingest::IngestSource = serde_json::from_str(&config_json)
        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> {
            let preview: String = config_json.chars().take(120).collect();
            format!("ingest job {id}: malformed config_json: {e} (preview: {preview:?})").into()
        })?;

    let result = match source {
        crate::crates::jobs::ingest::IngestSource::Github {
            repo,
            include_source,
        } => {
            let (owner, repo_name) = crate::crates::ingest::github::parse_github_repo(&repo)
                .ok_or_else(|| -> Box<dyn std::error::Error + Send + Sync> {
                    format!("invalid github target: {repo}").into()
                })?;
            let mut github_cfg = cfg.clone();
            github_cfg.github_include_source = include_source;
            crate::crates::services::ingest::ingest_github(&github_cfg, &owner, &repo_name, None)
                .await
                .map_err(lift_err)?
        }
        crate::crates::jobs::ingest::IngestSource::Reddit { target } => {
            crate::crates::services::ingest::ingest_reddit(cfg, &target, None)
                .await
                .map_err(lift_err)?
        }
        crate::crates::jobs::ingest::IngestSource::Youtube { target } => {
            crate::crates::services::ingest::ingest_youtube(cfg, &target, None)
                .await
                .map_err(lift_err)?
        }
        crate::crates::jobs::ingest::IngestSource::Sessions {
            sessions_claude,
            sessions_codex,
            sessions_gemini,
            sessions_project,
        } => {
            let mut sessions_cfg = cfg.clone();
            sessions_cfg.sessions_claude = sessions_claude;
            sessions_cfg.sessions_codex = sessions_codex;
            sessions_cfg.sessions_gemini = sessions_gemini;
            sessions_cfg.sessions_project = sessions_project;
            crate::crates::services::ingest::ingest_sessions(&sessions_cfg, None)
                .await
                .map_err(lift_err)?
        }
    };

    Ok(Some(result.payload))
}
