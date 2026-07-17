use async_trait::async_trait;
use axon_api::source::*;

use super::helpers::descriptor;
use super::{FakeJobWatchStore, capability, missing_job, missing_watch};
use crate::boundary::{Result, WatchStore};
use crate::limits::clamp_page_limit;
use crate::unified::pagination::{
    WatchCursor, WatchHistoryCursor, decode_watch_cursor, decode_watch_history_cursor,
    encode_watch_cursor, encode_watch_history_cursor,
};

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
        let cursor = request
            .cursor
            .as_deref()
            .map(decode_watch_cursor)
            .transpose()
            .map_err(|message| {
                ApiError::new("watch.cursor_invalid", ErrorStage::Retrieving, message)
            })?;
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
        items.sort_by(|left, right| right.watch_id.0.cmp(&left.watch_id.0));
        if let Some(cursor) = cursor.as_ref() {
            items.retain(|watch| watch.watch_id.0 < cursor.watch_id);
        }
        let total = cursor.is_none().then_some(items.len() as u64);
        let limit = clamp_page_limit(request.limit);
        let has_more = items.len() > limit as usize;
        items.truncate(limit as usize);
        let next_cursor = items.last().filter(|_| has_more).map(|watch| {
            encode_watch_cursor(&WatchCursor {
                created_at: 0,
                watch_id: watch.watch_id.0.clone(),
            })
        });
        Ok(Page {
            total,
            limit,
            next_cursor,
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
        let latest_job = state.jobs.get(&job_id).map(descriptor);
        if let Some(watch) = state.watches.get_mut(&watch_id) {
            watch.latest_job = latest_job;
        }
        state.watch_runs.entry(watch_id).or_default().push(job_id);
        Ok(())
    }

    async fn history(&self, request: WatchHistoryRequest) -> Result<WatchHistoryResult> {
        let cursor = request
            .cursor
            .as_deref()
            .map(decode_watch_history_cursor)
            .transpose()
            .map_err(|message| {
                ApiError::new("watch.cursor_invalid", ErrorStage::Retrieving, message)
            })?;
        let state = self.state.lock().await;
        if !state.watches.contains_key(&request.watch_id) {
            return Err(missing_watch(&request.watch_id));
        }
        let all_runs = state
            .watch_runs
            .get(&request.watch_id)
            .into_iter()
            .flat_map(|ids| ids.iter())
            .rev()
            .filter_map(|job_id| state.jobs.get(job_id))
            .map(descriptor)
            .filter(|job| request.status.is_none_or(|status| job.status == status))
            .collect::<Vec<_>>();
        let offset = match cursor.as_ref().and_then(|cursor| cursor.job_id.as_deref()) {
            Some(job_id) => all_runs
                .iter()
                .position(|job| job.job_id.0.to_string() == job_id)
                .map(|position| position + 1)
                .ok_or_else(|| {
                    ApiError::new(
                        "watch.cursor_invalid",
                        ErrorStage::Retrieving,
                        "watch history cursor no longer identifies a run",
                    )
                })?,
            None => 0,
        };
        let limit = clamp_page_limit(request.limit) as usize;
        let mut runs = all_runs
            .into_iter()
            .skip(offset)
            .take(limit + 1)
            .collect::<Vec<_>>();
        let has_more = runs.len() > limit;
        if has_more {
            runs.truncate(limit);
        }
        let next_cursor = has_more.then(|| {
            encode_watch_history_cursor(&WatchHistoryCursor {
                created_at: 0,
                run_id: 0,
                job_id: runs.last().map(|job| job.job_id.0.to_string()),
            })
        });
        Ok(WatchHistoryResult {
            watch_id: request.watch_id,
            jobs: runs,
            next_cursor,
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
