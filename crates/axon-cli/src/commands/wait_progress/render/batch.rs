use std::future::Future;
use std::io::IsTerminal;

use axon_api::source::{JobStatusUpdate, SourceProgressEvent};
use axon_core::{config::Config, ui};
use axon_services::source::foreground_progress::{ForegroundProgressReceiver, ForegroundSnapshot};
use tokio::sync::mpsc;

use super::{ProgressMode, WaitRenderer};
use crate::commands::wait_progress::format::FormattedWaitView;
use crate::commands::wait_progress::model::BatchWaitViewModel;

pub(crate) enum BatchProgressUpdate {
    Started {
        index: usize,
        target: String,
    },
    Snapshot {
        index: usize,
        update: Box<JobStatusUpdate>,
    },
    Event {
        index: usize,
        event: Box<SourceProgressEvent>,
    },
    Finished {
        index: usize,
        failed: bool,
    },
}

#[derive(Clone)]
pub(crate) struct BatchProgressForwarder {
    updates: mpsc::UnboundedSender<BatchProgressUpdate>,
}

pub(crate) struct BatchProgressSession {
    updates: mpsc::UnboundedReceiver<BatchProgressUpdate>,
    model: BatchWaitViewModel,
    renderer: WaitRenderer,
    color: bool,
}

pub(crate) fn batch_progress_channel() -> (
    BatchProgressForwarder,
    mpsc::UnboundedReceiver<BatchProgressUpdate>,
) {
    let (updates, receiver) = mpsc::unbounded_channel();
    (BatchProgressForwarder { updates }, receiver)
}

impl BatchProgressForwarder {
    pub(crate) fn failed_before_start(&self, index: usize, target: String) {
        let _ = self
            .updates
            .send(BatchProgressUpdate::Started { index, target });
        let _ = self.updates.send(BatchProgressUpdate::Finished {
            index,
            failed: true,
        });
    }

    pub(crate) async fn run_until<T, E>(
        &self,
        index: usize,
        target: impl Into<String>,
        mut receiver: ForegroundProgressReceiver,
        work: impl Future<Output = Result<T, E>>,
    ) -> Result<T, E> {
        let _ = self.updates.send(BatchProgressUpdate::Started {
            index,
            target: target.into(),
        });
        tokio::pin!(work);
        let mut snapshots_open = true;
        let mut events_open = true;
        loop {
            tokio::select! {
                result = &mut work => {
                    drain_ready(index, &mut receiver, &self.updates);
                    let _ = self.updates.send(BatchProgressUpdate::Finished {
                        index,
                        failed: result.is_err(),
                    });
                    return result;
                }
                changed = receiver.snapshots.changed(), if snapshots_open => {
                    if changed.is_ok() {
                        forward_snapshot(index, &mut receiver, &self.updates);
                    } else {
                        snapshots_open = false;
                    }
                }
                event = receiver.events.recv(), if events_open => {
                    if let Some(event) = event {
                        let _ = self.updates.send(BatchProgressUpdate::Event {
                            index,
                            event: Box::new(event),
                        });
                    } else {
                        events_open = false;
                    }
                }
            }
        }
    }
}

impl BatchProgressSession {
    pub(crate) fn new(
        cfg: &Config,
        total: usize,
        updates: mpsc::UnboundedReceiver<BatchProgressUpdate>,
    ) -> Self {
        let mode = ProgressMode::for_config(cfg, std::io::stderr().is_terminal());
        Self {
            updates,
            model: BatchWaitViewModel::new(total),
            renderer: WaitRenderer::new(mode),
            color: ui::stderr_color_enabled(),
        }
    }

    pub(crate) async fn run_until<T>(&mut self, work: impl Future<Output = T>) -> T {
        tokio::pin!(work);
        let mut cadence = tokio::time::interval(std::time::Duration::from_millis(250));
        loop {
            tokio::select! {
                result = &mut work => {
                    self.drain_ready();
                    let view = self.formatted(true);
                    let _ = self.renderer.finish(&view);
                    return result;
                }
                update = self.updates.recv() => {
                    if let Some(update) = update {
                        self.apply(update);
                    }
                }
                _ = cadence.tick() => {
                    let view = self.formatted(false);
                    let _ = self.renderer.render(&view);
                }
            }
        }
    }

    fn drain_ready(&mut self) {
        while let Ok(update) = self.updates.try_recv() {
            self.apply(update);
        }
    }

    fn apply(&mut self, update: BatchProgressUpdate) {
        match update {
            BatchProgressUpdate::Started { index, target } => self.model.running(index, target),
            BatchProgressUpdate::Snapshot { index, update } => {
                self.model.apply_snapshot(index, *update);
            }
            BatchProgressUpdate::Event { index, event } => self.model.apply_event(index, *event),
            BatchProgressUpdate::Finished { index, failed } => {
                if failed {
                    self.model.failed(index);
                } else {
                    self.model.completed(index);
                }
            }
        }
    }

    fn formatted(&self, terminal: bool) -> FormattedWaitView {
        let summary = self.model.summary();
        let mut active = vec![ui::accent_when(
            self.color,
            &format!("◐ source     {summary}"),
        )];
        if let Some(target) = self.model.active_detail() {
            let detail = target.progress.as_ref().map_or_else(
                || target.target.clone(),
                |progress| {
                    let count = progress.total.map_or_else(
                        || format!("{} {}", progress.done, progress.unit),
                        |total| format!("{}/{} {}", progress.done, total, progress.unit),
                    );
                    format!("{} {} · {count}", progress.phase.label(), target.target)
                },
            );
            active.push(ui::info_when(self.color, &format!("   {detail}")));
        }
        FormattedWaitView {
            heading: format!("  {} source batch", ui::primary_when(self.color, "axon")),
            milestones: Vec::new(),
            notices: Vec::new(),
            active: if terminal { Vec::new() } else { active },
            terminal: terminal
                .then(|| ui::success_when(self.color, &format!("✓ source     {summary}"))),
        }
    }
}

fn drain_ready(
    index: usize,
    receiver: &mut ForegroundProgressReceiver,
    updates: &mpsc::UnboundedSender<BatchProgressUpdate>,
) {
    if receiver.snapshots.has_changed().unwrap_or(false) {
        forward_snapshot(index, receiver, updates);
    }
    while let Ok(event) = receiver.events.try_recv() {
        let _ = updates.send(BatchProgressUpdate::Event {
            index,
            event: Box::new(event),
        });
    }
}

fn forward_snapshot(
    index: usize,
    receiver: &mut ForegroundProgressReceiver,
    updates: &mpsc::UnboundedSender<BatchProgressUpdate>,
) {
    if let Some(ForegroundSnapshot::Status(update)) = receiver.snapshots.borrow_and_update().clone()
    {
        let _ = updates.send(BatchProgressUpdate::Snapshot { index, update });
    }
}
