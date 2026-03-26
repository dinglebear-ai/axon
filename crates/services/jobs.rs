use std::error::Error;

use crate::crates::core::config::Config;
use crate::crates::jobs::backend::JobKind;
use crate::crates::jobs::graph::{self as graph_jobs, GraphJob};
use crate::crates::jobs::lite::ops::cancel_row;
use crate::crates::jobs::lite::query as lite_query;
use crate::crates::jobs::lite::store::{open_config_pool, reclaim_stale_running_jobs_for_table};
use crate::crates::services::types::ServiceJob;
use uuid::Uuid;

pub enum WorkerMode {
    Started,
    InProcess,
    Unsupported(&'static str),
}

fn graph_to_service_job(job: GraphJob) -> ServiceJob {
    ServiceJob {
        id: job.id,
        status: job.status,
        created_at: job.created_at,
        updated_at: job.updated_at,
        started_at: job.started_at,
        finished_at: job.finished_at,
        error_text: job.error_text,
        url: Some(job.url),
        source_type: None,
        target: None,
        urls_json: None,
        result_json: Some(serde_json::json!({
            "chunk_count": job.chunk_count,
            "entity_count": job.entity_count,
            "relation_count": job.relation_count,
        })),
        config_json: None,
    }
}

fn crawl_to_service_job(job: crate::crates::jobs::crawl::CrawlJob) -> ServiceJob {
    ServiceJob {
        id: job.id,
        status: job.status,
        created_at: job.created_at,
        updated_at: job.updated_at,
        started_at: job.started_at,
        finished_at: job.finished_at,
        error_text: job.error_text,
        url: Some(job.url),
        source_type: None,
        target: None,
        urls_json: None,
        result_json: job.result_json,
        config_json: None,
    }
}

fn embed_to_service_job(job: crate::crates::jobs::embed::EmbedJob) -> ServiceJob {
    ServiceJob {
        id: job.id,
        status: job.status,
        created_at: job.created_at,
        updated_at: job.updated_at,
        started_at: job.started_at,
        finished_at: job.finished_at,
        error_text: job.error_text,
        url: None,
        source_type: None,
        target: Some(job.input_text),
        urls_json: None,
        result_json: job.result_json,
        config_json: Some(job.config_json),
    }
}

fn extract_to_service_job(job: crate::crates::jobs::extract::ExtractJob) -> ServiceJob {
    ServiceJob {
        id: job.id,
        status: job.status,
        created_at: job.created_at,
        updated_at: job.updated_at,
        started_at: job.started_at,
        finished_at: job.finished_at,
        error_text: job.error_text,
        url: None,
        source_type: None,
        target: None,
        urls_json: Some(job.urls_json),
        result_json: job.result_json,
        config_json: None,
    }
}

fn ingest_to_service_job(job: crate::crates::jobs::ingest::IngestJob) -> ServiceJob {
    ServiceJob {
        id: job.id,
        status: job.status,
        created_at: job.created_at,
        updated_at: job.updated_at,
        started_at: job.started_at,
        finished_at: job.finished_at,
        error_text: job.error_text,
        url: None,
        source_type: Some(job.source_type),
        target: Some(job.target),
        urls_json: None,
        result_json: job.result_json,
        config_json: Some(job.config_json),
    }
}

fn refresh_to_service_job(job: crate::crates::jobs::refresh::RefreshJob) -> ServiceJob {
    ServiceJob {
        id: job.id,
        status: job.status,
        created_at: job.created_at,
        updated_at: job.updated_at,
        started_at: job.started_at,
        finished_at: job.finished_at,
        error_text: job.error_text,
        url: None,
        source_type: None,
        target: None,
        urls_json: Some(job.urls_json),
        result_json: job.result_json,
        config_json: Some(job.config_json),
    }
}

pub async fn list_jobs(
    cfg: &Config,
    kind: JobKind,
    limit: i64,
    offset: i64,
) -> Result<Vec<ServiceJob>, Box<dyn Error>> {
    if cfg.lite_mode {
        let pool = open_config_pool(cfg).await?;
        return Ok(lite_query::list_service_jobs(&pool, kind).await?);
    }

    let jobs = match kind {
        JobKind::Crawl => crate::crates::jobs::crawl::list_jobs(cfg, limit, offset)
            .await?
            .into_iter()
            .map(crawl_to_service_job)
            .collect(),
        JobKind::Embed => crate::crates::jobs::embed::list_embed_jobs(cfg, limit, offset)
            .await?
            .into_iter()
            .map(embed_to_service_job)
            .collect(),
        JobKind::Extract => crate::crates::jobs::extract::list_extract_jobs(cfg, limit, offset)
            .await?
            .into_iter()
            .map(extract_to_service_job)
            .collect(),
        JobKind::Ingest => crate::crates::jobs::ingest::list_ingest_jobs(cfg, None, limit, offset)
            .await?
            .into_iter()
            .map(ingest_to_service_job)
            .collect(),
        JobKind::Refresh => crate::crates::jobs::refresh::list_refresh_jobs(cfg, limit, offset)
            .await?
            .into_iter()
            .map(refresh_to_service_job)
            .collect(),
        JobKind::Graph => graph_jobs::list_graph_jobs(cfg, limit, offset)
            .await?
            .into_iter()
            .map(graph_to_service_job)
            .collect(),
    };
    Ok(jobs)
}

pub async fn job_status(
    cfg: &Config,
    kind: JobKind,
    id: Uuid,
) -> Result<Option<ServiceJob>, Box<dyn Error>> {
    if cfg.lite_mode {
        let pool = open_config_pool(cfg).await?;
        return Ok(lite_query::service_job(&pool, kind, id).await?);
    }

    Ok(match kind {
        JobKind::Crawl => crate::crates::jobs::crawl::get_job(cfg, id)
            .await?
            .map(crawl_to_service_job),
        JobKind::Embed => crate::crates::jobs::embed::get_embed_job(cfg, id)
            .await?
            .map(embed_to_service_job),
        JobKind::Extract => crate::crates::jobs::extract::get_extract_job(cfg, id)
            .await?
            .map(extract_to_service_job),
        JobKind::Ingest => crate::crates::jobs::ingest::get_ingest_job(cfg, id)
            .await?
            .map(ingest_to_service_job),
        JobKind::Refresh => crate::crates::jobs::refresh::get_refresh_job(cfg, id)
            .await?
            .map(refresh_to_service_job),
        JobKind::Graph => graph_jobs::list_graph_jobs(cfg, 500, 0)
            .await?
            .into_iter()
            .find(|job| job.id == id)
            .map(graph_to_service_job),
    })
}

pub async fn cancel_job(cfg: &Config, kind: JobKind, id: Uuid) -> Result<bool, Box<dyn Error>> {
    if cfg.lite_mode {
        let pool = open_config_pool(cfg).await?;
        return Ok(cancel_row(&pool, kind.table_name(), id).await?);
    }

    match kind {
        JobKind::Crawl => crate::crates::jobs::crawl::cancel_job(cfg, id).await,
        JobKind::Embed => crate::crates::jobs::embed::cancel_embed_job(cfg, id).await,
        JobKind::Extract => crate::crates::jobs::extract::cancel_extract_job(cfg, id).await,
        JobKind::Ingest => crate::crates::jobs::ingest::cancel_ingest_job(cfg, id).await,
        JobKind::Refresh => crate::crates::jobs::refresh::cancel_refresh_job(cfg, id).await,
        JobKind::Graph => Ok(false),
    }
}

pub async fn cleanup_jobs(cfg: &Config, kind: JobKind) -> Result<u64, Box<dyn Error>> {
    if cfg.lite_mode {
        let pool = open_config_pool(cfg).await?;
        return Ok(lite_query::cleanup_jobs(&pool, kind.table_name()).await?);
    }

    match kind {
        JobKind::Crawl => crate::crates::jobs::crawl::cleanup_jobs(cfg).await,
        JobKind::Embed => crate::crates::jobs::embed::cleanup_embed_jobs(cfg).await,
        JobKind::Extract => crate::crates::jobs::extract::cleanup_extract_jobs(cfg).await,
        JobKind::Ingest => crate::crates::jobs::ingest::cleanup_ingest_jobs(cfg).await,
        JobKind::Refresh => crate::crates::jobs::refresh::cleanup_refresh_jobs(cfg).await,
        JobKind::Graph => Ok(0),
    }
}

pub async fn clear_jobs(cfg: &Config, kind: JobKind) -> Result<u64, Box<dyn Error>> {
    if cfg.lite_mode {
        let pool = open_config_pool(cfg).await?;
        return Ok(lite_query::clear_jobs(&pool, kind.table_name()).await?);
    }

    match kind {
        JobKind::Crawl => crate::crates::jobs::crawl::clear_jobs(cfg).await,
        JobKind::Embed => crate::crates::jobs::embed::clear_embed_jobs(cfg).await,
        JobKind::Extract => crate::crates::jobs::extract::clear_extract_jobs(cfg).await,
        JobKind::Ingest => crate::crates::jobs::ingest::clear_ingest_jobs(cfg).await,
        JobKind::Refresh => crate::crates::jobs::refresh::clear_refresh_jobs(cfg).await,
        JobKind::Graph => Ok(0),
    }
}

pub async fn job_errors(
    cfg: &Config,
    kind: JobKind,
    id: Uuid,
) -> Result<Option<String>, Box<dyn Error>> {
    Ok(job_status(cfg, kind, id)
        .await?
        .and_then(|job| job.error_text))
}

pub async fn recover_jobs(cfg: &Config, kind: JobKind) -> Result<u64, Box<dyn Error>> {
    if cfg.lite_mode {
        let pool = open_config_pool(cfg).await?;
        let stale_threshold_ms =
            (cfg.watchdog_stale_timeout_secs + cfg.watchdog_confirm_secs) * 1_000;
        return Ok(reclaim_stale_running_jobs_for_table(
            &pool,
            kind.table_name(),
            stale_threshold_ms,
        )
        .await?);
    }

    match kind {
        JobKind::Crawl => crate::crates::jobs::crawl::recover_stale_crawl_jobs(cfg).await,
        JobKind::Embed => crate::crates::jobs::embed::recover_stale_embed_jobs(cfg).await,
        JobKind::Extract => crate::crates::jobs::extract::recover_stale_extract_jobs(cfg).await,
        JobKind::Ingest => crate::crates::jobs::ingest::recover_stale_ingest_jobs(cfg).await,
        JobKind::Refresh => crate::crates::jobs::refresh::recover_stale_refresh_jobs(cfg).await,
        JobKind::Graph => Ok(0),
    }
}

pub async fn run_worker(cfg: &Config, kind: JobKind) -> Result<WorkerMode, Box<dyn Error>> {
    if cfg.lite_mode {
        return Ok(WorkerMode::InProcess);
    }

    match kind {
        JobKind::Crawl => crate::crates::jobs::crawl::run_worker(cfg).await?,
        JobKind::Embed => crate::crates::jobs::embed::run_embed_worker(cfg).await?,
        JobKind::Extract => crate::crates::jobs::extract::run_extract_worker(cfg).await?,
        JobKind::Ingest => crate::crates::jobs::ingest::run_ingest_worker(cfg).await?,
        JobKind::Refresh => crate::crates::jobs::refresh::run_refresh_worker(cfg).await?,
        JobKind::Graph => return Ok(WorkerMode::Unsupported("graph worker is not exposed here")),
    }
    Ok(WorkerMode::Started)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::jobs::backend::JobPayload;
    use crate::crates::jobs::lite::ops::enqueue_job;
    use crate::crates::jobs::lite::store::open_sqlite_pool;
    use tempfile::NamedTempFile;

    fn lite_cfg(path: &std::path::Path) -> Config {
        let mut cfg = Config::default_lite();
        cfg.sqlite_path = path.to_path_buf();
        cfg
    }

    #[tokio::test]
    async fn lite_list_and_status_use_sqlite_backend() {
        let temp = NamedTempFile::new().expect("temp db file");
        let cfg = lite_cfg(temp.path());
        let pool = open_sqlite_pool(&cfg.sqlite_path.to_string_lossy())
            .await
            .expect("sqlite pool");
        let id = enqueue_job(
            &pool,
            &JobPayload::Embed {
                input: "README.md".into(),
                config_json: "{\"collection\":\"cortex\"}".into(),
            },
        )
        .await
        .expect("enqueue embed job");

        let jobs = list_jobs(&cfg, JobKind::Embed, 50, 0)
            .await
            .expect("list jobs");
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].target.as_deref(), Some("README.md"));

        let job = job_status(&cfg, JobKind::Embed, id)
            .await
            .expect("status")
            .expect("job exists");
        assert_eq!(job.id, id);
        assert_eq!(job.target.as_deref(), Some("README.md"));
    }

    #[tokio::test]
    async fn lite_worker_reports_in_process_mode() {
        let temp = NamedTempFile::new().expect("temp db file");
        let cfg = lite_cfg(temp.path());
        let mode = run_worker(&cfg, JobKind::Crawl).await.expect("worker mode");
        assert!(matches!(mode, WorkerMode::InProcess));
    }
}
