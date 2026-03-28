//! Full-stack backend: Postgres persistence + RabbitMQ dispatch.
//!
//! Workers remain as separate processes — unchanged from before.
//! This adapter wraps the existing per-job-type enqueue and query functions
//! so callers can use the uniform `JobBackend` interface without knowing
//! which storage / transport layer is in use.

use std::sync::Arc;

use async_trait::async_trait;

use crate::crates::core::config::Config;
use crate::crates::jobs::backend::{
    BackendResult, JobBackend, JobId, JobKind, JobPayload, JobStatusRow, JobSummary,
};
use crate::crates::jobs::status::JobStatus;

// The existing job functions return `Box<dyn std::error::Error>` (without
// Send + Sync bounds). `BackendResult` requires `Send + Sync`. Convert by
// stringifying the error — the information is preserved and the trait bounds
// are satisfied.
fn lift_err<E: std::fmt::Display>(e: E) -> Box<dyn std::error::Error + Send + Sync> {
    e.to_string().into()
}

/// Delegates all job operations to the existing Postgres + RabbitMQ functions.
pub struct FullBackend {
    cfg: Arc<Config>,
}

impl FullBackend {
    pub fn new(cfg: Arc<Config>) -> Self {
        Self { cfg }
    }
}

// ---------------------------------------------------------------------------
// Helper: convert raw status string → JobStatus
// ---------------------------------------------------------------------------

fn parse_status(s: &str) -> JobStatus {
    JobStatus::from_str(s)
}

#[async_trait]
impl JobBackend for FullBackend {
    // -----------------------------------------------------------------------
    // enqueue
    // -----------------------------------------------------------------------

    async fn enqueue(&self, payload: JobPayload) -> BackendResult<JobId> {
        let cfg = &*self.cfg;
        match payload {
            JobPayload::Crawl { url, .. } => {
                let id = crate::crates::jobs::crawl::start_crawl_job(cfg, &url)
                    .await
                    .map_err(lift_err)?;
                Ok(id)
            }
            JobPayload::Embed { input, .. } => {
                let id = crate::crates::jobs::embed::start_embed_job(cfg, &input, None)
                    .await
                    .map_err(lift_err)?;
                Ok(id)
            }
            JobPayload::Extract { urls, .. } => {
                let id = crate::crates::jobs::extract::start_extract_job(cfg, &urls, None)
                    .await
                    .map_err(lift_err)?;
                Ok(id)
            }
            JobPayload::Ingest {
                target,
                source_type,
                ..
            } => {
                use crate::crates::jobs::ingest::IngestSource;
                // Classify by source_type label. Unrecognised types are an error.
                let source = match source_type.as_str() {
                    "github" => IngestSource::Github {
                        repo: target.clone(),
                        include_source: true,
                    },
                    "reddit" => IngestSource::Reddit { target },
                    "youtube" => IngestSource::Youtube { target },
                    other => {
                        return Err(
                            format!("FullBackend: unknown ingest source_type '{other}'").into()
                        );
                    }
                };
                let id = crate::crates::jobs::ingest::start_ingest_job(cfg, source)
                    .await
                    .map_err(lift_err)?;
                Ok(id)
            }
            JobPayload::Refresh { url, .. } => {
                let id = crate::crates::jobs::refresh::start_refresh_job(cfg, &[url])
                    .await
                    .map_err(lift_err)?;
                Ok(id)
            }
            JobPayload::Graph { config_json } => {
                let (url, source_type) =
                    crate::crates::jobs::graph::parse_graph_config(&config_json)?;
                let pool = crate::crates::jobs::common::make_pool(cfg)
                    .await
                    .map_err(lift_err)?;
                let id =
                    crate::crates::jobs::graph::enqueue_graph_job(&pool, cfg, &url, &source_type)
                        .await
                        .map_err(lift_err)?;
                Ok(id)
            }
        }
    }

    // -----------------------------------------------------------------------
    // job_status
    // -----------------------------------------------------------------------

