//! [`CrawlRunner`]: executes a claimed unified `Crawl` job.
//!
//! Mirrors the legacy `run_crawl_job` (`crates/axon-jobs/src/workers/runners/
//! crawl.rs`) but drops the legacy-table-specific bits (row lookup by table
//! name, `job_status_row` cancellation polling) since the unified worker
//! already supplies the claimed job's request payload and a live
//! `CancellationToken`. Sitemap/llms.txt backfill and the post-crawl embed
//! handoff are preserved.
//!
//! `claimed.request_json` carries `{"urls": [<one url>], "config_json":
//! "..."}` (see `crawl_start_with_context` in
//! `crates/axon-services/src/crawl.rs`) — one unified job per URL, matching
//! the legacy per-URL job granularity.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use axon_api::source::{
    ApiError, ErrorStage, JobCreateRequest, JobIntent, JobKind as UnifiedJobKind, JobPriority,
    JobStagePlan, MetadataMap, PipelinePhase,
};
use axon_core::config::Config;
use axon_core::logging::log_warn;
use axon_jobs::boundary::JobStore;
use axon_jobs::config_snapshot::{apply_config_snapshot, config_snapshot_json};
use axon_jobs::unified::SqliteUnifiedJobStore;
use axon_jobs::workers::UnifiedJobRunner;
use axon_jobs::workers::unified::UnifiedClaimedJob;
use tokio_util::sync::CancellationToken;

use crate::runtime::job_runners::heartbeat_running;

pub(super) struct CrawlRunner {
    pub(super) cfg: Arc<Config>,
}

#[async_trait]
impl UnifiedJobRunner for CrawlRunner {
    async fn run(
        &self,
        claimed: &UnifiedClaimedJob,
        store: &SqliteUnifiedJobStore,
        shutdown: &CancellationToken,
    ) -> Result<(), ApiError> {
        heartbeat_running(store, claimed, PipelinePhase::Fetching).await;
        if shutdown.is_cancelled() {
            return Err(crawl_error("crawl canceled before running"));
        }
        let request = claimed
            .request_json
            .as_ref()
            .ok_or_else(|| crawl_error("crawl job has no request payload"))?;
        let urls: Vec<String> = request
            .get("urls")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .ok_or_else(|| crawl_error("crawl job request is missing a `urls` array"))?;
        let url = urls
            .first()
            .ok_or_else(|| crawl_error("crawl job request `urls` array is empty"))?
            .clone();
        let config_json = request
            .get("config_json")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let mut effective_cfg = apply_config_snapshot(&self.cfg, config_json).map_err(|error| {
            ApiError::new(
                "job_runner.invalid_config_snapshot",
                ErrorStage::Planning,
                error.to_string(),
            )
        })?;
        effective_cfg.seed_url = Some(url.clone());

        let run_fut = run_crawl_and_backfill(&effective_cfg, &url, claimed.job_id.0, shutdown);
        let (summary, job_output_dir) = tokio::select! {
            _ = shutdown.cancelled() => return Err(crawl_error("crawl canceled")),
            result = run_fut => result?,
        };

        enqueue_embed_handoff(store, &effective_cfg, &job_output_dir, &summary, claimed).await?;
        Ok(())
    }
}

/// Run the crawl engine once, then sitemap/llms.txt backfill if enabled.
/// Returns the crawl summary and the job's output directory.
async fn run_crawl_and_backfill(
    effective_cfg: &Config,
    url: &str,
    job_id: uuid::Uuid,
    shutdown: &CancellationToken,
) -> Result<
    (
        axon_adapters::web_engine::engine::CrawlSummary,
        std::path::PathBuf,
    ),
    ApiError,
> {
    let job_output_dir = axon_adapters::web_engine::predict_crawl_output_dir(
        &effective_cfg.output_dir,
        url,
        &job_id.to_string(),
    );
    let id_str = job_id.to_string();

    let (mut summary, seen_urls) = axon_adapters::web_engine::engine::run_crawl_once(
        effective_cfg,
        url,
        effective_cfg.render_mode,
        &job_output_dir,
        None,
        effective_cfg.discover_sitemaps,
        Arc::new(HashMap::new()),
        Some(&id_str),
    )
    .await
    .map_err(|error| crawl_error(error.to_string()))?;

    if shutdown.is_cancelled() {
        return Err(crawl_error("crawl canceled"));
    }

    if effective_cfg.discover_sitemaps || effective_cfg.discover_llms_txt {
        run_backfill(
            effective_cfg,
            url,
            &job_output_dir,
            &seen_urls,
            &mut summary,
        )
        .await;
    }

    Ok((summary, job_output_dir))
}

