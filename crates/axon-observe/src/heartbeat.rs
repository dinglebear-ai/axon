//! Heartbeat builders for the target observability boundary.

pub const MODULE_NAME: &str = "heartbeat";

use axon_api::source::{
    JobHeartbeat, JobId, LifecycleStatus, PipelinePhase, ProviderReservationSnapshot, StageCounts,
    Timestamp,
};
use chrono::Utc;

pub fn foreground_interval_secs() -> u64 {
    5
}

pub fn background_interval_secs() -> u64 {
    15
}

pub fn heartbeat(
    job_id: JobId,
    attempt: u32,
    phase: PipelinePhase,
    status: LifecycleStatus,
) -> JobHeartbeat {
    JobHeartbeat {
        job_id,
        attempt,
        worker_id: None,
        phase,
        status,
        stage_id: None,
        heartbeat_at: Timestamp::from(Utc::now()),
        last_event_sequence: None,
        counts: None,
        provider_reservations: Vec::new(),
    }
}

pub trait JobHeartbeatExt {
    fn with_worker(self, worker_id: impl Into<String>) -> Self;
    fn with_last_event_sequence(self, sequence: u64) -> Self;
    fn with_counts(self, counts: StageCounts) -> Self;
    fn with_provider_reservations(self, reservations: Vec<ProviderReservationSnapshot>) -> Self;
}

impl JobHeartbeatExt for JobHeartbeat {
    fn with_worker(mut self, worker_id: impl Into<String>) -> Self {
        self.worker_id = Some(worker_id.into());
        self
    }

    fn with_last_event_sequence(mut self, sequence: u64) -> Self {
        self.last_event_sequence = Some(sequence);
        self
    }

    fn with_counts(mut self, counts: StageCounts) -> Self {
        self.counts = Some(counts);
        self
    }

    fn with_provider_reservations(
        mut self,
        reservations: Vec<ProviderReservationSnapshot>,
    ) -> Self {
        self.provider_reservations = reservations;
        self
    }
}
