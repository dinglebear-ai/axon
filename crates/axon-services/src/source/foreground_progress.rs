use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use axon_api::source::{JobId, JobStatusUpdate, SourceProgressEvent};
use tokio::sync::{mpsc, watch};

pub const FOREGROUND_EVENT_CAPACITY: usize = 256;

#[derive(Debug, Clone, PartialEq)]
pub enum ForegroundSnapshot {
    JobStarted(JobId),
    Status(JobStatusUpdate),
}

impl ForegroundSnapshot {
    pub fn job_id(&self) -> JobId {
        match self {
            Self::JobStarted(job_id) => *job_id,
            Self::Status(update) => update.job_id,
        }
    }

    pub fn status(&self) -> Option<&JobStatusUpdate> {
        match self {
            Self::Status(update) => Some(update),
            Self::JobStarted(_) => None,
        }
    }
}

#[derive(Clone)]
pub struct ForegroundProgressSender {
    snapshots: watch::Sender<Option<ForegroundSnapshot>>,
    events: mpsc::Sender<SourceProgressEvent>,
    overflow: Arc<AtomicBool>,
}

pub struct ForegroundProgressReceiver {
    pub snapshots: watch::Receiver<Option<ForegroundSnapshot>>,
    pub events: mpsc::Receiver<SourceProgressEvent>,
    overflow: Arc<AtomicBool>,
}

pub fn foreground_progress_channel() -> (ForegroundProgressSender, ForegroundProgressReceiver) {
    foreground_progress_channel_with_capacity(FOREGROUND_EVENT_CAPACITY)
}

fn foreground_progress_channel_with_capacity(
    capacity: usize,
) -> (ForegroundProgressSender, ForegroundProgressReceiver) {
    let (snapshot_tx, snapshot_rx) = watch::channel(None);
    let (event_tx, event_rx) = mpsc::channel(capacity.max(1));
    let overflow = Arc::new(AtomicBool::new(false));
    (
        ForegroundProgressSender {
            snapshots: snapshot_tx,
            events: event_tx,
            overflow: Arc::clone(&overflow),
        },
        ForegroundProgressReceiver {
            snapshots: snapshot_rx,
            events: event_rx,
            overflow,
        },
    )
}

impl ForegroundProgressSender {
    pub fn job_started(&self, job_id: JobId) {
        self.snapshots
            .send_replace(Some(ForegroundSnapshot::JobStarted(job_id)));
    }

    pub fn snapshot(&self, update: JobStatusUpdate) {
        self.snapshots
            .send_replace(Some(ForegroundSnapshot::Status(update)));
    }

    pub fn event(&self, event: SourceProgressEvent) -> bool {
        match self.events.try_send(event) {
            Ok(()) => true,
            Err(mpsc::error::TrySendError::Full(_)) => {
                self.overflow.store(true, Ordering::Release);
                false
            }
            Err(mpsc::error::TrySendError::Closed(_)) => false,
        }
    }
}

impl ForegroundProgressReceiver {
    pub fn overflowed(&self) -> bool {
        self.overflow.load(Ordering::Acquire)
    }

    pub fn take_overflowed(&self) -> bool {
        self.overflow.swap(false, Ordering::AcqRel)
    }

    pub fn mark_overflowed(&self) {
        self.overflow.store(true, Ordering::Release);
    }
}

#[cfg(test)]
#[path = "foreground_progress_tests.rs"]
mod tests;
