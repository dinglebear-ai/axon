use async_trait::async_trait;
use axon_api::source::*;

use super::{FakeJobWatchStore, capability, missing_job, missing_watch};
use crate::boundary::{Result, WatchStore};
use crate::limits::clamp_page_limit;

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
        if let Some(scope) = request.scope {
            watch.scope = scope;
        }
        Ok(watch.clone())
    }

    async fn get(&self, watch_id: WatchId) -> Result<Option<WatchResult>> {
        Ok(self.state.lock().await.watches.get(&watch_id).cloned())
    }

    async fn list(&self, request: WatchListRequest) -> Result<Page<WatchSummary>> {
        if request.cursor.is_some() {
            return Err(ApiError::new(
                "watch.cursor_unsupported",
                ErrorStage::Retrieving,
                "fake watch store does not implement cursor pagination",
            ));
        }
        let state = self.state.lock().await;
        let mut items = state
            .watches
            .values()
            .map(|watch| WatchSummary {
                watch_id: watch.watch_id.clone(),
                source_id: watch.source_id.clone(),
                enabled: watch.enabled,
                schedule: watch.schedule.clone(),
                next_run_at: state.peek_timestamp(),
                last_job_id: watch.latest_job.as_ref().map(|job| job.job_id),
                last_status: watch.latest_job.as_ref().map(|job| job.status),
            })
            .collect::<Vec<_>>();
        if let Some(enabled) = request.enabled {
            items.retain(|watch| watch.enabled == enabled);
        }
        if let Some(source_id) = request.source_id {
            items.retain(|watch| watch.source_id == source_id);
        }
        if let Some(adapter) = request.adapter {
            items.retain(|watch| {
                state
                    .watches
                    .get(&watch.watch_id)
                    .map(|detail| detail.adapter.name == adapter)
                    .unwrap_or(false)
            });
        }
        let total = items.len() as u64;
        let limit = clamp_page_limit(request.limit);
        items.truncate(limit as usize);
        Ok(Page {
            total: Some(total),
            limit,
            next_cursor: None,
            items,
        })
    }

    async fn record_run(&self, watch_id: WatchId, job_id: JobId) -> Result<()> {
        let mut state = self.state.lock().await;
        if !state.watches.contains_key(&watch_id) {
            return Err(missing_watch(&watch_id));
        }
        if !state.jobs.contains_key(&job_id) {
            return Err(missing_job(job_id));
        }
        let source_id = state
            .watches
            .get(&watch_id)
            .map(|watch| watch.source_id.clone());
        if let Some(job) = state.jobs.get_mut(&job_id) {
            job.watch_id = Some(watch_id.clone());
            job.source_id = source_id;
        }
        let latest_job = state.jobs.get(&job_id).map(|job| JobDescriptor {
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
        });
        if let Some(watch) = state.watches.get_mut(&watch_id) {
            watch.latest_job = latest_job;
        }
        state.watch_runs.entry(watch_id).or_default().push(job_id);
        Ok(())
    }

    async fn history(&self, request: WatchHistoryRequest) -> Result<WatchHistoryResult> {
        let state = self.state.lock().await;
        if !state.watches.contains_key(&request.watch_id) {
            return Err(missing_watch(&request.watch_id));
        }
        let runs = state
            .watch_runs
            .get(&request.watch_id)
            .into_iter()
            .flat_map(|ids| ids.iter())
            .rev()
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
            .take(clamp_page_limit(request.limit) as usize)
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
        state.next_watch = 0;
        Ok(())
    }

    async fn capabilities(&self) -> Result<WatchStoreCapability> {
        Ok(capability("fake-watch-store").into())
    }
}
