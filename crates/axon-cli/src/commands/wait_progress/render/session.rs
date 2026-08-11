use std::collections::HashSet;
use std::error::Error;
use std::future::Future;
use std::io;
use std::io::IsTerminal;
use std::time::{Duration, Instant};

use axon_api::source::{JobId, LifecycleStatus, SourceProgressEvent, SourceResult, SourceScope};
use axon_core::{config::Config, ui};
use axon_services::source::foreground_progress::{
    ForegroundEventStore, ForegroundProgressReceiver, ForegroundSnapshot,
};

use super::{ProgressMode, WaitRenderer};
use crate::commands::wait_progress::format::format_wait_view;
use crate::commands::wait_progress::model::{OperatorPhase, WaitViewModel, operator_phase};
use crate::commands::wait_progress::timing::{RateEstimate, TimingEstimator};

pub(crate) struct WaitProgressSession {
    receiver: ForegroundProgressReceiver,
    store: Option<ForegroundEventStore>,
    model: WaitViewModel,
    renderer: WaitRenderer,
    timing: TimingEstimator,
    estimate: Option<RateEstimate>,
    started_at: Instant,
    active_phase: Option<OperatorPhase>,
    seen_event_ids: HashSet<String>,
    dirty: bool,
    reconciliation_warning_emitted: bool,
    render_error: Option<io::Error>,
}

impl WaitProgressSession {
    pub(crate) fn source(
        cfg: &Config,
        target: impl Into<String>,
        scope: Option<SourceScope>,
        receiver: ForegroundProgressReceiver,
        store: Option<ForegroundEventStore>,
    ) -> Self {
        let mode = ProgressMode::for_config(cfg, io::stderr().is_terminal());
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
            dirty: true,
            reconciliation_warning_emitted: false,
            render_error: None,
        }
    }

    pub(crate) async fn run_until<E>(
        &mut self,
        work: impl Future<Output = Result<SourceResult, E>>,
    ) -> Result<SourceResult, Box<dyn Error>>
    where
        E: Into<Box<dyn Error>>,
    {
        tokio::pin!(work);
        let mut cadence = tokio::time::interval(Duration::from_millis(250));
        let mut snapshots_open = true;
        let mut events_open = true;
        loop {
            tokio::select! {
                result = &mut work => {
                    self.drain_ready_updates();
                    self.reconcile_if_overflowed().await;
                    let result = result.map_err(Into::into);
                    let status = result
                        .as_ref()
                        .map_or(LifecycleStatus::Failed, |source| source.status);
                    self.finish(status);
                    if result.is_ok()
                        && let Some(error) = self.render_error.take()
                    {
                        return Err(Box::new(error));
                    }
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

    pub(crate) fn finish(&mut self, status: LifecycleStatus) {
        self.model.finish(status);
        let formatted = self.formatted();
        let render_result = self.renderer.finish(&formatted);
        self.record_render_result(render_result);
        self.dirty = false;
    }

    fn apply_latest_snapshot(&mut self) {
        if let Some(source_kind) = self.receiver.source_kind() {
            self.dirty |= self.model.set_source_kind(source_kind);
        }
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
            ForegroundSnapshot::Routed {
                job_id,
                source_kind,
            } => {
                self.model.job_id = Some(job_id);
                self.dirty |= self.model.set_source_kind(source_kind);
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
                let diagnostic = self
                    .renderer
                    .diagnostic(&ui::muted(&format!("progress catch-up deferred: {error}")));
                self.record_render_result(diagnostic);
                self.reconciliation_warning_emitted = true;
            }
        } else {
            self.reconciliation_warning_emitted = false;
        }
    }

    async fn reconcile_events(
        &mut self,
        store: ForegroundEventStore,
        job_id: JobId,
    ) -> Result<(), axon_api::source::ApiError> {
        for event in store.public_source_events(job_id).await? {
            self.apply_event(event);
        }
        Ok(())
    }

    fn render_if_dirty(&mut self) {
        if !self.dirty {
            return;
        }
        let formatted = self.formatted();
        let render_result = self.renderer.render(&formatted);
        self.record_render_result(render_result);
        self.dirty = false;
    }

    fn record_render_result(&mut self, result: io::Result<()>) {
        if let Err(error) = result
            && self.render_error.is_none()
        {
            self.render_error = Some(error);
        }
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
