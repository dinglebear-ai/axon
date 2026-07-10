//! `WatchService` — recurring watch definitions (create/update/get/list/exec/
//! pause/resume/delete/history).
//!
//! Contract: `docs/pipeline-unification/foundation/types/service-contract.md`
//! §WatchService. **Finding vs. the approved wiring plan:** the plan assumed
//! `watch.rs`'s `WatchDef`/`WatchDefCreate` free functions could thinly wrap
//! into the contract's `WatchRequest`/`WatchResult`/`WatchSummary` DTOs, but
//! `WatchDef` is a generic scheduler row (`id`, `name`, `task_type`,
//! `task_payload: serde_json::Value`, `every_seconds`, ...) with no
//! `canonical_uri`/`adapter`/`scope` fields the contract DTOs expect —
//! reconstructing those from `task_payload` would require knowing (and
//! committing to) the JSON shape the `watch` task-runner expects, which is
//! genuinely new orchestration, not a thin field-for-field wrap. So
//! `create`/`update`/`get`/`list` (all of which return a `WatchResult` or
//! `WatchSummary`) stay documented stubs
//! ([`crate::service_traits::not_implemented`]).
//!
//! `exec` and `history` are different: their return types (`JobDescriptor`,
//! `WatchHistoryResult { jobs: Vec<JobDescriptor>, .. }`) never reference
//! `canonical_uri`/`adapter`/`scope`, so they don't hit the blocker above and
//! *are* wired as real thin wraps — `exec` loads the `WatchDef` by id
//! (`watch::get_watch_def`) then runs it synchronously
//! (`watch::run_watch_now`), and `history` lists runs
//! (`watch::list_watch_runs`) — both mapping `WatchRun` into a synthesized
//! `JobDescriptor`. `pause`/`resume`/`delete` still have no backing free
//! function at all (CLAUDE.md documents them as "parse but are not yet
//! implemented"). Only the `Fake` implements full in-memory semantics for
//! every method.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axon_api::source::{
    JobDescriptor, JobId, JobKind, LifecycleStatus, Page, WatchExecRequest, WatchHistoryRequest,
    WatchHistoryResult, WatchId, WatchListRequest, WatchRequest, WatchResult, WatchSummary,
    WatchUpdateRequest,
};

use crate::context::ServiceContext;
use crate::service_traits::not_implemented;
use crate::watch;

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
    async fn create(&self, _request: WatchRequest) -> anyhow::Result<WatchResult> {
        Err(not_implemented("WatchService::create"))
    }

    async fn update(
        &self,
        _watch_id: WatchId,
        _request: WatchUpdateRequest,
    ) -> anyhow::Result<WatchResult> {
        Err(not_implemented("WatchService::update"))
    }

    async fn get(&self, _watch_id: WatchId) -> anyhow::Result<WatchResult> {
        Err(not_implemented("WatchService::get"))
    }

    async fn list(&self, _request: WatchListRequest) -> anyhow::Result<Page<WatchSummary>> {
        Err(not_implemented("WatchService::list"))
    }

    async fn exec(
        &self,
        watch_id: WatchId,
        _request: WatchExecRequest,
    ) -> anyhow::Result<JobDescriptor> {
        let uuid = parse_watch_uuid(&watch_id)?;
        let def = watch::get_watch_def(&self.ctx.cfg, uuid)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?
            .ok_or_else(|| anyhow::anyhow!("watch {} not found", watch_id.0))?;
        let run = watch::run_watch_now(&self.ctx.cfg, &def)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(watch_run_to_job_descriptor(&run))
    }

    async fn pause(&self, _watch_id: WatchId) -> anyhow::Result<WatchResult> {
        Err(not_implemented("WatchService::pause"))
    }

    async fn resume(&self, _watch_id: WatchId) -> anyhow::Result<WatchResult> {
        Err(not_implemented("WatchService::resume"))
    }

    async fn delete(&self, _watch_id: WatchId) -> anyhow::Result<axon_api::source::DeleteResult> {
        Err(not_implemented("WatchService::delete"))
    }

    async fn history(&self, request: WatchHistoryRequest) -> anyhow::Result<WatchHistoryResult> {
        let uuid = parse_watch_uuid(&request.watch_id)?;
        let limit = i64::from(request.limit.unwrap_or(50));
        let runs = watch::list_watch_runs(&self.ctx.cfg, uuid, limit)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(WatchHistoryResult {
            watch_id: request.watch_id,
            jobs: runs.iter().map(watch_run_to_job_descriptor).collect(),
            next_cursor: None,
        })
    }
}

/// Parse a `WatchId` (string-wrapped UUID) into the `Uuid` the `watch.rs`
/// free functions key rows by.
fn parse_watch_uuid(watch_id: &WatchId) -> anyhow::Result<uuid::Uuid> {
    uuid::Uuid::parse_str(&watch_id.0)
        .map_err(|e| anyhow::anyhow!("invalid watch id {}: {e}", watch_id.0))
}

/// Map a `WatchRun` scheduler row into a synthesized `JobDescriptor`. Neither
/// `JobDescriptor` nor the `WatchHistoryResult`/`exec` return types reference
/// `canonical_uri`/`adapter`/`scope`, so this mapping is a real thin wrap —
/// unlike `create`/`get`/`list`, which are blocked on those missing fields
/// (see module doc comment).
fn watch_run_to_job_descriptor(run: &watch::WatchRun) -> JobDescriptor {
    let job_id = JobId::new(run.dispatched_job_id.unwrap_or(run.id));
    let status = watch_run_status_to_lifecycle(&run.status);
    JobDescriptor {
        kind: JobKind::Watch,
        id: job_id,
        status_url: format!("/v1/jobs/{}", job_id.0),
        events_url: format!("/v1/jobs/{}/events", job_id.0),
        stream_url: format!("/v1/jobs/{}/stream", job_id.0),
        poll_after_ms: 1_000,
        cancel_url: None,
        retry_url: None,
        job_id,
        status,
        poll: None,
        created_at: Some(axon_api::source::Timestamp::from(run.created_at)),
        updated_at: Some(axon_api::source::Timestamp::from(run.updated_at)),
    }
}

fn watch_run_status_to_lifecycle(status: &str) -> LifecycleStatus {
    match status {
        axon_jobs::watch::WATCH_RUN_STATUS_RUNNING => LifecycleStatus::Running,
        axon_jobs::watch::WATCH_RUN_STATUS_COMPLETED => LifecycleStatus::Completed,
        axon_jobs::watch::WATCH_RUN_STATUS_FAILED => LifecycleStatus::Failed,
        _ => LifecycleStatus::Queued,
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
