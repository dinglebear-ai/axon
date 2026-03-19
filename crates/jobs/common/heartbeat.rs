//! Content-aware heartbeat: detects when a job's `result_json` stops changing
//! (content-aware staleness) vs just touching `updated_at` (blind liveness).
//!
//! ## How it works
//!
//! Every `interval_secs`, the heartbeat:
//! 1. Touches `updated_at` (keeps the watchdog happy — same as the old heartbeat)
//! 2. Reads `result_json` and compares to the previous snapshot
//! 3. Logs a warning when content is unchanged for `STALE_STREAK_WARN_THRESHOLD`
//!    consecutive intervals (diagnostic only — does NOT cancel jobs)
//!
//! ## Why
//!
//! A job can touch `updated_at` forever without making real progress (e.g. stuck
//! in a retry loop, blocked on a lock, spinning on an empty queue). Content-aware
//! heartbeat catches this by comparing `result_json` snapshots across intervals.

use crate::crates::core::logging::{log_debug, log_warn};
use crate::crates::jobs::common::JobTable;
use sqlx::PgPool;
use std::time::Duration;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use uuid::Uuid;

/// Number of consecutive unchanged intervals before the first warning.
/// At 30s cadence, 6 intervals = 3 minutes of no progress.
const STALE_STREAK_WARN_THRESHOLD: u32 = 6;

/// Compare two `result_json` snapshots. Returns `true` if content
/// is identical (no progress between heartbeats).
///
/// Rules:
/// - `(Some(a), Some(b))` where `a == b` → stale (no change)
/// - `(None, None)` → not stale (job just started, no result yet)
/// - `(None, Some(_))` or `(Some(_), None)` → not stale (transition = progress)
pub fn is_content_stale(
    prev: &Option<serde_json::Value>,
    curr: &Option<serde_json::Value>,
) -> bool {
    match (prev, curr) {
        (Some(p), Some(c)) => p == c,
        _ => false,
    }
}

/// Spawn a heartbeat that:
/// 1. Touches `updated_at` every interval (keeps watchdog happy)
/// 2. Reads `result_json` and compares to previous snapshot
/// 3. Logs warning when content unchanged for 6+ intervals (3 min at 30s)
///
/// Diagnostic only — does NOT cancel jobs.
///
/// # Usage
///
/// ```ignore
/// let (stop_tx, hb) = spawn_content_aware_heartbeat(pool.clone(), TABLE, id, 30);
/// // ... do work ...
/// let _ = stop_tx.send(true);
/// let _ = hb.await;
/// ```
pub fn spawn_content_aware_heartbeat(
    pool: PgPool,
    table: JobTable,
    id: Uuid,
    interval_secs: u64,
) -> (watch::Sender<bool>, JoinHandle<()>) {
    let (stop_tx, mut stop_rx) = watch::channel(false);
    let handle = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(interval_secs));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut prev_snapshot: Option<serde_json::Value> = None;
        let mut stale_streak: u32 = 0;

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    // Keep the watchdog happy (same as the old blind heartbeat).
                    let _ = super::job_ops::touch_running_job(&pool, table, id).await;

                    let curr = read_result_json(&pool, table, id).await;

                    if is_content_stale(&prev_snapshot, &curr) {
                        stale_streak += 1;
                        if stale_streak == STALE_STREAK_WARN_THRESHOLD {
                            let phase = curr.as_ref()
                                .and_then(|v| v.get("phase"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown");
                            log_warn(&format!(
                                "heartbeat content_stale job_id={id} table={} streak={stale_streak} phase={phase} no_progress_secs={}",
                                table.as_str(),
                                u64::from(stale_streak) * interval_secs,
                            ));
                        } else if stale_streak > STALE_STREAK_WARN_THRESHOLD
                            && stale_streak.is_multiple_of(STALE_STREAK_WARN_THRESHOLD)
                        {
                            log_warn(&format!(
                                "heartbeat content_still_stale job_id={id} table={} streak={stale_streak} no_progress_secs={}",
                                table.as_str(),
                                u64::from(stale_streak) * interval_secs,
                            ));
                        }
                    } else {
                        if stale_streak >= STALE_STREAK_WARN_THRESHOLD {
                            log_debug(&format!(
                                "heartbeat content_unstalled job_id={id} table={} streak_was={stale_streak}",
                                table.as_str(),
                            ));
                        }
                        stale_streak = 0;
                    }

                    prev_snapshot = curr;
                }
                changed = stop_rx.changed() => {
                    if changed.is_err() || *stop_rx.borrow() {
                        break;
                    }
                }
            }
        }
    });
    (stop_tx, handle)
}

/// Spawn a background heartbeat task that calls [`touch_running_job`] on `interval_secs`
/// cadence until the returned sender signals stop.
///
/// This is the original blind heartbeat — preserved for callers that don't need
/// content-aware staleness detection.
///
/// # Usage
///
/// ```ignore
/// let (stop_tx, heartbeat) = spawn_heartbeat_task(pool.clone(), TABLE, id, 15);
/// // ... do work ...
/// let _ = stop_tx.send(true);
/// let _ = heartbeat.await;
/// ```
pub fn spawn_heartbeat_task(
    pool: PgPool,
    table: JobTable,
    id: Uuid,
    interval_secs: u64,
) -> (watch::Sender<bool>, JoinHandle<()>) {
    let (stop_tx, mut stop_rx) = watch::channel(false);
    let handle = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(interval_secs));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    let _ = super::job_ops::touch_running_job(&pool, table, id).await;
                }
                changed = stop_rx.changed() => {
                    if changed.is_err() || *stop_rx.borrow() {
                        break;
                    }
                }
            }
        }
    });
    (stop_tx, handle)
}

/// Read `result_json` for a job. Returns `None` on any error (network, missing row).
/// Never panics — callers treat missing data as "no snapshot available".
async fn read_result_json(pool: &PgPool, table: JobTable, id: Uuid) -> Option<serde_json::Value> {
    let query = format!("SELECT result_json FROM {} WHERE id = $1", table.as_str());
    sqlx::query_scalar::<_, Option<serde_json::Value>>(&query)
        .bind(id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_stale_when_result_json_unchanged() {
        let prev = Some(serde_json::json!({"files_done": 150, "phase": "embedding_batch"}));
        let curr = Some(serde_json::json!({"files_done": 150, "phase": "embedding_batch"}));
        assert!(is_content_stale(&prev, &curr));
    }

    #[test]
    fn detect_progress_when_result_json_changes() {
        let prev = Some(serde_json::json!({"files_done": 150, "phase": "embedding_batch"}));
        let curr = Some(serde_json::json!({"files_done": 151, "phase": "collecting_files"}));
        assert!(!is_content_stale(&prev, &curr));
    }

    #[test]
    fn null_previous_is_never_stale() {
        let prev = None;
        let curr = Some(serde_json::json!({"files_done": 1}));
        assert!(!is_content_stale(&prev, &curr));
    }

    #[test]
    fn both_null_is_not_stale() {
        assert!(!is_content_stale(&None, &None));
    }

    #[test]
    fn some_to_none_is_not_stale() {
        let prev = Some(serde_json::json!({"files_done": 5}));
        assert!(!is_content_stale(&prev, &None));
    }
}
