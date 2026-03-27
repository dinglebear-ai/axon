use std::error::Error;
use std::sync::Arc;

use async_trait::async_trait;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::crates::core::config::Config;
use crate::crates::jobs::backend::{BackendResult, JobBackend, JobKind, JobPayload};
use crate::crates::jobs::full::FullBackend;
use crate::crates::jobs::graph::{self as graph_jobs, GraphJob};
use crate::crates::jobs::lite::LiteBackend;
use crate::crates::jobs::lite::ops::cancel_row;
use crate::crates::jobs::lite::query as lite_query;
use crate::crates::jobs::lite::store::{open_config_pool, reclaim_stale_running_jobs_for_table};
use crate::crates::jobs::status::JobStatus;
use crate::crates::services::types::ServiceJob;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerMode {
    Started,
    InProcess,
    Unsupported(&'static str),
}

// NOTE: #[async_trait] is required here because this trait is used as
// `dyn ServiceJobRuntime` (object safety). Native async fn in traits (Rust 1.75+)
// uses RPITIT which makes the trait non-object-safe. Once all callers are
// converted to generics, this can be removed.
#[async_trait]
pub trait ServiceJobRuntime: Send + Sync {
    fn mode_name(&self) -> &'static str;

    async fn enqueue(&self, payload: JobPayload) -> BackendResult<Uuid>;
    async fn wait_for_job(&self, id: Uuid, kind: JobKind) -> BackendResult<String>;
    async fn job_errors(&self, id: Uuid, kind: JobKind) -> BackendResult<Option<String>>;
    async fn has_active_jobs(&self, kind: JobKind) -> BackendResult<bool>;

    async fn list_jobs(
        &self,
        kind: JobKind,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ServiceJob>, Box<dyn Error>>;
    async fn list_ingest_jobs(
        &self,
        source_filter: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ServiceJob>, Box<dyn Error>> {
        let jobs = self.list_jobs(JobKind::Ingest, limit, offset).await?;
        if let Some(filter) = source_filter {
            Ok(jobs
                .into_iter()
                .filter(|job| job.source_type.as_deref() == Some(filter))
                .collect())
        } else {
            Ok(jobs)
        }
    }
    async fn job_status(
        &self,
        kind: JobKind,
        id: Uuid,
    ) -> Result<Option<ServiceJob>, Box<dyn Error>>;
    async fn cancel_job(&self, kind: JobKind, id: Uuid) -> Result<bool, Box<dyn Error>>;
    async fn cleanup_jobs(&self, kind: JobKind) -> Result<u64, Box<dyn Error>>;
    async fn clear_jobs(&self, kind: JobKind) -> Result<u64, Box<dyn Error>>;
    async fn recover_jobs(
        &self,
        kind: JobKind,
        stale_threshold_ms: i64,
    ) -> Result<u64, Box<dyn Error>>;
    async fn run_worker(&self, kind: JobKind) -> Result<WorkerMode, Box<dyn Error>>;
}

pub async fn resolve_runtime(
    cfg: Arc<Config>,
) -> Result<Arc<dyn ServiceJobRuntime>, Box<dyn Error + Send + Sync>> {
    if cfg.lite_mode {
        let pool = open_config_pool(&cfg)
            .await
            .map_err(|e| -> Box<dyn Error + Send + Sync> { e.to_string().into() })?;
        let backend = LiteBackend::new(Arc::clone(&cfg))
            .await
            .map_err(|e| -> Box<dyn Error + Send + Sync> { e.to_string().into() })?;
        return Ok(Arc::new(LiteServiceRuntime {
            _cfg: cfg,
            backend: Arc::new(backend),
            pool,
        }));
    }

    let backend = FullBackend::new(Arc::clone(&cfg));
    Ok(Arc::new(FullServiceRuntime {
        cfg,
        backend: Arc::new(backend),
    }))
}

pub struct FullServiceRuntime {
    cfg: Arc<Config>,
    backend: Arc<FullBackend>,
}

pub struct LiteServiceRuntime {
    _cfg: Arc<Config>,
    backend: Arc<LiteBackend>,
    pool: SqlitePool,
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

fn has_active_status(status: JobStatus) -> bool {
    matches!(status, JobStatus::Pending | JobStatus::Running)
}

#[async_trait]
impl ServiceJobRuntime for FullServiceRuntime {
    fn mode_name(&self) -> &'static str {
        "full"
    }

    async fn enqueue(&self, payload: JobPayload) -> BackendResult<Uuid> {
        self.backend.enqueue(payload).await
    }

    async fn wait_for_job(&self, id: Uuid, kind: JobKind) -> BackendResult<String> {
        self.backend.wait_for_job(id, kind).await
    }

    async fn job_errors(&self, id: Uuid, kind: JobKind) -> BackendResult<Option<String>> {
        self.backend.job_errors(id, kind).await
    }

    // NOTE: FullBackend::list_jobs fetches up to 500 rows server-side. A proper
    // fix requires adding a count_active_jobs query to the JobBackend trait or
    // per-kind Postgres modules. Short-circuit .iter().any() avoids extra
    // allocation but the 500-row fetch remains a backend limitation.
    async fn has_active_jobs(&self, kind: JobKind) -> BackendResult<bool> {
        let jobs = self.backend.list_jobs(kind).await?;
        Ok(jobs.iter().any(|job| has_active_status(job.status)))
    }

    async fn list_jobs(
        &self,
        kind: JobKind,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ServiceJob>, Box<dyn Error>> {
        Ok(match kind {
            JobKind::Crawl => crate::crates::jobs::crawl::list_jobs(&self.cfg, limit, offset)
                .await?
                .into_iter()
                .map(crawl_to_service_job)
                .collect(),
            JobKind::Embed => crate::crates::jobs::embed::list_embed_jobs(&self.cfg, limit, offset)
                .await?
                .into_iter()
                .map(embed_to_service_job)
                .collect(),
            JobKind::Extract => {
                crate::crates::jobs::extract::list_extract_jobs(&self.cfg, limit, offset)
                    .await?
                    .into_iter()
                    .map(extract_to_service_job)
                    .collect()
            }
            JobKind::Ingest => {
                crate::crates::jobs::ingest::list_ingest_jobs(&self.cfg, None, limit, offset)
                    .await?
                    .into_iter()
                    .map(ingest_to_service_job)
                    .collect()
            }
            JobKind::Refresh => {
                crate::crates::jobs::refresh::list_refresh_jobs(&self.cfg, limit, offset)
                    .await?
                    .into_iter()
                    .map(refresh_to_service_job)
                    .collect()
            }
            JobKind::Graph => graph_jobs::list_graph_jobs(&self.cfg, limit, offset)
                .await?
                .into_iter()
                .map(graph_to_service_job)
                .collect(),
        })
    }

    async fn list_ingest_jobs(
        &self,
        source_filter: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ServiceJob>, Box<dyn Error>> {
        Ok(
            crate::crates::jobs::ingest::list_ingest_jobs(&self.cfg, source_filter, limit, offset)
                .await?
                .into_iter()
                .map(ingest_to_service_job)
                .collect(),
        )
    }

    async fn job_status(
        &self,
        kind: JobKind,
        id: Uuid,
    ) -> Result<Option<ServiceJob>, Box<dyn Error>> {
        Ok(match kind {
            JobKind::Crawl => crate::crates::jobs::crawl::get_job(&self.cfg, id)
                .await?
                .map(crawl_to_service_job),
            JobKind::Embed => crate::crates::jobs::embed::get_embed_job(&self.cfg, id)
                .await?
                .map(embed_to_service_job),
            JobKind::Extract => crate::crates::jobs::extract::get_extract_job(&self.cfg, id)
                .await?
                .map(extract_to_service_job),
            JobKind::Ingest => crate::crates::jobs::ingest::get_ingest_job(&self.cfg, id)
                .await?
                .map(ingest_to_service_job),
            JobKind::Refresh => crate::crates::jobs::refresh::get_refresh_job(&self.cfg, id)
                .await?
                .map(refresh_to_service_job),
            JobKind::Graph => graph_jobs::get_graph_job(&self.cfg, id)
                .await?
                .map(graph_to_service_job),
        })
    }

    async fn cancel_job(&self, kind: JobKind, id: Uuid) -> Result<bool, Box<dyn Error>> {
        match kind {
            JobKind::Crawl => crate::crates::jobs::crawl::cancel_job(&self.cfg, id).await,
            JobKind::Embed => crate::crates::jobs::embed::cancel_embed_job(&self.cfg, id).await,
            JobKind::Extract => {
                crate::crates::jobs::extract::cancel_extract_job(&self.cfg, id).await
            }
            JobKind::Ingest => crate::crates::jobs::ingest::cancel_ingest_job(&self.cfg, id).await,
            JobKind::Refresh => {
                crate::crates::jobs::refresh::cancel_refresh_job(&self.cfg, id).await
            }
            JobKind::Graph => Ok(false),
        }
    }

    async fn cleanup_jobs(&self, kind: JobKind) -> Result<u64, Box<dyn Error>> {
        match kind {
            JobKind::Crawl => crate::crates::jobs::crawl::cleanup_jobs(&self.cfg).await,
            JobKind::Embed => crate::crates::jobs::embed::cleanup_embed_jobs(&self.cfg).await,
            JobKind::Extract => crate::crates::jobs::extract::cleanup_extract_jobs(&self.cfg).await,
            JobKind::Ingest => crate::crates::jobs::ingest::cleanup_ingest_jobs(&self.cfg).await,
            JobKind::Refresh => crate::crates::jobs::refresh::cleanup_refresh_jobs(&self.cfg).await,
            JobKind::Graph => Ok(0),
        }
    }

    async fn clear_jobs(&self, kind: JobKind) -> Result<u64, Box<dyn Error>> {
        match kind {
            JobKind::Crawl => crate::crates::jobs::crawl::clear_jobs(&self.cfg).await,
            JobKind::Embed => crate::crates::jobs::embed::clear_embed_jobs(&self.cfg).await,
            JobKind::Extract => crate::crates::jobs::extract::clear_extract_jobs(&self.cfg).await,
            JobKind::Ingest => crate::crates::jobs::ingest::clear_ingest_jobs(&self.cfg).await,
            JobKind::Refresh => crate::crates::jobs::refresh::clear_refresh_jobs(&self.cfg).await,
            JobKind::Graph => Ok(0),
        }
    }

    async fn recover_jobs(
        &self,
        kind: JobKind,
        _stale_threshold_ms: i64,
    ) -> Result<u64, Box<dyn Error>> {
        match kind {
            JobKind::Crawl => crate::crates::jobs::crawl::recover_stale_crawl_jobs(&self.cfg).await,
            JobKind::Embed => crate::crates::jobs::embed::recover_stale_embed_jobs(&self.cfg).await,
            JobKind::Extract => {
                crate::crates::jobs::extract::recover_stale_extract_jobs(&self.cfg).await
            }
            JobKind::Ingest => {
                crate::crates::jobs::ingest::recover_stale_ingest_jobs(&self.cfg).await
            }
            JobKind::Refresh => {
                crate::crates::jobs::refresh::recover_stale_refresh_jobs(&self.cfg).await
            }
            JobKind::Graph => Ok(0),
        }
    }

    async fn run_worker(&self, kind: JobKind) -> Result<WorkerMode, Box<dyn Error>> {
        let cfg = Arc::clone(&self.cfg);
        let (tx, rx) = tokio::sync::oneshot::channel();
        std::thread::spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(err) => {
                    let _ = tx.send(Err::<WorkerMode, String>(err.to_string()));
                    return;
                }
            };
            let local = tokio::task::LocalSet::new();
            local.block_on(&runtime, async move {
                let result = match kind {
                    JobKind::Crawl => crate::crates::jobs::crawl::run_worker(&cfg)
                        .await
                        .map(|_| WorkerMode::Started)
                        .map_err(|err| err.to_string()),
                    JobKind::Embed => crate::crates::jobs::embed::run_embed_worker(&cfg)
                        .await
                        .map(|_| WorkerMode::Started)
                        .map_err(|err| err.to_string()),
                    JobKind::Extract => crate::crates::jobs::extract::run_extract_worker(&cfg)
                        .await
                        .map(|_| WorkerMode::Started)
                        .map_err(|err| err.to_string()),
                    JobKind::Ingest => crate::crates::jobs::ingest::run_ingest_worker(&cfg)
                        .await
                        .map(|_| WorkerMode::Started)
                        .map_err(|err| err.to_string()),
                    JobKind::Refresh => crate::crates::jobs::refresh::run_refresh_worker(&cfg)
                        .await
                        .map(|_| WorkerMode::Started)
                        .map_err(|err| err.to_string()),
                    JobKind::Graph => Ok::<WorkerMode, String>(WorkerMode::Unsupported(
                        "graph worker is not exposed here",
                    )),
                };
                let _ = tx.send(result);
            });
        });

        match rx.await {
            Ok(Ok(mode)) => Ok(mode),
            Ok(Err(err)) => Err(err.into()),
            Err(_) => Err("worker thread panicked".into()),
        }
    }
}

#[async_trait]
impl ServiceJobRuntime for LiteServiceRuntime {
    fn mode_name(&self) -> &'static str {
        "lite"
    }

    async fn enqueue(&self, payload: JobPayload) -> BackendResult<Uuid> {
        self.backend.enqueue(payload).await
    }

    async fn wait_for_job(&self, id: Uuid, kind: JobKind) -> BackendResult<String> {
        self.backend.wait_for_job(id, kind).await
    }

    async fn job_errors(&self, id: Uuid, kind: JobKind) -> BackendResult<Option<String>> {
        self.backend.job_errors(id, kind).await
    }

    /// SQL EXISTS check against the cached pool — avoids fetching all rows.
    async fn has_active_jobs(&self, kind: JobKind) -> BackendResult<bool> {
        let table = kind.table_name();
        let sql = format!(
            "SELECT EXISTS(SELECT 1 FROM {} WHERE status IN ('pending','running') LIMIT 1)",
            table,
        );
        let exists: bool = sqlx::query_scalar(&sql)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| -> Box<dyn Error + Send + Sync> { e.to_string().into() })?;
        Ok(exists)
    }

    async fn list_jobs(
        &self,
        kind: JobKind,
        _limit: i64,
        _offset: i64,
    ) -> Result<Vec<ServiceJob>, Box<dyn Error>> {
        Ok(lite_query::list_service_jobs(&self.pool, kind).await?)
    }

    async fn list_ingest_jobs(
        &self,
        source_filter: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ServiceJob>, Box<dyn Error>> {
        Ok(lite_query::list_ingest_service_jobs(&self.pool, source_filter, limit, offset).await?)
    }

    async fn job_status(
        &self,
        kind: JobKind,
        id: Uuid,
    ) -> Result<Option<ServiceJob>, Box<dyn Error>> {
        Ok(lite_query::service_job(&self.pool, kind, id).await?)
    }

    async fn cancel_job(&self, kind: JobKind, id: Uuid) -> Result<bool, Box<dyn Error>> {
        Ok(cancel_row(&self.pool, kind.table_name(), id).await?)
    }

    async fn cleanup_jobs(&self, kind: JobKind) -> Result<u64, Box<dyn Error>> {
        Ok(lite_query::cleanup_jobs(&self.pool, kind.table_name()).await?)
    }

    async fn clear_jobs(&self, kind: JobKind) -> Result<u64, Box<dyn Error>> {
        Ok(lite_query::clear_jobs(&self.pool, kind.table_name()).await?)
    }

    async fn recover_jobs(
        &self,
        kind: JobKind,
        stale_threshold_ms: i64,
    ) -> Result<u64, Box<dyn Error>> {
        Ok(
            reclaim_stale_running_jobs_for_table(&self.pool, kind.table_name(), stale_threshold_ms)
                .await?,
        )
    }

    async fn run_worker(&self, _kind: JobKind) -> Result<WorkerMode, Box<dyn Error>> {
        Ok(WorkerMode::InProcess)
    }
}
