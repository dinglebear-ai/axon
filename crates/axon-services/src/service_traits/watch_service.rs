//! `WatchService` — source-backed recurring watch definitions
//! (create/update/get/list/exec/pause/resume/delete/history).
//!
//! Contract: `docs/pipeline-unification/foundation/types/service-contract.md`
//! §WatchService. The production implementation is backed by
//! `SqliteWatchStore` (`axon_source_watches` / `axon_source_watch_runs`) and
//! manual `exec` enqueues a detached `JobKind::Source` job through the unified
//! source pipeline.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axon_api::source::{
    DeleteResult, JobDescriptor, JobId, JobKind, LifecycleStatus, Page, WatchExecRequest,
    WatchHistoryRequest, WatchHistoryResult, WatchId, WatchListRequest, WatchRequest, WatchResult,
    WatchSummary, WatchUpdateRequest,
};

use crate::context::ServiceContext;
use crate::watch::{self, SourceWatchStoreTrait};

#[async_trait]
pub trait WatchService: Send + Sync {
    async fn create(&self, request: WatchRequest) -> anyhow::Result<WatchResult>;
    async fn update(
        &self,
        watch_id: WatchId,
        request: WatchUpdateRequest,
    ) -> anyhow::Result<WatchResult>;
    async fn get(&self, watch_id: WatchId) -> anyhow::Result<WatchResult>;
    async fn list(&self, request: WatchListRequest) -> anyhow::Result<Page<WatchSummary>>;
    async fn exec(
        &self,
        watch_id: WatchId,
        request: WatchExecRequest,
    ) -> anyhow::Result<JobDescriptor>;
    async fn pause(&self, watch_id: WatchId) -> anyhow::Result<WatchResult>;
    async fn resume(&self, watch_id: WatchId) -> anyhow::Result<WatchResult>;
    async fn delete(&self, watch_id: WatchId) -> anyhow::Result<axon_api::source::DeleteResult>;
    async fn history(&self, request: WatchHistoryRequest) -> anyhow::Result<WatchHistoryResult>;
}

pub struct WatchServiceImpl {
    ctx: Arc<ServiceContext>,
}

impl WatchServiceImpl {
    pub fn new(ctx: Arc<ServiceContext>) -> Self {
        Self { ctx }
    }
}

#[async_trait]
impl WatchService for WatchServiceImpl {
    async fn create(&self, request: WatchRequest) -> anyhow::Result<WatchResult> {
        let pool = self.ctx.jobs.sqlite_pool();
        watch::create_source_watch(&self.ctx.cfg, pool.as_deref(), request, None)
            .await
            .map_err(|err| anyhow::anyhow!("{err}"))
    }

    async fn update(
        &self,
        watch_id: WatchId,
        request: WatchUpdateRequest,
    ) -> anyhow::Result<WatchResult> {
        let pool = self.ctx.jobs.sqlite_pool();
        let store = watch::open_source_watch_store(&self.ctx.cfg, pool.as_deref())
            .await
            .map_err(|err| anyhow::anyhow!("{err}"))?;
        SourceWatchStoreTrait::update(&store, watch_id, request)
            .await
            .map_err(|err| anyhow::anyhow!("{}", err.message))
    }

    async fn get(&self, watch_id: WatchId) -> anyhow::Result<WatchResult> {
        let pool = self.ctx.jobs.sqlite_pool();
        let store = watch::open_source_watch_store(&self.ctx.cfg, pool.as_deref())
            .await
            .map_err(|err| anyhow::anyhow!("{err}"))?;
        SourceWatchStoreTrait::get(&store, watch_id.clone())
            .await
            .map_err(|err| anyhow::anyhow!("{}", err.message))?
            .ok_or_else(|| anyhow::anyhow!("watch {} not found", watch_id.0))
    }

    async fn list(&self, request: WatchListRequest) -> anyhow::Result<Page<WatchSummary>> {
        let pool = self.ctx.jobs.sqlite_pool();
        let store = watch::open_source_watch_store(&self.ctx.cfg, pool.as_deref())
            .await
            .map_err(|err| anyhow::anyhow!("{err}"))?;
        SourceWatchStoreTrait::list(&store, request)
            .await
            .map_err(|err| anyhow::anyhow!("{}", err.message))
    }

    async fn exec(
        &self,
        watch_id: WatchId,
        request: WatchExecRequest,
    ) -> anyhow::Result<JobDescriptor> {
        let pool = self.ctx.jobs.sqlite_pool();
        watch::exec_source_watch(&self.ctx, pool.as_deref(), watch_id, request, None)
            .await
            .map_err(|err| anyhow::anyhow!("{err}"))
    }

    async fn pause(&self, watch_id: WatchId) -> anyhow::Result<WatchResult> {
        self.update(
            watch_id,
            WatchUpdateRequest {
                enabled: Some(false),
                schedule: None,
                options: None,
                embed: None,
                collection: None,
                scope: None,
            },
        )
        .await
    }

    async fn resume(&self, watch_id: WatchId) -> anyhow::Result<WatchResult> {
        self.update(
            watch_id,
            WatchUpdateRequest {
                enabled: Some(true),
                schedule: None,
                options: None,
                embed: None,
                collection: None,
                scope: None,
            },
        )
        .await
    }

    async fn delete(&self, watch_id: WatchId) -> anyhow::Result<DeleteResult> {
        let pool = self.ctx.jobs.sqlite_pool();
        let store = watch::open_source_watch_store(&self.ctx.cfg, pool.as_deref())
            .await
            .map_err(|err| anyhow::anyhow!("{err}"))?;
        let deleted = store
            .delete(watch_id.clone())
            .await
            .map_err(|err| anyhow::anyhow!("{}", err.message))?;
        Ok(DeleteResult {
            deleted,
            id: watch_id.0,
        })
    }

