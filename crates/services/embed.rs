//! Service-layer wrappers for embed job lifecycle operations and synchronous embedding entry points.

use crate::crates::core::config::Config;
use crate::crates::jobs::backend::{JobKind, JobPayload};
use crate::crates::jobs::lite::config_snapshot::lite_config_snapshot_json;
use crate::crates::services::context::ServiceContext;
use crate::crates::services::events::ServiceEvent;
use crate::crates::services::jobs as job_service;
use crate::crates::services::runtime::ServiceJobRuntime;
use crate::crates::services::runtime::WorkerMode;
use crate::crates::services::types::{
    EmbedJobResult, EmbedStartResult, ExecutionMode, JobStartOutcome, StartDisposition,
};
use crate::crates::vector::ops::{embed_path_native, embed_path_native_with_progress};
use std::error::Error;
use tokio::sync::mpsc;
use uuid::Uuid;

// --- Pure mapping helpers (no I/O, testable without live services) ---

pub fn map_embed_start_result(job_id: String) -> EmbedStartResult {
    EmbedStartResult { job_id }
}

pub fn map_embed_job_result(payload: serde_json::Value) -> EmbedJobResult {
    EmbedJobResult { payload }
}

// --- Service lifecycle wrappers ---

pub async fn embed_status(
    service_context: &ServiceContext,
    id: Uuid,
) -> Result<Option<EmbedJobResult>, Box<dyn Error>> {
    let job = job_service::job_status(service_context, JobKind::Embed, id).await?;
    Ok(job.map(|value| {
        map_embed_job_result(serde_json::to_value(value).unwrap_or(serde_json::Value::Null))
    }))
}

pub async fn embed_list(
    service_context: &ServiceContext,
    limit: i64,
    offset: i64,
) -> Result<EmbedJobResult, Box<dyn Error>> {
    let jobs = job_service::list_jobs(service_context, JobKind::Embed, limit, offset).await?;
    Ok(map_embed_job_result(serde_json::to_value(jobs)?))
}

pub async fn embed_cancel(
    service_context: &ServiceContext,
    id: Uuid,
) -> Result<bool, Box<dyn Error>> {
    job_service::cancel_job(service_context, JobKind::Embed, id).await
}

pub async fn embed_cleanup(service_context: &ServiceContext) -> Result<u64, Box<dyn Error>> {
    job_service::cleanup_jobs(service_context, JobKind::Embed).await
}

pub async fn embed_clear(service_context: &ServiceContext) -> Result<u64, Box<dyn Error>> {
    job_service::clear_jobs(service_context, JobKind::Embed).await
}

pub async fn embed_recover(service_context: &ServiceContext) -> Result<u64, Box<dyn Error>> {
    job_service::recover_jobs(service_context, JobKind::Embed).await
}

pub async fn embed_worker(service_context: &ServiceContext) -> Result<(), Box<dyn Error>> {
    match job_service::start_worker(service_context, JobKind::Embed).await? {
        WorkerMode::Started | WorkerMode::InProcess { .. } => Ok(()),
        WorkerMode::Unsupported(message) => Err(message.into()),
    }
}

// --- Service functions ---

pub async fn embed_start_with_context(
    cfg: &Config,
    input: &str,
    service_context: &ServiceContext,
    tx: Option<mpsc::Sender<ServiceEvent>>,
    _source_type: Option<&str>,
) -> Result<JobStartOutcome<EmbedStartResult>, Box<dyn Error>> {
    // tx is accepted for API compatibility
    let _ = tx;
    let job_id = service_context
        .jobs
        .enqueue(JobPayload::Embed {
            input: input.to_string(),
            config_json: lite_config_snapshot_json(cfg)?,
        })
        .await
        .map_err(|e| -> Box<dyn Error> { e })?;

    if !cfg.wait {
        return Ok(JobStartOutcome {
            disposition: StartDisposition::Enqueued,
            execution_mode: ExecutionMode::InProcess,
            result: map_embed_start_result(job_id.to_string()),
        });
    }

    wait_for_embed_completion(service_context.jobs.as_ref(), job_id).await?;
    Ok(JobStartOutcome {
        disposition: StartDisposition::Completed,
        execution_mode: ExecutionMode::InProcess,
        result: map_embed_start_result(job_id.to_string()),
    })
}