    async fn job_status(&self, id: JobId, kind: JobKind) -> BackendResult<Option<JobStatusRow>> {
        let cfg = &*self.cfg;
        match kind {
            JobKind::Crawl => {
                let opt = crate::crates::jobs::crawl::get_job(cfg, id)
                    .await
                    .map_err(lift_err)?;
                let Some(job) = opt else {
                    return Ok(None);
                };
                Ok(Some(JobStatusRow {
                    id: job.id,
                    status: parse_status(&job.status),
                    created_at: job.created_at,
                    updated_at: job.updated_at,
                    started_at: job.started_at,
                    finished_at: job.finished_at,
                    error_text: job.error_text,
                    result_json: job.result_json,
                }))
            }
            JobKind::Embed => {
                let opt = crate::crates::jobs::embed::get_embed_job(cfg, id)
                    .await
                    .map_err(lift_err)?;
                let Some(job) = opt else {
                    return Ok(None);
                };
                Ok(Some(JobStatusRow {
                    id: job.id,
                    status: parse_status(&job.status),
                    created_at: job.created_at,
                    updated_at: job.updated_at,
                    started_at: job.started_at,
                    finished_at: job.finished_at,
                    error_text: job.error_text,
                    result_json: job.result_json,
                }))
            }
            JobKind::Extract => {
                let opt = crate::crates::jobs::extract::get_extract_job(cfg, id)
                    .await
                    .map_err(lift_err)?;
                let Some(job) = opt else {
                    return Ok(None);
                };
                Ok(Some(JobStatusRow {
                    id: job.id,
                    status: parse_status(&job.status),
                    created_at: job.created_at,
                    updated_at: job.updated_at,
                    started_at: job.started_at,
                    finished_at: job.finished_at,
                    error_text: job.error_text,
                    result_json: job.result_json,
                }))
            }
            JobKind::Ingest => {
                let opt = crate::crates::jobs::ingest::get_ingest_job(cfg, id)
                    .await
                    .map_err(lift_err)?;
                let Some(job) = opt else {
                    return Ok(None);
                };
                Ok(Some(JobStatusRow {
                    id: job.id,
                    status: parse_status(&job.status),
                    created_at: job.created_at,
                    updated_at: job.updated_at,
                    started_at: job.started_at,
                    finished_at: job.finished_at,
                    error_text: job.error_text,
                    result_json: job.result_json,
                }))
            }
            JobKind::Refresh => {
                let opt = crate::crates::jobs::refresh::get_refresh_job(cfg, id)
                    .await
                    .map_err(lift_err)?;
                let Some(job) = opt else {
                    return Ok(None);
                };
                Ok(Some(JobStatusRow {
                    id: job.id,
                    status: parse_status(&job.status),
                    created_at: job.created_at,
                    updated_at: job.updated_at,
                    started_at: job.started_at,
                    finished_at: job.finished_at,
                    error_text: job.error_text,
                    result_json: job.result_json,
                }))
            }
            JobKind::Graph => {
                let opt = crate::crates::jobs::graph::get_graph_job(cfg, id)
                    .await
                    .map_err(lift_err)?;
                let Some(job) = opt else {
                    return Ok(None);
                };
                Ok(Some(JobStatusRow {
                    id: job.id,
                    status: parse_status(&job.status),
                    created_at: job.created_at,
                    updated_at: job.updated_at,
                    started_at: job.started_at,
                    finished_at: job.finished_at,
                    error_text: job.error_text,
                    result_json: None,
                }))
            }
        }
    }

    // -----------------------------------------------------------------------
    // cancel_job
    // -----------------------------------------------------------------------

    async fn cancel_job(&self, id: JobId, kind: JobKind) -> BackendResult<bool> {
        let cfg = &*self.cfg;
        match kind {
            JobKind::Crawl => Ok(crate::crates::jobs::crawl::cancel_job(cfg, id)
                .await
                .map_err(lift_err)?),
            JobKind::Embed => Ok(crate::crates::jobs::embed::cancel_embed_job(cfg, id)
                .await
                .map_err(lift_err)?),
            JobKind::Extract => Ok(crate::crates::jobs::extract::cancel_extract_job(cfg, id)
                .await
                .map_err(lift_err)?),
            JobKind::Ingest => Ok(crate::crates::jobs::ingest::cancel_ingest_job(cfg, id)
                .await
                .map_err(lift_err)?),
            JobKind::Refresh => Ok(crate::crates::jobs::refresh::cancel_refresh_job(cfg, id)
                .await
                .map_err(lift_err)?),
            JobKind::Graph => Ok(crate::crates::jobs::graph::cancel_graph_job(cfg, id)
                .await
                .map_err(lift_err)?),
        }
    }

    // -----------------------------------------------------------------------
    // list_jobs
    // -----------------------------------------------------------------------

