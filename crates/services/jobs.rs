use std::error::Error;
use std::fmt;

use uuid::Uuid;

use crate::crates::jobs::backend::JobKind;
use crate::crates::services::context::ServiceContext;
pub use crate::crates::services::runtime::WorkerMode;
use crate::crates::services::types::ServiceJob;

// Helper: downgrade Send+Sync error to plain Box<dyn Error> for callers that don't need Send+Sync.
// Wraps the original error to preserve the Display output and source chain without stringifying.
fn downgrade(e: Box<dyn Error + Send + Sync>) -> Box<dyn Error> {
    struct Wrapper(Box<dyn Error + Send + Sync>);
    impl fmt::Display for Wrapper {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            self.0.fmt(f)
        }
    }
    impl fmt::Debug for Wrapper {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            self.0.fmt(f)
        }
    }
    impl Error for Wrapper {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            Some(self.0.as_ref())
        }
    }
    Box::new(Wrapper(e))
}

pub async fn list_jobs(
    service_context: &ServiceContext,
    kind: JobKind,
    limit: i64,
    offset: i64,
) -> Result<Vec<ServiceJob>, Box<dyn Error>> {
    service_context
        .jobs
        .list_jobs(kind, limit, offset)
        .await
        .map_err(downgrade)
}

pub async fn list_ingest_jobs(
    service_context: &ServiceContext,
    source_filter: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<ServiceJob>, Box<dyn Error>> {
    service_context
        .jobs
        .list_ingest_jobs(source_filter, limit, offset)
        .await
        .map_err(downgrade)
}

pub async fn job_status(
    service_context: &ServiceContext,
    kind: JobKind,
    id: Uuid,
) -> Result<Option<ServiceJob>, Box<dyn Error>> {
    service_context
        .jobs
        .job_status(kind, id)
        .await
        .map_err(downgrade)
}

pub async fn cancel_job(
    service_context: &ServiceContext,
    kind: JobKind,
    id: Uuid,
) -> Result<bool, Box<dyn Error>> {
    service_context
        .jobs
        .cancel_job(kind, id)
        .await
        .map_err(downgrade)
}

pub async fn cleanup_jobs(
    service_context: &ServiceContext,
    kind: JobKind,
) -> Result<u64, Box<dyn Error>> {
    service_context
        .jobs
        .cleanup_jobs(kind)
        .await
        .map_err(downgrade)
}

pub async fn clear_jobs(
    service_context: &ServiceContext,
    kind: JobKind,
) -> Result<u64, Box<dyn Error>> {
    service_context
        .jobs
        .clear_jobs(kind)
        .await
        .map_err(downgrade)
}

pub async fn job_errors(
    service_context: &ServiceContext,
    kind: JobKind,
    id: Uuid,
) -> Result<Option<String>, Box<dyn Error>> {
    Ok(job_status(service_context, kind, id)
        .await?
        .and_then(|job| job.error_text))
}

pub async fn recover_jobs(
    service_context: &ServiceContext,
    kind: JobKind,
) -> Result<u64, Box<dyn Error>> {
    let stale_threshold_ms = (service_context.cfg.watchdog_stale_timeout_secs
        + service_context.cfg.watchdog_confirm_secs)
        .max(0)
        * 1_000;
    service_context
        .jobs
        .recover_jobs(kind, stale_threshold_ms)
        .await
        .map_err(downgrade)
}

pub async fn start_worker(
    service_context: &ServiceContext,
    kind: JobKind,
) -> Result<WorkerMode, Box<dyn Error>> {
    service_context
        .jobs
        .start_worker(kind)
        .await
        .map_err(downgrade)
}

pub async fn notify_worker(
    service_context: &ServiceContext,
    kind: JobKind,
) -> Result<(), Box<dyn Error>> {
    service_context
        .jobs
        .notify_worker(kind)
        .await
        .map_err(downgrade)
}

pub async fn drain_jobs(
    service_context: &ServiceContext,
    kind: JobKind,
) -> Result<WorkerMode, Box<dyn Error>> {
    service_context
        .jobs
        .drain_jobs(kind)
        .await
        .map_err(downgrade)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::jobs::backend::{BackendResult, JobKind, JobPayload};
    use crate::crates::services::runtime::ServiceJobRuntime;
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};
    use uuid::Uuid;

    struct CaptureRuntime {
        seen_filters: Mutex<Vec<Option<String>>>,
    }

    #[async_trait]
    impl ServiceJobRuntime for CaptureRuntime {
        fn mode_name(&self) -> &'static str {
            "test"
        }

        async fn enqueue(&self, _payload: JobPayload) -> BackendResult<Uuid> {
            Err("not implemented".into())
        }

        async fn wait_for_job(&self, _id: Uuid, _kind: JobKind) -> BackendResult<String> {
            Err("not implemented".into())
        }

        async fn job_errors(&self, _id: Uuid, _kind: JobKind) -> BackendResult<Option<String>> {
            Ok(None)
        }

        async fn has_active_jobs(&self, _kind: JobKind) -> BackendResult<bool> {
            Ok(false)
        }

        async fn list_jobs(
            &self,
            _kind: JobKind,
            _limit: i64,
            _offset: i64,
        ) -> Result<Vec<ServiceJob>, Box<dyn Error + Send + Sync>> {
            Ok(Vec::new())
        }

        async fn list_ingest_jobs(
            &self,
            source_filter: Option<&str>,
            _limit: i64,
            _offset: i64,
        ) -> Result<Vec<ServiceJob>, Box<dyn Error + Send + Sync>> {
            self.seen_filters
                .lock()
                .expect("lock")
                .push(source_filter.map(str::to_string));
            Ok(Vec::new())
        }

        async fn job_status(
            &self,
            _kind: JobKind,
            _id: Uuid,
        ) -> Result<Option<ServiceJob>, Box<dyn Error + Send + Sync>> {
            Ok(None)
        }

        async fn cancel_job(
            &self,
            _kind: JobKind,
            _id: Uuid,
        ) -> Result<bool, Box<dyn Error + Send + Sync>> {
            Ok(false)
        }

        async fn cleanup_jobs(&self, _kind: JobKind) -> Result<u64, Box<dyn Error + Send + Sync>> {
            Ok(0)
        }

        async fn clear_jobs(&self, _kind: JobKind) -> Result<u64, Box<dyn Error + Send + Sync>> {
            Ok(0)
        }

        async fn recover_jobs(
            &self,
            _kind: JobKind,
            _stale_threshold_ms: i64,
        ) -> Result<u64, Box<dyn Error + Send + Sync>> {
            Ok(0)
        }

        async fn count_jobs(&self, _kind: JobKind) -> Result<i64, Box<dyn Error + Send + Sync>> {
            Ok(0)
        }
    }

    #[tokio::test]
    async fn list_ingest_jobs_delegates_source_filter_to_runtime() {
        let cfg = Arc::new(crate::crates::core::config::Config::default());
        let runtime = Arc::new(CaptureRuntime {
            seen_filters: Mutex::new(Vec::new()),
        });
        let ctx = ServiceContext::from_runtime(cfg, runtime.clone());

        let jobs = list_ingest_jobs(&ctx, Some("sessions"), 50, 0)
            .await
            .expect("list should succeed");
        assert!(jobs.is_empty());
        assert_eq!(
            runtime.seen_filters.lock().expect("lock").as_slice(),
            &[Some("sessions".to_string())]
        );
    }
}
