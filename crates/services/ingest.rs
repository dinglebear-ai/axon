use crate::crates::core::config::Config;
use crate::crates::ingest;
use crate::crates::ingest::progress::PhaseReporter;
use crate::crates::jobs::backend::{JobKind, JobPayload};
use crate::crates::jobs::ingest::types::{source_type_label, target_label};
pub use crate::crates::jobs::ingest::{IngestJob, IngestSource};
use crate::crates::jobs::ingest::{
    count_ingest_jobs, get_ingest_job, list_ingest_jobs, start_ingest_job,
};
use crate::crates::jobs::lite::config_snapshot::ingest_config_json;
use crate::crates::services::context::ServiceContext;
use crate::crates::services::events::{LogLevel, ServiceEvent, emit};
use crate::crates::services::jobs as job_service;
use crate::crates::services::runtime::WorkerMode;
use crate::crates::services::types::{
    ExecutionMode, IngestJobResult, IngestResult, IngestStartResult, JobListResult,
    JobStartOutcome, StartDisposition,
};
use std::error::Error;
use tokio::sync::mpsc;
use uuid::Uuid;

pub mod classify;
pub use classify::classify_target;

// --- Pure mapping helper (no I/O, testable without live services) ---

pub fn map_ingest_result(payload: serde_json::Value) -> IngestResult {
    IngestResult { payload }
}

pub fn map_ingest_start_result(job_id: String) -> IngestStartResult {
    IngestStartResult { job_id }
}

pub fn map_ingest_job_result(payload: serde_json::Value) -> IngestJobResult {
    IngestJobResult { payload }
}

// --- Service lifecycle wrappers ---

pub async fn ingest_start(
    cfg: &Config,
    source: IngestSource,
) -> Result<IngestStartResult, Box<dyn Error>> {
    let job_id = start_ingest_job(cfg, source).await?;
    Ok(map_ingest_start_result(job_id.to_string()))
}

pub async fn ingest_start_with_context(
    cfg: &Config,
    source: IngestSource,
    service_context: &ServiceContext,
) -> Result<JobStartOutcome<IngestStartResult>, Box<dyn Error>> {
    // Always route through service_context.jobs.enqueue() so that notify()
    // fires immediately and workers wake without 0-5 second polling delay.
    // The previous `if !cfg.lite_mode` branch called start_ingest_job() which
    // opened a fresh SQLite pool per call (re-running migrations) and never
    // called notify().
    let source_type = source_type_label(&source).to_string();
    let target = target_label(&source);
    let config_json = ingest_config_json(cfg, &source)?;
    let job_id = service_context
        .jobs
        .enqueue(JobPayload::Ingest {
            target,
            source_type,
            config_json,
        })
        .await
        .map_err(|e| -> Box<dyn Error> { e })?;
    Ok(JobStartOutcome {
        disposition: StartDisposition::Enqueued,
        execution_mode: ExecutionMode::InProcess,
        result: map_ingest_start_result(job_id.to_string()),
    })
}

pub async fn ingest_status(
    service_context: &ServiceContext,
    id: Uuid,
) -> Result<Option<IngestJobResult>, Box<dyn Error>> {
    let job = job_service::job_status(service_context, JobKind::Ingest, id).await?;
    Ok(job.map(|value| {
        map_ingest_job_result(serde_json::to_value(value).unwrap_or(serde_json::Value::Null))
    }))
}

pub async fn ingest_list(
    service_context: &ServiceContext,
    limit: i64,
    offset: i64,
) -> Result<IngestResult, Box<dyn Error>> {
    let jobs = job_service::list_jobs(service_context, JobKind::Ingest, limit, offset).await?;
    Ok(map_ingest_result(serde_json::to_value(jobs)?))
}

pub async fn ingest_cancel(
    service_context: &ServiceContext,
    id: Uuid,
) -> Result<bool, Box<dyn Error>> {
    job_service::cancel_job(service_context, JobKind::Ingest, id).await
}

