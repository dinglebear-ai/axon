use std::future::Future;

use axon_api::source::{JobStatusUpdate, SourceProgressEvent};
use axon_services::source::foreground_progress::{ForegroundProgressReceiver, ForegroundSnapshot};
use tokio::sync::mpsc;

pub(crate) enum BatchProgressUpdate {
    Started {
        index: usize,
        target: String,
    },
    Snapshot {
        index: usize,
        update: JobStatusUpdate,
    },
    Event {
        index: usize,
        event: SourceProgressEvent,
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

pub(crate) fn batch_progress_channel() -> (
    BatchProgressForwarder,
    mpsc::UnboundedReceiver<BatchProgressUpdate>,
) {
    let (updates, receiver) = mpsc::unbounded_channel();
    (BatchProgressForwarder { updates }, receiver)
}

impl BatchProgressForwarder {
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
                        let _ = self.updates.send(BatchProgressUpdate::Event { index, event });
                    } else {
                        events_open = false;
                    }
                }
            }
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
        let _ = updates.send(BatchProgressUpdate::Event { index, event });
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