/// Discover sitemap + llms.txt candidates concurrently, merge, and append any
/// missed pages to the manifest. Failures are logged and swallowed (matching
/// the legacy runner) — a backfill hiccup should not fail the whole crawl,
/// which has already produced primary results worth keeping.
async fn run_backfill(
    effective_cfg: &Config,
    url: &str,
    job_output_dir: &std::path::Path,
    seen_urls: &std::collections::HashSet<String>,
    summary: &mut axon_adapters::web_engine::engine::CrawlSummary,
) {
    let sitemap_urls = if effective_cfg.discover_sitemaps {
        match axon_adapters::web_engine::engine::discover_sitemap_urls(effective_cfg, url).await {
            Ok(discovery) => discovery.urls,
            Err(error) => {
                log_warn(&format!(
                    "command=sitemap discovery failed url={url}: {error}"
                ));
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };
    let llms_urls = if effective_cfg.discover_llms_txt {
        match axon_adapters::web_engine::engine::discover_llms_txt_urls(effective_cfg, url).await {
            Ok(urls) => urls,
            Err(error) => {
                log_warn(&format!(
                    "command=llms_txt discovery failed url={url}: {error}"
                ));
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };

    let mut seen = std::collections::HashSet::new();
    let merged: Vec<String> = sitemap_urls
        .into_iter()
        .chain(llms_urls)
        .filter_map(|candidate| {
            axon_adapters::web_engine::engine::canonicalize_url_for_dedupe(&candidate)
        })
        .filter(|candidate| seen.insert(candidate.clone()))
        .collect();
    if merged.is_empty() {
        return;
    }
    if let Err(error) = axon_adapters::web_engine::engine::append_candidate_backfill(
        effective_cfg,
        job_output_dir,
        seen_urls,
        merged,
        summary,
    )
    .await
    {
        log_warn(&format!(
            "command=backfill candidate fetch/write failed url={url}: {error}"
        ));
    }
}

/// Enqueue a follow-up `Embed` job on the unified store for the freshly
/// crawled markdown directory, mirroring `try_enqueue_embed_handoff` in the
/// legacy runner. When `embed=true`, this handoff is a required indexing
/// stage: returning crawl success while the markdown was never queued for
/// indexing makes search/research waiters report a false success.
async fn enqueue_embed_handoff(
    store: &SqliteUnifiedJobStore,
    effective_cfg: &Config,
    job_output_dir: &std::path::Path,
    summary: &axon_adapters::web_engine::engine::CrawlSummary,
    claimed: &UnifiedClaimedJob,
) -> Result<(), ApiError> {
    if !effective_cfg.embed {
        return Ok(());
    }
    if summary.markdown_files == 0 {
        return Err(crawl_error(
            "crawl produced no markdown files to embed while embed=true",
        ));
    }
    let markdown_dir = job_output_dir
        .join("markdown")
        .to_string_lossy()
        .to_string();
    let config_json = match config_snapshot_json(effective_cfg) {
        Ok(json) => json,
        Err(error) => {
            return Err(crawl_error(format!(
                "failed to snapshot config for embed handoff: {error}"
            )));
        }
    };
    let request = JobCreateRequest {
        request_id: None,
        job_kind: UnifiedJobKind::Embed,
        job_intent: JobIntent::Run,
        source_id: None,
        watch_id: None,
        parent_job_id: Some(claimed.job_id),
        root_job_id: Some(claimed.job_id),
        attempt: 1,
        priority: JobPriority::Normal,
        idempotency_key: None,
        stage_plan: vec![JobStagePlan {
            phase: PipelinePhase::Embedding,
            required: true,
            provider_requirements: Vec::new(),
            estimated_items: None,
        }],
        request: Some(serde_json::json!({
            "input": markdown_dir,
            "config_json": config_json,
        })),
        // The follow-up embed job inherits the crawl job's own auth snapshot
        // rather than defaulting to trusted_system — it runs with exactly the
        // authorization the crawl itself was granted at enqueue time.
        auth_snapshot: claimed.auth_snapshot.clone(),
        config_snapshot_id: None,
        requirements: MetadataMap::new(),
        result_schema: Some("embed_result".to_string()),
        warnings: Vec::new(),
        error: None,
        metadata: MetadataMap::new(),
        deadline_at: None,
    };
    store.create(request).await.map_err(|error| {
        crawl_error(format!(
            "failed to enqueue follow-up embed job; markdown on disk but unindexed: {}",
            error.message
        ))
    })?;
    Ok(())
}

fn crawl_error(message: impl Into<String>) -> ApiError {
    ApiError::new(
        "job_runner.crawl_failed",
        ErrorStage::Fetching,
        message.into(),
    )
}
