use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use axon_api::source::*;
use tokio::sync::Mutex;
use uuid::Uuid;

pub type Result<T> = std::result::Result<T, ApiError>;

#[async_trait]
pub trait JobStore: Send + Sync {
    async fn create(&self, request: JobCreateRequest) -> Result<JobDescriptor>;
    async fn get(&self, job_id: JobId) -> Result<Option<JobSummary>>;
    async fn update_status(&self, status: JobStatusUpdate) -> Result<()>;
    async fn append_event(&self, event: SourceProgressEvent) -> Result<()>;
    async fn heartbeat(&self, heartbeat: JobHeartbeat) -> Result<()>;
    async fn list(&self, request: JobListRequest) -> Result<Page<JobSummary>>;
    async fn events(&self, request: JobEventListRequest) -> Result<JobEventPage>;
    async fn reset(&self) -> Result<()>;
    async fn capabilities(&self) -> Result<JobStoreCapability>;
}

#[async_trait]
pub trait WatchStore: Send + Sync {
    async fn create(&self, request: WatchRequest) -> Result<WatchResult>;
    async fn update(&self, watch_id: WatchId, request: WatchUpdateRequest) -> Result<WatchResult>;
    async fn get(&self, watch_id: WatchId) -> Result<Option<WatchResult>>;
    async fn list(&self, request: WatchListRequest) -> Result<Page<WatchSummary>>;
    async fn record_run(&self, watch_id: WatchId, job_id: JobId) -> Result<()>;
    async fn history(&self, request: WatchHistoryRequest) -> Result<WatchHistoryResult>;
    async fn reset(&self) -> Result<()>;
    async fn capabilities(&self) -> Result<WatchStoreCapability>;
}

#[derive(Debug, Clone, Default)]
pub struct FakeJobWatchStore {
    state: Arc<Mutex<FakeJobWatchState>>,
}

#[derive(Debug, Default)]
struct FakeJobWatchState {
    jobs: BTreeMap<JobId, JobSummary>,
    events: BTreeMap<JobId, Vec<JobEvent>>,
    watches: BTreeMap<WatchId, WatchResult>,
    watch_runs: BTreeMap<WatchId, Vec<JobId>>,
    next_job: u128,
    next_watch: u64,
}

impl FakeJobWatchStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl JobStore for FakeJobWatchStore {
    async fn create(&self, request: JobCreateRequest) -> Result<JobDescriptor> {
        let mut state = self.state.lock().await;
        state.next_job += 1;
        let job_id = JobId::new(Uuid::from_u128(state.next_job));
        let summary = JobSummary {
            job_id,
            kind: request.kind,
            status: LifecycleStatus::Queued,
            phase: PipelinePhase::Queued,
            created_at: timestamp(),
            updated_at: timestamp(),
            source_id: None,
            watch_id: None,
            counts: None,
            last_error: None,
        };
        state.jobs.insert(job_id, summary);
        Ok(JobDescriptor {
            job_id,
            kind: request.kind,
            status: LifecycleStatus::Queued,
            poll: PollDescriptor {
                status_url: format!("/v1/jobs/{job_id}", job_id = job_id.0),
                events_url: Some(format!("/v1/jobs/{job_id}/events", job_id = job_id.0)),
                suggested_interval_ms: 1000,
            },
            created_at: timestamp(),
            updated_at: timestamp(),
        })
    }

    async fn get(&self, job_id: JobId) -> Result<Option<JobSummary>> {
        Ok(self.state.lock().await.jobs.get(&job_id).cloned())
    }

    async fn update_status(&self, status: JobStatusUpdate) -> Result<()> {
        if let Some(job) = self.state.lock().await.jobs.get_mut(&status.job_id) {
            job.status = status.status;
            job.phase = status.phase;
            job.last_error = status.error;
            job.updated_at = timestamp();
        }
        Ok(())
    }

    async fn append_event(&self, event: SourceProgressEvent) -> Result<()> {
        self.state
            .lock()
            .await
            .events
            .entry(event.job_id)
            .or_default()
            .push(JobEvent {
                event_id: event.event_id,
                sequence: event.sequence,
                job_id: event.job_id,
                phase: event.phase,
                status: event.status,
                severity: event.severity,
                message: event.message,
                timestamp: event.timestamp,
                details: MetadataMap::new(),
            });
        Ok(())
    }

    async fn heartbeat(&self, heartbeat: JobHeartbeat) -> Result<()> {
        if let Some(job) = self.state.lock().await.jobs.get_mut(&heartbeat.job_id) {
            job.phase = heartbeat.phase;
            job.updated_at = heartbeat.timestamp;
        }
        Ok(())
    }

    async fn list(&self, request: JobListRequest) -> Result<Page<JobSummary>> {
        let mut items = self
            .state
            .lock()
            .await
            .jobs
            .values()
            .cloned()
            .collect::<Vec<_>>();
        if let Some(status) = request.status {
            items.retain(|job| job.status == status);
        }
        Ok(Page {
            total: Some(items.len() as u64),
            limit: request.limit.unwrap_or(100),
            next_cursor: None,
            items,
        })
    }

