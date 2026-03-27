mod full;
mod mapping;

use std::error::Error;
use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use crate::crates::core::config::Config;
use crate::crates::jobs::backend::{BackendResult, JobBackend, JobKind, JobPayload};
use crate::crates::jobs::full::FullBackend;
use crate::crates::jobs::lite::LiteBackend;
use crate::crates::jobs::lite::ops::cancel_row;
use crate::crates::jobs::lite::query as lite_query;
use crate::crates::jobs::lite::store::reclaim_stale_running_jobs_for_table;
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
    ) -> Result<Vec<ServiceJob>, Box<dyn Error + Send + Sync>>;
    async fn list_ingest_jobs(
        &self,
        source_filter: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ServiceJob>, Box<dyn Error + Send + Sync>> {
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
    ) -> Result<Option<ServiceJob>, Box<dyn Error + Send + Sync>>;
    async fn cancel_job(
        &self,
        kind: JobKind,
        id: Uuid,
    ) -> Result<bool, Box<dyn Error + Send + Sync>>;
    async fn cleanup_jobs(&self, kind: JobKind) -> Result<u64, Box<dyn Error + Send + Sync>>;
    async fn clear_jobs(&self, kind: JobKind) -> Result<u64, Box<dyn Error + Send + Sync>>;
    async fn recover_jobs(
        &self,
        kind: JobKind,
        stale_threshold_ms: i64,
    ) -> Result<u64, Box<dyn Error + Send + Sync>>;
    async fn run_worker(&self, kind: JobKind) -> Result<WorkerMode, Box<dyn Error + Send + Sync>>;
}

/// Convert any `Display` error into `Box<dyn Error + Send + Sync>` by stringifying.
/// Used where underlying functions return `Box<dyn Error>` (without Send+Sync) but
/// the trait requires Send+Sync.
fn lift_ss<E: std::fmt::Display>(e: E) -> Box<dyn Error + Send + Sync> {
    e.to_string().into()
}

pub async fn resolve_runtime(
    cfg: Arc<Config>,
) -> Result<Arc<dyn ServiceJobRuntime>, Box<dyn Error + Send + Sync>> {
    if cfg.lite_mode {
        let backend = LiteBackend::new(Arc::clone(&cfg))
            .await
            .map_err(|e| -> Box<dyn Error + Send + Sync> { e.to_string().into() })?;
        return Ok(Arc::new(LiteServiceRuntime {
            _cfg: cfg,
            backend: Arc::new(backend),
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
            .fetch_one(self.backend.pool().as_ref())
            .await
            .map_err(|e| -> Box<dyn Error + Send + Sync> { e.to_string().into() })?;
        Ok(exists)
    }

    async fn list_jobs(
        &self,
        kind: JobKind,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ServiceJob>, Box<dyn Error + Send + Sync>> {
        Ok(lite_query::list_service_jobs(self.backend.pool(), kind, limit, offset).await?)
    }

    async fn list_ingest_jobs(
        &self,
        source_filter: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ServiceJob>, Box<dyn Error + Send + Sync>> {
        Ok(
            lite_query::list_ingest_service_jobs(self.backend.pool(), source_filter, limit, offset)
                .await?,
        )
    }

    async fn job_status(
        &self,
        kind: JobKind,
        id: Uuid,
    ) -> Result<Option<ServiceJob>, Box<dyn Error + Send + Sync>> {
        Ok(lite_query::service_job(self.backend.pool(), kind, id).await?)
    }

    async fn cancel_job(
        &self,
        kind: JobKind,
        id: Uuid,
    ) -> Result<bool, Box<dyn Error + Send + Sync>> {
        Ok(cancel_row(self.backend.pool(), kind.table_name(), id).await?)
    }

    async fn cleanup_jobs(&self, kind: JobKind) -> Result<u64, Box<dyn Error + Send + Sync>> {
        Ok(lite_query::cleanup_jobs(self.backend.pool(), kind.table_name()).await?)
    }

    async fn clear_jobs(&self, kind: JobKind) -> Result<u64, Box<dyn Error + Send + Sync>> {
        Ok(lite_query::clear_jobs(self.backend.pool(), kind.table_name()).await?)
    }

    async fn recover_jobs(
        &self,
        kind: JobKind,
        stale_threshold_ms: i64,
    ) -> Result<u64, Box<dyn Error + Send + Sync>> {
        Ok(reclaim_stale_running_jobs_for_table(
            self.backend.pool(),
            kind.table_name(),
            stale_threshold_ms,
        )
        .await?)
    }

    async fn run_worker(&self, _kind: JobKind) -> Result<WorkerMode, Box<dyn Error + Send + Sync>> {
        Ok(WorkerMode::InProcess)
    }
}