pub async fn ingest_cleanup(service_context: &ServiceContext) -> Result<u64, Box<dyn Error>> {
    job_service::cleanup_jobs(service_context, JobKind::Ingest).await
}

pub async fn ingest_clear(service_context: &ServiceContext) -> Result<u64, Box<dyn Error>> {
    job_service::clear_jobs(service_context, JobKind::Ingest).await
}

pub async fn ingest_recover(service_context: &ServiceContext) -> Result<u64, Box<dyn Error>> {
    job_service::recover_jobs(service_context, JobKind::Ingest).await
}

pub async fn ingest_count(cfg: &Config) -> Result<i64, Box<dyn Error>> {
    count_ingest_jobs(cfg).await
}

pub async fn ingest_status_raw(
    cfg: &Config,
    id: Uuid,
) -> Result<Option<IngestJob>, Box<dyn Error>> {
    get_ingest_job(cfg, id).await
}

pub async fn ingest_list_raw(
    cfg: &Config,
    limit: i64,
    offset: i64,
) -> Result<JobListResult<IngestJob>, Box<dyn Error>> {
    let (jobs, total) = tokio::join!(
        list_ingest_jobs(cfg, None, limit, offset),
        count_ingest_jobs(cfg),
    );
    let jobs = jobs?;
    let total = total.unwrap_or(jobs.len() as i64);
    Ok(JobListResult::new(jobs, total, limit, offset))
}

pub async fn ingest_worker(service_context: &ServiceContext) -> Result<(), Box<dyn Error>> {
    match job_service::start_worker(service_context, JobKind::Ingest).await? {
        WorkerMode::Started | WorkerMode::InProcess { .. } => Ok(()),
        WorkerMode::Unsupported(message) => Err(message.into()),
    }
}

// --- Service functions ---

/// Ingest a GitHub repository (code, issues, PRs, wiki) into the vector store.
///
/// Calls `ingest::github::ingest_github` which performs the fetch and embed
/// synchronously. For async/fire-and-forget behaviour use the job queue via
/// the ingest CLI command.
#[must_use = "ingest_github returns a Result that should be handled"]
pub async fn ingest_github(
    cfg: &Config,
    owner: &str,
    repo: &str,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<IngestResult, Box<dyn Error>> {
    ingest_github_with_progress(cfg, owner, repo, tx, None).await
}

/// Ingest a GitHub repository with an optional structured progress sink.
#[must_use = "ingest_github_with_progress returns a Result that should be handled"]
pub async fn ingest_github_with_progress(
    cfg: &Config,
    owner: &str,
    repo: &str,
    tx: Option<mpsc::Sender<ServiceEvent>>,
    progress_tx: Option<mpsc::Sender<serde_json::Value>>,
) -> Result<IngestResult, Box<dyn Error>> {
    let repo_slug = format!("{owner}/{repo}");

    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("ingesting github repo: {repo_slug}"),
        },
    )
    .await;

    let chunks = ingest::github::ingest_github(
        cfg,
        &repo_slug,
        cfg.github_include_source,
        PhaseReporter::new(progress_tx),
    )
    .await
    .map_err(|e| -> Box<dyn Error> {
        format!("github ingest failed for {repo_slug}: {e}").into()
    })?;

    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("github ingest complete: {chunks} chunks"),
        },
    )
    .await;

    let payload = serde_json::json!({
        "source": "github",
        "repo": repo_slug,
        "chunks": chunks,
    });
    Ok(map_ingest_result(payload))
}

