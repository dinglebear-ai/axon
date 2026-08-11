use std::collections::HashSet;
use std::future::Future;
use std::io::IsTerminal;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axon_api::source::{
    JobEventListRequest, JobId, LifecycleStatus, SourceProgressEvent, SourceScope, Visibility,
};
use axon_core::{config::Config, ui};
use axon_jobs::boundary::JobStore;
use axon_services::source::foreground_progress::{ForegroundProgressReceiver, ForegroundSnapshot};

use super::{ProgressMode, WaitRenderer};
use crate::commands::wait_progress::format::format_wait_view;
use crate::commands::wait_progress::model::{OperatorPhase, WaitViewModel, operator_phase};
use crate::commands::wait_progress::timing::{RateEstimate, TimingEstimator};

pub(crate) struct WaitProgressSession {
    receiver: ForegroundProgressReceiver,
    store: Option<Arc<dyn JobStore>>,
    model: WaitViewModel,
    renderer: WaitRenderer,
    timing: TimingEstimator,
    estimate: Option<RateEstimate>,
    started_at: Instant,
    active_phase: Option<OperatorPhase>,
    seen_event_ids: HashSet<String>,
    last_durable_sequence: Option<u64>,
    next_cursor: Option<String>,
    dirty: bool,
    reconciliation_warning_emitted: bool,
}

impl WaitProgressSession {
    pub(crate) fn source(
        cfg: &Config,
        target: impl Into<String>,
        scope: Option<SourceScope>,
        receiver: ForegroundProgressReceiver,
        store: Option<Arc<dyn JobStore>>,
    ) -> Self {
        let mode = ProgressMode::for_config(cfg, std::io::stderr().is_terminal());
        Self {
            receiver,
            store,
            model: WaitViewModel::source(target, scope),
            renderer: WaitRenderer::new(mode),
            timing: TimingEstimator::default(),
            estimate: None,
            started_at: Instant::now(),
            active_phase: None,
            seen_event_ids: HashSet::new(),
            last_durable_sequence: None,
            next_cursor: None,
            dirty: true,
            reconciliation_warning_emitted: false,
        }
    }

    pub(crate) async fn run_until<T>(&mut self, work: impl Future<Output = T>) -> T {
        tokio::pin!(work);
        let mut cadence = tokio::time::interval(Duration::from_millis(250));
        let mut snapshots_open = true;
        let mut events_open = true;
        loop {
            tokio::select! {
                result = &mut work => {
                    self.drain_ready_updates();
                    self.finish();
                    return result;
                }
                changed = self.receiver.snapshots.changed(), if snapshots_open => {
                    if changed.is_ok() {
                        self.apply_latest_snapshot();
                    } else {
                        snapshots_open = false;
                    }
                }
                event = self.receiver.events.recv(), if events_open => {
                    if let Some(event) = event {
                        self.apply_event(event);
                    } else {
                        events_open = false;
                    }
                }
                _ = cadence.tick() => {
                    self.reconcile_if_overflowed().await;
                    self.render_if_dirty();
                }
            }
        }
    }

    pub(crate) fn finish(&mut self) {
        self.model.finish();
        let formatted = self.formatted();
        let _ = self.renderer.finish(&formatted);
        self.dirty = false;
    }

    fn apply_latest_snapshot(&mut self) {
        let snapshot = self.receiver.snapshots.borrow_and_update().clone();
        let Some(snapshot) = snapshot else {
            return;
        };
        match snapshot {
            ForegroundSnapshot::JobStarted(job_id) => {
                if self.model.job_id != Some(job_id) {
                    self.model.job_id = Some(job_id);
                    self.dirty = true;
                }
            }
            ForegroundSnapshot::Status(update) => {
                let phase = operator_phase(update.phase);
                if self.active_phase != Some(phase) {
                    self.timing.reset();
                    self.active_phase = Some(phase);
                }
                if self.model.apply_snapshot(*update) {
                    self.sample_timing();
                    self.dirty = true;
                }
            }
        }
    }

    fn apply_event(&mut self, event: SourceProgressEvent) {
        if !self.seen_event_ids.insert(event.event_id.clone()) {
            return;
        }
        let elapsed = self.started_at.elapsed();
        if event.status == LifecycleStatus::Running {
            self.model.start_phase_at(event.phase, elapsed);
        } else if matches!(
            event.status,
            LifecycleStatus::Completed | LifecycleStatus::CompletedDegraded
        ) {
            self.model.complete_phase_at(event.phase, elapsed);
        }
        if self.model.apply_event(event) {
            self.dirty = true;
        }
    }

    fn sample_timing(&mut self) {
        let Some(active) = self.model.active.as_ref() else {
            self.estimate = None;
            return;
        };
        self.estimate = self
            .timing
            .sample(self.started_at.elapsed(), active.done, active.total);
    }

    fn drain_ready_updates(&mut self) {
        if self.receiver.snapshots.has_changed().unwrap_or(false) {
            self.apply_latest_snapshot();
        }
        while let Ok(event) = self.receiver.events.try_recv() {
            self.apply_event(event);
        }
    }

    async fn reconcile_if_overflowed(&mut self) {
        if !self.receiver.take_overflowed() {
            return;
        }
        let (Some(store), Some(job_id)) = (self.store.clone(), self.model.job_id) else {
            self.receiver.mark_overflowed();
            return;
        };
        if let Err(error) = self.reconcile_events(store, job_id).await {
            self.receiver.mark_overflowed();
            if !self.reconciliation_warning_emitted {
                self.renderer
                    .diagnostic(&ui::muted(&format!("progress catch-up deferred: {error}")));
                self.reconciliation_warning_emitted = true;
            }
        } else {
            self.reconciliation_warning_emitted = false;
        }
    }

    async fn reconcile_events(
        &mut self,
        store: Arc<dyn JobStore>,
        job_id: JobId,
    ) -> Result<(), axon_api::source::ApiError> {
        loop {
            let request = JobEventListRequest {
                job_id,
                after_sequence: self.last_durable_sequence,
                limit: Some(200),
                severity: None,
                visibility: Some(Visibility::Public),
                phase: None,
                since_sequence: None,
                cursor: self.next_cursor.take(),
            };
            let page = store.events(request).await?;
            for event in page.events {
                if self.seen_event_ids.insert(event.event_id.clone())
                    && self.model.apply_persisted_event(event)
                {
                    self.dirty = true;
                }
            }
            self.last_durable_sequence = Some(page.last_sequence);
            self.next_cursor = page.next_cursor;
            if self.next_cursor.is_none() {
                return Ok(());
            }
        }
    }

    fn render_if_dirty(&mut self) {
        if !self.dirty {
            return;
        }
        let formatted = self.formatted();
        let _ = self.renderer.render(&formatted);
        self.dirty = false;
    }

    fn formatted(&self) -> crate::commands::wait_progress::format::FormattedWaitView {
        let width = std::env::var("COLUMNS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(80);
        format_wait_view(
            &self.model,
            width,
            self.estimate,
            ui::stderr_color_enabled(),
        )
    }
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
