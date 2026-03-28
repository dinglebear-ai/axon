use std::error::Error;
use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use crate::crates::jobs::backend::{BackendResult, JobBackend, JobKind, JobPayload};
use crate::crates::services::types::ServiceJob;

use super::{FullServiceRuntime, ServiceJobRuntime, WorkerMode, lift_ss};

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

    async fn has_active_jobs(&self, kind: JobKind) -> BackendResult<bool> {
        let pool = self
            .pool
            .get_or_try_init(|| async {
                crate::crates::jobs::common::make_pool(&self.cfg)
                    .await
                    .map_err(lift_ss)
            })
            .await?;
        let table = match kind {
            JobKind::Crawl => "axon_crawl_jobs",
            JobKind::Embed => "axon_embed_jobs",
            JobKind::Extract => "axon_extract_jobs",
            JobKind::Ingest => "axon_ingest_jobs",
            JobKind::Refresh => "axon_refresh_jobs",
            JobKind::Graph => "axon_graph_jobs",
        };
        let sql = format!(
            "SELECT EXISTS(SELECT 1 FROM {} WHERE status IN ('pending','running') LIMIT 1)",
            table
        );
        let exists: bool = sqlx::query_scalar(&sql)
            .fetch_one(pool)
            .await
            .map_err(lift_ss)?;
        Ok(exists)
    }

    async fn list_jobs(
        &self,
        kind: JobKind,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ServiceJob>, Box<dyn Error + Send + Sync>> {
        Ok(match kind {
            JobKind::Crawl => crate::crates::jobs::crawl::list_jobs(&self.cfg, limit, offset)
                .await
                .map_err(lift_ss)?
                .into_iter()
                .map(ServiceJob::from)
                .collect(),
            JobKind::Embed => crate::crates::jobs::embed::list_embed_jobs(&self.cfg, limit, offset)
                .await
                .map_err(lift_ss)?
                .into_iter()
                .map(ServiceJob::from)
                .collect(),
            JobKind::Extract => {
                crate::crates::jobs::extract::list_extract_jobs(&self.cfg, limit, offset)
                    .await
                    .map_err(lift_ss)?
                    .into_iter()
                    .map(ServiceJob::from)
                    .collect()
            }
            JobKind::Ingest => {
                crate::crates::jobs::ingest::list_ingest_jobs(&self.cfg, None, limit, offset)
                    .await
                    .map_err(lift_ss)?
                    .into_iter()
                    .map(ServiceJob::from)
                    .collect()
            }
            JobKind::Refresh => {
                crate::crates::jobs::refresh::list_refresh_jobs(&self.cfg, limit, offset)
                    .await
                    .map_err(lift_ss)?
                    .into_iter()
                    .map(ServiceJob::from)
                    .collect()
            }
            JobKind::Graph => crate::crates::jobs::graph::list_graph_jobs(&self.cfg, limit, offset)
                .await
                .map_err(lift_ss)?
                .into_iter()
                .map(ServiceJob::from)
                .collect(),
        })
    }

    async fn list_ingest_jobs(
        &self,
        source_filter: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ServiceJob>, Box<dyn Error + Send + Sync>> {
        Ok(
            crate::crates::jobs::ingest::list_ingest_jobs(&self.cfg, source_filter, limit, offset)
                .await
                .map_err(lift_ss)?
                .into_iter()
                .map(ServiceJob::from)
                .collect(),
        )
    }

    async fn job_status(
        &self,
        kind: JobKind,
        id: Uuid,
    ) -> Result<Option<ServiceJob>, Box<dyn Error + Send + Sync>> {
        Ok(match kind {
            JobKind::Crawl => crate::crates::jobs::crawl::get_job(&self.cfg, id)
                .await
                .map_err(lift_ss)?
                .map(ServiceJob::from),
            JobKind::Embed => crate::crates::jobs::embed::get_embed_job(&self.cfg, id)
                .await
                .map_err(lift_ss)?
                .map(ServiceJob::from),
            JobKind::Extract => crate::crates::jobs::extract::get_extract_job(&self.cfg, id)
                .await
                .map_err(lift_ss)?
                .map(ServiceJob::from),
            JobKind::Ingest => crate::crates::jobs::ingest::get_ingest_job(&self.cfg, id)
                .await
                .map_err(lift_ss)?
                .map(ServiceJob::from),
            JobKind::Refresh => crate::crates::jobs::refresh::get_refresh_job(&self.cfg, id)
                .await
                .map_err(lift_ss)?
                .map(ServiceJob::from),
            JobKind::Graph => crate::crates::jobs::graph::get_graph_job(&self.cfg, id)
                .await
                .map_err(lift_ss)?
                .map(ServiceJob::from),
        })
    }

    async fn cancel_job(
        &self,
        kind: JobKind,
        id: Uuid,
    ) -> Result<bool, Box<dyn Error + Send + Sync>> {
        match kind {
            JobKind::Crawl => crate::crates::jobs::crawl::cancel_job(&self.cfg, id)
                .await
                .map_err(|e| e.to_string().into()),
            JobKind::Embed => crate::crates::jobs::embed::cancel_embed_job(&self.cfg, id)
                .await
                .map_err(|e| e.to_string().into()),
            JobKind::Extract => crate::crates::jobs::extract::cancel_extract_job(&self.cfg, id)
                .await
                .map_err(|e| e.to_string().into()),
            JobKind::Ingest => crate::crates::jobs::ingest::cancel_ingest_job(&self.cfg, id)
                .await
                .map_err(|e| e.to_string().into()),
            JobKind::Refresh => crate::crates::jobs::refresh::cancel_refresh_job(&self.cfg, id)
                .await
                .map_err(|e| e.to_string().into()),
            JobKind::Graph => Err("cancel_job for Graph is not implemented in full mode".into()),
        }
    }

    async fn cleanup_jobs(&self, kind: JobKind) -> Result<u64, Box<dyn Error + Send + Sync>> {
        match kind {
            JobKind::Crawl => crate::crates::jobs::crawl::cleanup_jobs(&self.cfg)
                .await
                .map_err(|e| e.to_string().into()),
            JobKind::Embed => crate::crates::jobs::embed::cleanup_embed_jobs(&self.cfg)
                .await
                .map_err(|e| e.to_string().into()),
            JobKind::Extract => crate::crates::jobs::extract::cleanup_extract_jobs(&self.cfg)
                .await
                .map_err(|e| e.to_string().into()),
            JobKind::Ingest => crate::crates::jobs::ingest::cleanup_ingest_jobs(&self.cfg)
                .await
                .map_err(|e| e.to_string().into()),
            JobKind::Refresh => crate::crates::jobs::refresh::cleanup_refresh_jobs(&self.cfg)
                .await
                .map_err(|e| e.to_string().into()),
            JobKind::Graph => Err("cleanup_jobs for Graph is not implemented in full mode".into()),
        }
    }

    async fn clear_jobs(&self, kind: JobKind) -> Result<u64, Box<dyn Error + Send + Sync>> {
        match kind {
            JobKind::Crawl => crate::crates::jobs::crawl::clear_jobs(&self.cfg)
                .await
                .map_err(|e| e.to_string().into()),
            JobKind::Embed => crate::crates::jobs::embed::clear_embed_jobs(&self.cfg)
                .await
                .map_err(|e| e.to_string().into()),
            JobKind::Extract => crate::crates::jobs::extract::clear_extract_jobs(&self.cfg)
                .await
                .map_err(|e| e.to_string().into()),
            JobKind::Ingest => crate::crates::jobs::ingest::clear_ingest_jobs(&self.cfg)
                .await
                .map_err(|e| e.to_string().into()),
            JobKind::Refresh => crate::crates::jobs::refresh::clear_refresh_jobs(&self.cfg)
                .await
                .map_err(|e| e.to_string().into()),
            JobKind::Graph => Err("clear_jobs for Graph is not implemented in full mode".into()),
        }
    }

    async fn recover_jobs(
        &self,
        kind: JobKind,
        _stale_threshold_ms: i64,
    ) -> Result<u64, Box<dyn Error + Send + Sync>> {
        match kind {
            JobKind::Crawl => crate::crates::jobs::crawl::recover_stale_crawl_jobs(&self.cfg)
                .await
                .map_err(|e| e.to_string().into()),
            JobKind::Embed => crate::crates::jobs::embed::recover_stale_embed_jobs(&self.cfg)
                .await
                .map_err(|e| e.to_string().into()),
            JobKind::Extract => crate::crates::jobs::extract::recover_stale_extract_jobs(&self.cfg)
                .await
                .map_err(|e| e.to_string().into()),
            JobKind::Ingest => crate::crates::jobs::ingest::recover_stale_ingest_jobs(&self.cfg)
                .await
                .map_err(|e| e.to_string().into()),
            JobKind::Refresh => crate::crates::jobs::refresh::recover_stale_refresh_jobs(&self.cfg)
                .await
                .map_err(|e| e.to_string().into()),
            JobKind::Graph => Err("recover_jobs for Graph is not implemented in full mode".into()),
        }
    }

    async fn run_worker(&self, kind: JobKind) -> Result<WorkerMode, Box<dyn Error + Send + Sync>> {
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
