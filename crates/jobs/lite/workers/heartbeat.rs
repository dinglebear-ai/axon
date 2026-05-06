use std::sync::Arc;
use std::time::Duration;

use sqlx::SqlitePool;
use tokio::task::JoinHandle;

use crate::crates::jobs::backend::JobKind;
use crate::crates::jobs::lite::ops::touch_heartbeat;

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
    pub(super) fn spawn(pool: Arc<SqlitePool>, kind: JobKind, id: uuid::Uuid) -> Self {
        let handle = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(HEARTBEAT_INTERVAL);
            // Skip the immediate tick — claim_next_pending already sets updated_at.
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            ticker.tick().await;
            loop {
                ticker.tick().await;
                if let Err(e) = touch_heartbeat(&pool, kind, id).await {
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
