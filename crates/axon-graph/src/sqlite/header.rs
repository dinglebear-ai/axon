//! Shared timestamp and stage-header helpers for the SQLite graph store.

use axon_api::source::{
    JobId, LifecycleStatus, PipelinePhase, StageCounts, StageId, StageResultHeader, Timestamp,
};

/// Current UTC timestamp as an RFC3339 string.
pub fn now_timestamp() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// A completed `Graphing`-phase stage header for a graph write result.
pub fn stage_header() -> StageResultHeader {
    let now = Timestamp(now_timestamp());
    StageResultHeader {
        job_id: JobId::new(uuid::Uuid::nil()),
        stage_id: StageId::new(uuid::Uuid::nil()),
        phase: PipelinePhase::Graphing,
        status: LifecycleStatus::Completed,
        started_at: now.clone(),
        completed_at: Some(now),
        counts: StageCounts {
            items_total: None,
            items_done: 0,
            documents_total: None,
            documents_done: 0,
            chunks_total: None,
            chunks_done: 0,
            bytes_total: None,
            bytes_done: 0,
        },
        warnings: Vec::new(),
        error: None,
    }
}