    async fn events(&self, request: JobEventListRequest) -> Result<JobEventPage> {
        let items = self
            .state
            .lock()
            .await
            .events
            .get(&request.job_id)
            .cloned()
            .unwrap_or_default();
        Ok(Page {
            total: Some(items.len() as u64),
            limit: request.limit.unwrap_or(100),
            next_cursor: None,
            items,
        })
    }

    async fn reset(&self) -> Result<()> {
        let mut state = self.state.lock().await;
        state.jobs.clear();
        state.events.clear();
        state.watch_runs.clear();
        Ok(())
    }

    async fn capabilities(&self) -> Result<JobStoreCapability> {
        Ok(capability("fake-job-store").into())
    }
}

#[async_trait]
impl WatchStore for FakeJobWatchStore {
    async fn create(&self, request: WatchRequest) -> Result<WatchResult> {
        let mut state = self.state.lock().await;
        state.next_watch += 1;
        let watch_id = WatchId::new(format!("watch_{}", state.next_watch));
        let result = WatchResult {
            watch_id: watch_id.clone(),
            source_id: SourceId::new(format!("source_{}", state.next_watch)),
            canonical_uri: request.source,
            adapter: AdapterRef {
                name: "fake".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
            scope: request.scope.unwrap_or(SourceScope::Page),
            enabled: request.enabled.unwrap_or(true),
            schedule: request.schedule,
            job: None,
            latest_job: None,
            warnings: Vec::new(),
        };
        state.watches.insert(watch_id, result.clone());
        Ok(result)
    }

    async fn update(&self, watch_id: WatchId, request: WatchUpdateRequest) -> Result<WatchResult> {
        let mut state = self.state.lock().await;
        let watch = state.watches.get_mut(&watch_id).ok_or_else(|| {
            ApiError::new("watch.not_found", ErrorStage::Retrieving, "watch not found")
        })?;
        if let Some(schedule) = request.schedule {
            watch.schedule = schedule;
        }
        if let Some(enabled) = request.enabled {
            watch.enabled = enabled;
        }
        Ok(watch.clone())
    }

    async fn get(&self, watch_id: WatchId) -> Result<Option<WatchResult>> {
        Ok(self.state.lock().await.watches.get(&watch_id).cloned())
    }

    async fn list(&self, request: WatchListRequest) -> Result<Page<WatchSummary>> {
        let mut items = self
            .state
            .lock()
            .await
            .watches
            .values()
            .map(|watch| WatchSummary {
                watch_id: watch.watch_id.clone(),
                source_id: watch.source_id.clone(),
                enabled: watch.enabled,
                schedule: watch.schedule.clone(),
                next_run_at: timestamp(),
                last_job_id: None,
                last_status: None,
            })
            .collect::<Vec<_>>();
        if let Some(enabled) = request.enabled {
            items.retain(|watch| watch.enabled == enabled);
        }
        Ok(Page {
            total: Some(items.len() as u64),
            limit: request.limit.unwrap_or(100),
            next_cursor: None,
            items,
        })
    }

    async fn record_run(&self, watch_id: WatchId, job_id: JobId) -> Result<()> {
        self.state
            .lock()
            .await
            .watch_runs
            .entry(watch_id)
            .or_default()
            .push(job_id);
        Ok(())
    }

    async fn history(&self, request: WatchHistoryRequest) -> Result<WatchHistoryResult> {
        let state = self.state.lock().await;
        let runs = state
            .watch_runs
            .get(&request.watch_id)
            .into_iter()
            .flat_map(|ids| ids.iter())
            .filter_map(|job_id| state.jobs.get(job_id))
            .map(|job| JobDescriptor {
                job_id: job.job_id,
                kind: job.kind,
                status: job.status,
                poll: PollDescriptor {
                    status_url: format!("/v1/jobs/{job_id}", job_id = job.job_id.0),
                    events_url: None,
                    suggested_interval_ms: 1000,
                },
                created_at: job.created_at.clone(),
                updated_at: job.updated_at.clone(),
            })
            .collect();
        Ok(WatchHistoryResult {
            runs,
            warnings: Vec::new(),
        })
    }

    async fn reset(&self) -> Result<()> {
        let mut state = self.state.lock().await;
        state.watches.clear();
        state.watch_runs.clear();
        Ok(())
    }

    async fn capabilities(&self) -> Result<WatchStoreCapability> {
        Ok(capability("fake-watch-store").into())
    }
}

fn capability(name: &str) -> CapabilityBase {
    CapabilityBase {
        name: name.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        owner_crate: "axon-jobs".to_string(),
        health: HealthStatus::Healthy,
        features: vec!["fake".to_string()],
        limits: MetadataMap::new(),
    }
}

fn timestamp() -> Timestamp {
    Timestamp("2026-07-01T00:00:00Z".to_string())
}

#[cfg(test)]
#[path = "boundary_tests.rs"]
mod tests;
