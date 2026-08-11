use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use axon_api::source::{
    ApiError, ErrorStage, JobEventListRequest, JobId, JobStatusUpdate, SourceKind,
    SourceProgressEvent, Visibility,
};
use axon_jobs::boundary::JobStore;
use tokio::sync::{mpsc, watch};

pub const FOREGROUND_EVENT_CAPACITY: usize = 256;

#[derive(Debug, Clone, PartialEq)]
pub enum ForegroundSnapshot {
    JobStarted(JobId),
    Routed {
        job_id: JobId,
        source_kind: SourceKind,
    },
    Status(Box<JobStatusUpdate>),
}

impl ForegroundSnapshot {
    pub fn job_id(&self) -> JobId {
        match self {
            Self::JobStarted(job_id) => *job_id,
            Self::Routed { job_id, .. } => *job_id,
            Self::Status(update) => update.job_id,
        }
    }

    pub fn status(&self) -> Option<&JobStatusUpdate> {
        match self {
            Self::Status(update) => Some(update),
            Self::JobStarted(_) | Self::Routed { .. } => None,
        }
    }
}

#[derive(Clone)]
pub struct ForegroundEventStore {
    store: Arc<dyn JobStore>,
}

impl ForegroundEventStore {
    #[doc(hidden)]
    pub fn new(store: Arc<dyn JobStore>) -> Self {
        Self { store }
    }

    pub async fn public_source_events(
        &self,
        job_id: JobId,
    ) -> Result<Vec<SourceProgressEvent>, ApiError> {
        let mut events = Vec::new();
        let mut cursor = None;
        loop {
            let page = self
                .store
                .events(JobEventListRequest {
                    job_id,
                    after_sequence: None,
                    limit: Some(200),
                    severity: None,
                    visibility: Some(Visibility::Public),
                    phase: None,
                    since_sequence: None,
                    cursor: cursor.take(),
                })
                .await?;
            for event in page.events {
                let value = event
                    .details
                    .get("source_progress_event")
                    .cloned()
                    .ok_or_else(|| malformed_progress_event("missing event projection"))?;
                events.push(
                    serde_json::from_value(value)
                        .map_err(|error| malformed_progress_event(&error.to_string()))?,
                );
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                return Ok(events);
            }
        }
    }
}

fn malformed_progress_event(reason: &str) -> ApiError {
    ApiError::new(
        "progress.event_projection_invalid",
        ErrorStage::Retrieving,
        format!("stored progress event is invalid: {reason}"),
    )
}

#[derive(Debug, Clone)]
pub struct ForegroundProgressSender {
    snapshots: watch::Sender<Option<ForegroundSnapshot>>,
    events: mpsc::Sender<SourceProgressEvent>,
    overflow: Arc<AtomicBool>,
    source_kind: Arc<Mutex<Option<SourceKind>>>,
}

pub struct ForegroundProgressReceiver {
    pub snapshots: watch::Receiver<Option<ForegroundSnapshot>>,
    pub events: mpsc::Receiver<SourceProgressEvent>,
    overflow: Arc<AtomicBool>,
    source_kind: Arc<Mutex<Option<SourceKind>>>,
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
    let source_kind = Arc::new(Mutex::new(None));
    (
        ForegroundProgressSender {
            snapshots: snapshot_tx,
            events: event_tx,
            overflow: Arc::clone(&overflow),
            source_kind: Arc::clone(&source_kind),
        },
        ForegroundProgressReceiver {
            snapshots: snapshot_rx,
            events: event_rx,
            overflow,
            source_kind,
        },
    )
}

impl ForegroundProgressSender {
    pub fn job_started(&self, job_id: JobId) {
        self.snapshots
            .send_replace(Some(ForegroundSnapshot::JobStarted(job_id)));
    }

    pub fn routed(&self, job_id: JobId, source_kind: SourceKind) {
        if let Ok(mut sticky_source_kind) = self.source_kind.lock() {
            *sticky_source_kind = Some(source_kind);
        }
        self.snapshots
            .send_replace(Some(ForegroundSnapshot::Routed {
                job_id,
                source_kind,
            }));
    }

    pub fn snapshot(&self, update: JobStatusUpdate) {
        self.snapshots
            .send_replace(Some(ForegroundSnapshot::Status(Box::new(update))));
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
    pub fn source_kind(&self) -> Option<SourceKind> {
        self.source_kind.lock().ok().and_then(|kind| *kind)
    }

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