    async fn list_jobs(&self, kind: JobKind) -> BackendResult<Vec<JobSummary>> {
        let cfg = &*self.cfg;
        match kind {
            JobKind::Crawl => {
                let rows = crate::crates::jobs::crawl::list_jobs(cfg, 500, 0)
                    .await
                    .map_err(lift_err)?;
                Ok(rows
                    .into_iter()
                    .map(|j| JobSummary {
                        id: j.id,
                        status: parse_status(&j.status),
                        created_at: j.created_at,
                        target: j.url,
                    })
                    .collect())
            }
            JobKind::Embed => {
                let rows = crate::crates::jobs::embed::list_embed_jobs(cfg, 500, 0)
                    .await
                    .map_err(lift_err)?;
                Ok(rows
                    .into_iter()
                    .map(|j| JobSummary {
                        id: j.id,
                        status: parse_status(&j.status),
                        created_at: j.created_at,
                        target: j.input_text,
                    })
                    .collect())
            }
            JobKind::Extract => {
                let rows = crate::crates::jobs::extract::list_extract_jobs(cfg, 500, 0)
                    .await
                    .map_err(lift_err)?;
                Ok(rows
                    .into_iter()
                    .map(|j| JobSummary {
                        id: j.id,
                        status: parse_status(&j.status),
                        created_at: j.created_at,
                        target: j.urls_json.to_string(),
                    })
                    .collect())
            }
            JobKind::Ingest => {
                let rows = crate::crates::jobs::ingest::list_ingest_jobs(cfg, None, 500, 0)
                    .await
                    .map_err(lift_err)?;
                Ok(rows
                    .into_iter()
                    .map(|j| JobSummary {
                        id: j.id,
                        status: parse_status(&j.status),
                        created_at: j.created_at,
                        target: j.target,
                    })
                    .collect())
            }
            JobKind::Refresh => {
                let rows = crate::crates::jobs::refresh::list_refresh_jobs(cfg, 500, 0)
                    .await
                    .map_err(lift_err)?;
                Ok(rows
                    .into_iter()
                    .map(|j| JobSummary {
                        id: j.id,
                        status: parse_status(&j.status),
                        created_at: j.created_at,
                        target: j.urls_json.to_string(),
                    })
                    .collect())
            }
            JobKind::Graph => {
                let rows = crate::crates::jobs::graph::list_graph_jobs(cfg, 500, 0)
                    .await
                    .map_err(lift_err)?;
                Ok(rows
                    .into_iter()
                    .map(|j| JobSummary {
                        id: j.id,
                        status: parse_status(&j.status),
                        created_at: j.created_at,
                        target: j.url,
                    })
                    .collect())
            }
        }
    }

    // -----------------------------------------------------------------------
    // cleanup_jobs
    // -----------------------------------------------------------------------

    async fn cleanup_jobs(&self, kind: JobKind) -> BackendResult<u64> {
        let cfg = &*self.cfg;
        match kind {
            JobKind::Crawl => Ok(crate::crates::jobs::crawl::cleanup_jobs(cfg)
                .await
                .map_err(lift_err)?),
            JobKind::Embed => Ok(crate::crates::jobs::embed::cleanup_embed_jobs(cfg)
                .await
                .map_err(lift_err)?),
            JobKind::Extract => Ok(crate::crates::jobs::extract::cleanup_extract_jobs(cfg)
                .await
                .map_err(lift_err)?),
            JobKind::Ingest => Ok(crate::crates::jobs::ingest::cleanup_ingest_jobs(cfg)
                .await
                .map_err(lift_err)?),
            JobKind::Refresh => Ok(crate::crates::jobs::refresh::cleanup_refresh_jobs(cfg)
                .await
                .map_err(lift_err)?),
            JobKind::Graph => Ok(crate::crates::jobs::graph::cleanup_graph_jobs(cfg)
                .await
                .map_err(lift_err)?),
        }
    }

    // -----------------------------------------------------------------------
    // clear_jobs
    // -----------------------------------------------------------------------

    async fn clear_jobs(&self, kind: JobKind) -> BackendResult<u64> {
        let cfg = &*self.cfg;
        match kind {
            JobKind::Crawl => Ok(crate::crates::jobs::crawl::clear_jobs(cfg)
                .await
                .map_err(lift_err)?),
            JobKind::Embed => Ok(crate::crates::jobs::embed::clear_embed_jobs(cfg)
                .await
                .map_err(lift_err)?),
            JobKind::Extract => Ok(crate::crates::jobs::extract::clear_extract_jobs(cfg)
                .await
                .map_err(lift_err)?),
            JobKind::Ingest => Ok(crate::crates::jobs::ingest::clear_ingest_jobs(cfg)
                .await
                .map_err(lift_err)?),
            JobKind::Refresh => Ok(crate::crates::jobs::refresh::clear_refresh_jobs(cfg)
                .await
                .map_err(lift_err)?),
            JobKind::Graph => Ok(crate::crates::jobs::graph::clear_graph_jobs(cfg)
                .await
                .map_err(lift_err)?),
        }
    }

    // -----------------------------------------------------------------------
    // job_errors
    // -----------------------------------------------------------------------

    async fn job_errors(&self, id: JobId, kind: JobKind) -> BackendResult<Option<String>> {
        // Delegate to job_status and extract error_text.
        let row = self.job_status(id, kind).await?;
        Ok(row.and_then(|r| r.error_text))
    }
}