/// Ingest a Reddit subreddit or thread into the vector store.
///
/// `target` may be a subreddit name (e.g. `"rust"`) or a full thread URL.
#[must_use = "ingest_reddit returns a Result that should be handled"]
pub async fn ingest_reddit(
    cfg: &Config,
    target: &str,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<IngestResult, Box<dyn Error>> {
    ingest_reddit_with_progress(cfg, target, tx, None).await
}

/// Ingest a Reddit subreddit or thread with an optional structured progress sink.
#[must_use = "ingest_reddit_with_progress returns a Result that should be handled"]
pub async fn ingest_reddit_with_progress(
    cfg: &Config,
    target: &str,
    tx: Option<mpsc::Sender<ServiceEvent>>,
    progress_tx: Option<mpsc::Sender<serde_json::Value>>,
) -> Result<IngestResult, Box<dyn Error>> {
    ingest_reddit_with_progress_and_options(
        cfg,
        target,
        tx,
        progress_tx,
        &ingest::reddit::RedditIngestOptions::default(),
    )
    .await
}

/// Ingest a Reddit subreddit or thread with progress and source-local controls.
#[must_use = "ingest_reddit_with_progress_and_options returns a Result that should be handled"]
pub async fn ingest_reddit_with_progress_and_options(
    cfg: &Config,
    target: &str,
    tx: Option<mpsc::Sender<ServiceEvent>>,
    progress_tx: Option<mpsc::Sender<serde_json::Value>>,
    options: &ingest::reddit::RedditIngestOptions,
) -> Result<IngestResult, Box<dyn Error>> {
    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("ingesting reddit target: {target}"),
        },
    )
    .await;

    let reporter = PhaseReporter::new(progress_tx);
    let summary = ingest::reddit::ingest_reddit_with_options(cfg, target, &reporter, options)
        .await
        .map_err(|e| -> Box<dyn Error> {
            format!("reddit ingest failed for {target}: {e}").into()
        })?;

    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("reddit ingest complete: {} chunks", summary.chunks_embedded),
        },
    )
    .await;

    let payload = serde_json::json!({
        "source": "reddit",
        "target": target,
        "chunks": summary.chunks_embedded,
        "reddit_stats": {
            "posts_seen": summary.stats.posts_seen,
            "posts_prepared": summary.stats.posts_prepared,
            "comment_fetch_attempts": summary.stats.comment_fetch_attempts,
            "comment_fetch_failures": summary.stats.comment_fetch_failures,
            "partial_comment_failures": summary.stats.has_partial_comment_failures(),
        },
    });
    Ok(map_ingest_result(payload))
}

/// Ingest YouTube content into the vector store.
///
/// `url` may be a single video URL, a bare video ID, a playlist URL
/// (`youtube.com/playlist?list=...`), or a channel URL (`/@handle`, `/c/`, `/channel/`).
#[must_use = "ingest_youtube returns a Result that should be handled"]
pub async fn ingest_youtube(
    cfg: &Config,
    url: &str,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<IngestResult, Box<dyn Error>> {
    ingest_youtube_with_progress(cfg, url, tx, None).await
}

/// Ingest YouTube content with an optional structured progress sink.
#[must_use = "ingest_youtube_with_progress returns a Result that should be handled"]
pub async fn ingest_youtube_with_progress(
    cfg: &Config,
    url: &str,
    tx: Option<mpsc::Sender<ServiceEvent>>,
    progress_tx: Option<mpsc::Sender<serde_json::Value>>,
) -> Result<IngestResult, Box<dyn Error>> {
    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("ingesting youtube: {url}"),
        },
    )
    .await;

    let reporter = PhaseReporter::new(progress_tx);
    let chunks = ingest::youtube::ingest_youtube_target(cfg, url, &reporter)
        .await
        .map_err(|e| -> Box<dyn Error> {
            format!("youtube ingest failed for {url}: {e}").into()
        })?;

    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("youtube ingest complete: {chunks} chunks"),
        },
    )
    .await;

    let payload = serde_json::json!({
        "source": "youtube",
        "url": url,
        "chunks": chunks,
    });
    Ok(map_ingest_result(payload))
}