    async fn history(&self, request: WatchHistoryRequest) -> anyhow::Result<WatchHistoryResult> {
        let pool = self.ctx.jobs.sqlite_pool();
        watch::history_source_watch(&self.ctx.cfg, pool.as_deref(), request)
            .await
            .map_err(|err| anyhow::anyhow!("{err}"))
    }
}

fn fake_watch_result(watch_id: WatchId, request: &WatchRequest) -> WatchResult {
    WatchResult {
        watch_id,
        source_id: axon_api::source::SourceId::new(format!("fake:{}", request.source)),
        canonical_uri: request.source.clone(),
        adapter: axon_api::source::AdapterRef {
            name: "fake".to_string(),
            version: "0".to_string(),
        },
        scope: request.scope.unwrap_or(axon_api::source::SourceScope::Page),
        enabled: request.enabled.unwrap_or(true),
        schedule: request.schedule.clone(),
        job: None,
        latest_job: None,
        warnings: Vec::new(),
    }
}

/// Deterministic in-memory fake covering every `WatchService` method.
#[derive(Default)]
pub struct FakeWatchService {
    watches: Mutex<std::collections::HashMap<String, WatchResult>>,
}

impl FakeWatchService {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl WatchService for FakeWatchService {
    async fn create(&self, request: WatchRequest) -> anyhow::Result<WatchResult> {
        let watch_id = WatchId::new(format!("watch-{}", uuid::Uuid::new_v4()));
        let result = fake_watch_result(watch_id.clone(), &request);
        self.watches
            .lock()
            .unwrap()
            .insert(watch_id.0, result.clone());
        Ok(result)
    }

    async fn update(
        &self,
        watch_id: WatchId,
        request: WatchUpdateRequest,
    ) -> anyhow::Result<WatchResult> {
        let mut watches = self.watches.lock().unwrap();
        let watch = watches
            .get_mut(&watch_id.0)
            .ok_or_else(|| anyhow::anyhow!("watch {} not found", watch_id.0))?;
        if let Some(enabled) = request.enabled {
            watch.enabled = enabled;
        }
        if let Some(schedule) = request.schedule {
            watch.schedule = schedule;
        }
        Ok(watch.clone())
    }

    async fn get(&self, watch_id: WatchId) -> anyhow::Result<WatchResult> {
        self.watches
            .lock()
            .unwrap()
            .get(&watch_id.0)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("watch {} not found", watch_id.0))
    }

    async fn list(&self, request: WatchListRequest) -> anyhow::Result<Page<WatchSummary>> {
        let watches = self.watches.lock().unwrap();
        let limit = request.limit.unwrap_or(50);
        let items = watches
            .values()
            .take(limit as usize)
            .map(|w| WatchSummary {
                watch_id: w.watch_id.clone(),
                source_id: w.source_id.clone(),
                enabled: w.enabled,
                schedule: w.schedule.clone(),
                next_run_at: axon_api::source::Timestamp::from(chrono::Utc::now()),
                last_job_id: None,
                last_status: None,
            })
            .collect();
        Ok(Page {
            items,
            next_cursor: None,
            limit,
            total: Some(watches.len() as u64),
        })
    }

    async fn exec(
        &self,
        watch_id: WatchId,
        _request: WatchExecRequest,
    ) -> anyhow::Result<JobDescriptor> {
        if !self.watches.lock().unwrap().contains_key(&watch_id.0) {
            anyhow::bail!("watch {} not found", watch_id.0);
        }
        let job_id = JobId::new(uuid::Uuid::new_v4());
        Ok(JobDescriptor {
            kind: JobKind::Watch,
            id: job_id,
            status_url: format!("/v1/jobs/{}", job_id.0),
            events_url: format!("/v1/jobs/{}/events", job_id.0),
            stream_url: format!("/v1/jobs/{}/stream", job_id.0),
            poll_after_ms: 1_000,
            cancel_url: None,
            retry_url: None,
            job_id,
            status: LifecycleStatus::Queued,
            poll: None,
            created_at: None,
            updated_at: None,
        })
    }

    async fn pause(&self, watch_id: WatchId) -> anyhow::Result<WatchResult> {
        let mut watches = self.watches.lock().unwrap();
        let watch = watches
            .get_mut(&watch_id.0)
            .ok_or_else(|| anyhow::anyhow!("watch {} not found", watch_id.0))?;
        watch.enabled = false;
        Ok(watch.clone())
    }

    async fn resume(&self, watch_id: WatchId) -> anyhow::Result<WatchResult> {
        let mut watches = self.watches.lock().unwrap();
        let watch = watches
            .get_mut(&watch_id.0)
            .ok_or_else(|| anyhow::anyhow!("watch {} not found", watch_id.0))?;
        watch.enabled = true;
        Ok(watch.clone())
    }

    async fn delete(&self, watch_id: WatchId) -> anyhow::Result<axon_api::source::DeleteResult> {
        let removed = self.watches.lock().unwrap().remove(&watch_id.0).is_some();
        Ok(axon_api::source::DeleteResult {
            deleted: removed,
            id: watch_id.0,
        })
    }

    async fn history(&self, request: WatchHistoryRequest) -> anyhow::Result<WatchHistoryResult> {
        Ok(WatchHistoryResult {
            watch_id: request.watch_id,
            jobs: Vec::new(),
            next_cursor: None,
        })
    }
}

#[cfg(test)]
#[path = "watch_service_tests.rs"]
mod tests;