/// Enqueue an embed job for the input specified in cfg and return its job ID
/// immediately. The embed input is resolved from cfg.positional or cfg.output_dir
/// following the same logic as the CLI embed command.
pub async fn embed_start(
    cfg: &Config,
    service_context: &ServiceContext,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<JobStartOutcome<EmbedStartResult>, Box<dyn Error>> {
    let input = cfg.positional.first().cloned().unwrap_or_else(|| {
        cfg.output_dir
            .join("markdown")
            .to_string_lossy()
            .to_string()
    });
    embed_start_with_context(cfg, &input, service_context, tx, None).await
}

pub async fn embed_now(cfg: &Config, input: &str) -> Result<EmbedJobResult, Box<dyn Error>> {
    embed_path_native(cfg, input).await?;
    Ok(map_embed_job_result(serde_json::json!({
        "input": input,
        "collection": cfg.collection,
        "completed": true,
    })))
}

pub async fn embed_now_with_source(
    cfg: &Config,
    input: &str,
    source_type: Option<&str>,
) -> Result<EmbedJobResult, Box<dyn Error>> {
    embed_path_native_with_progress(cfg, input, None, source_type).await?;
    Ok(map_embed_job_result(serde_json::json!({
        "input": input,
        "collection": cfg.collection,
        "completed": true,
    })))
}

async fn wait_for_embed_completion(
    runtime: &dyn ServiceJobRuntime,
    job_id: Uuid,
) -> Result<(), Box<dyn Error>> {
    let final_status = runtime
        .wait_for_job(job_id, JobKind::Embed)
        .await
        .map_err(|e| -> Box<dyn Error> { e })?;
    if final_status == "failed" {
        if let Ok(Some(err)) = runtime.job_errors(job_id, JobKind::Embed).await {
            return Err(format!("embed job {job_id} failed: {err}").into());
        }
        return Err(format!("embed job {job_id} failed").into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::core::config::Config;
    use crate::crates::jobs::backend::BackendResult;
    use crate::crates::services::context::ServiceContext;
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};

    struct CaptureRuntime {
        payloads: Mutex<Vec<JobPayload>>,
    }

    #[async_trait]
    impl ServiceJobRuntime for CaptureRuntime {
        fn mode_name(&self) -> &'static str {
            "test"
        }

        async fn enqueue(&self, payload: JobPayload) -> BackendResult<Uuid> {
            self.payloads.lock().expect("lock").push(payload);
            Ok(Uuid::new_v4())
        }

        async fn wait_for_job(&self, _id: Uuid, _kind: JobKind) -> BackendResult<String> {
            panic!("--wait false embed start must enqueue without waiting")
        }

        async fn job_errors(&self, _id: Uuid, _kind: JobKind) -> BackendResult<Option<String>> {
            Ok(None)
        }

        async fn has_active_jobs(&self, _kind: JobKind) -> BackendResult<bool> {
            panic!("--wait false embed start must not drain the queue")
        }

        async fn list_jobs(
            &self,
            _kind: JobKind,
            _limit: i64,
            _offset: i64,
        ) -> Result<Vec<crate::crates::services::types::ServiceJob>, Box<dyn Error + Send + Sync>>
        {
            Ok(vec![])
        }

        async fn job_status(
            &self,
            _kind: JobKind,
            _id: Uuid,
        ) -> Result<Option<crate::crates::services::types::ServiceJob>, Box<dyn Error + Send + Sync>>
        {
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
    async fn embed_start_with_context_enqueues_without_blocking_when_wait_false()
    -> Result<(), Box<dyn Error + Send + Sync>> {
        let mut cfg = Config::test_default();
        cfg.wait = false;
        let runtime = Arc::new(CaptureRuntime {
            payloads: Mutex::new(Vec::new()),
        });
        let ctx = ServiceContext::from_runtime(Arc::new(cfg.clone()), runtime.clone());

        let outcome = embed_start_with_context(&cfg, "./README.md", &ctx, None, None)
            .await
            .map_err(|e| e.to_string())?;

        assert_eq!(outcome.disposition, StartDisposition::Enqueued);
        assert_eq!(outcome.execution_mode, ExecutionMode::InProcess);
        assert_eq!(runtime.payloads.lock().expect("lock").len(), 1);
        Ok(())
    }
}