/// Ingest AI session exports (Claude/Codex/Gemini) into the vector store.
///
/// Session sources and paths are read from cfg (sessions_claude, sessions_codex,
/// sessions_gemini, sessions_project).
#[must_use = "ingest_sessions returns a Result that should be handled"]
pub async fn ingest_sessions(
    cfg: &Config,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<IngestResult, Box<dyn Error>> {
    ingest_sessions_with_progress(cfg, tx, None).await
}

/// Ingest AI session exports with an optional structured progress sink.
#[must_use = "ingest_sessions_with_progress returns a Result that should be handled"]
pub async fn ingest_sessions_with_progress(
    cfg: &Config,
    tx: Option<mpsc::Sender<ServiceEvent>>,
    progress_tx: Option<mpsc::Sender<serde_json::Value>>,
) -> Result<IngestResult, Box<dyn Error>> {
    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: "ingesting session exports".to_string(),
        },
    )
    .await;

    let reporter = PhaseReporter::new(progress_tx);
    let chunks = ingest::sessions::ingest_sessions(cfg, &reporter)
        .await
        .map_err(|e| -> Box<dyn Error> { format!("session exports ingest failed: {e}").into() })?;

    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("sessions ingest complete: {chunks} chunks"),
        },
    )
    .await;

    let payload = serde_json::json!({
        "source": "sessions",
        "chunks": chunks,
    });
    Ok(map_ingest_result(payload))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::core::config::Config;
    use crate::crates::jobs::backend::{BackendResult, JobKind, JobPayload};
    use crate::crates::jobs::lite::config_snapshot::decode_ingest_job_config;
    use crate::crates::services::context::ServiceContext;
    use crate::crates::services::runtime::ServiceJobRuntime;
    use crate::crates::services::types::{ExecutionMode, StartDisposition};
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};
    use uuid::Uuid;

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
            panic!("--wait false ingest start must enqueue without waiting")
        }

        async fn job_errors(&self, _id: Uuid, _kind: JobKind) -> BackendResult<Option<String>> {
            Ok(None)
        }

        async fn has_active_jobs(&self, _kind: JobKind) -> BackendResult<bool> {
            panic!("--wait false ingest start must not drain the queue")
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
    async fn ingest_start_with_context_enqueues_sessions_jobs_in_lite_mode() {
        let mut cfg = Config::test_default();
        cfg.lite_mode = true;
        cfg.sessions_claude = true;
        cfg.sessions_codex = false;
        cfg.sessions_gemini = true;
        cfg.sessions_project = Some("axon-rust".to_string());

        let runtime = Arc::new(CaptureRuntime {
            payloads: Mutex::new(Vec::new()),
        });
        let service_context = ServiceContext::from_runtime(Arc::new(cfg.clone()), runtime.clone());
        let source = IngestSource::Sessions {
            sessions_claude: true,
            sessions_codex: false,
            sessions_gemini: true,
            sessions_project: Some("axon-rust".to_string()),
        };

        let outcome = ingest_start_with_context(&cfg, source.clone(), &service_context)
            .await
            .expect("enqueue sessions");

        assert_eq!(outcome.disposition, StartDisposition::Enqueued);
        assert_eq!(outcome.execution_mode, ExecutionMode::InProcess);

        let payloads = runtime.payloads.lock().expect("lock");
        assert_eq!(payloads.len(), 1);
        let JobPayload::Ingest {
            target,
            source_type,
            config_json,
        } = &payloads[0]
        else {
            panic!("expected ingest payload");
        };

        assert_eq!(source_type, "sessions");
        assert_eq!(target, "claude,gemini:axon-rust");
        let (decoded, effective_cfg) =
            decode_ingest_job_config(&cfg, config_json).expect("decode source config");
        assert!(matches!(
            decoded,
            IngestSource::Sessions {
                sessions_claude: true,
                sessions_codex: false,
                sessions_gemini: true,
                sessions_project: Some(ref project),
            } if project == "axon-rust"
        ));
        assert_eq!(effective_cfg.collection, cfg.collection);
        assert!(effective_cfg.sessions_claude);
        assert!(!effective_cfg.sessions_codex);
        assert!(effective_cfg.sessions_gemini);
        assert_eq!(effective_cfg.sessions_project.as_deref(), Some("axon-rust"));
    }
}
