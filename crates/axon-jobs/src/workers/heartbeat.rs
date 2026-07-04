use std::sync::Arc;
use std::time::Duration;

use sqlx::SqlitePool;
use tokio::task::JoinHandle;

use crate::backend::JobKind;
use crate::ops::touch_heartbeat_for_attempt;
use axon_api::source::{JobHeartbeat, JobId, LifecycleStatus, PipelinePhase, Timestamp};

/// Default heartbeat interval. Watchdog stale threshold (default 300s + 60s
/// confirm = 360s) is much larger, so a 30s interval gives ~12x margin.
pub(super) const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

/// RAII guard that spawns a periodic heartbeat task for a running job and
/// aborts it on drop. The heartbeat updates `updated_at` so the watchdog's
/// stale detection does not reclaim long-running jobs that haven't emitted
/// a progress update.
pub(super) struct HeartbeatGuard {
    handle: Option<JoinHandle<()>>,
}

impl HeartbeatGuard {
    pub(super) fn spawn(
        pool: Arc<SqlitePool>,
        kind: JobKind,
        id: uuid::Uuid,
        attempt_id: String,
    ) -> Self {
        let handle = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(HEARTBEAT_INTERVAL);
            // Skip the immediate tick — claim_next_pending already sets updated_at.
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            ticker.tick().await;
            loop {
                ticker.tick().await;
                if let Err(e) =
                    touch_heartbeat_for_attempt(&pool, kind, id, Some(&attempt_id)).await
                {
                    tracing::warn!(
                        table = kind.table_name(),
                        job_id = %id,
                        error = %e,
                        "heartbeat: touch_heartbeat failed"
                    );
                }
            }
        });
        Self {
            handle: Some(handle),
        }
    }
}

impl Drop for HeartbeatGuard {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}

#[allow(dead_code)]
pub(crate) fn legacy_job_heartbeat(
    id: uuid::Uuid,
    kind: JobKind,
    attempt: u32,
    worker_id: Option<String>,
    last_event_sequence: Option<u64>,
) -> JobHeartbeat {
    JobHeartbeat {
        job_id: JobId::new(id),
        attempt,
        worker_id,
        phase: match kind {
            JobKind::Crawl => PipelinePhase::Fetching,
            JobKind::Embed => PipelinePhase::Embedding,
            JobKind::Extract => PipelinePhase::Synthesizing,
            JobKind::Ingest => PipelinePhase::Fetching,
        },
        status: LifecycleStatus::Running,
        stage_id: None,
        heartbeat_at: Timestamp::from(chrono::Utc::now()),
        last_event_sequence,
        counts: None,
        provider_reservations: Vec::new(),
    }
}

#[cfg(test)]
#[path = "heartbeat_tests.rs"]
mod tests;
