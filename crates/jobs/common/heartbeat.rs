//! Content-aware heartbeat: detects when a job's `result_json` stops changing
//! (content-aware staleness) vs just touching `updated_at` (blind liveness).
//!
//! ## How it works
//!
//! Every `interval_secs`, the heartbeat:
//! 1. Touches `updated_at` (keeps the watchdog happy — same as the old heartbeat)
//! 2. Reads `result_json` and compares to the previous snapshot
//! 3. Logs a warning when content is unchanged for `STALE_STREAK_WARN_THRESHOLD`
//!    consecutive intervals
//! 4. Cancels the returned `CancellationToken` when content is unchanged for
//!    `STALE_STREAK_KILL_THRESHOLD` consecutive intervals (forces job failure)
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
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Number of consecutive unchanged intervals before the first warning.
/// At 30s cadence, 6 intervals = 3 minutes of no progress.
const STALE_STREAK_WARN_THRESHOLD: u32 = 6;

/// Number of consecutive unchanged intervals before the heartbeat cancels the job.
/// At 30s cadence, 20 intervals = 10 minutes of no progress → forced failure.
/// Must be greater than `STALE_STREAK_WARN_THRESHOLD`.
pub(crate) const STALE_STREAK_KILL_THRESHOLD: u32 = 20;

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

/// Spawn a content-aware heartbeat that:
/// 1. Touches `updated_at` every interval (keeps watchdog happy)
/// 2. Reads `result_json` and compares to previous snapshot
/// 3. Logs warning when content unchanged for `STALE_STREAK_WARN_THRESHOLD` intervals
/// 4. Cancels `kill_token` when content unchanged for `STALE_STREAK_KILL_THRESHOLD` intervals
///
/// Returns `(stop_tx, kill_token, handle)`:
/// - `stop_tx`: signal the heartbeat to stop gracefully (send `true` when job completes normally)
/// - `kill_token`: cancelled by the heartbeat when no progress detected for kill threshold;
///   callers should `select!` on `kill_token.cancelled()` to detect a forced kill
/// - `handle`: `JoinHandle` for the background task; await after stopping
///
/// # Usage
///
/// ```ignore
/// let (stop_tx, kill_token, hb) = spawn_content_aware_heartbeat(pool.clone(), TABLE, id, 30);
/// tokio::select! {
///     _ = inner_future => { let _ = stop_tx.send(true); }
///     _ = kill_token.cancelled() => { /* job killed — mark failed */ }
/// }
/// let _ = hb.await;
/// ```
pub fn spawn_content_aware_heartbeat(
    pool: PgPool,
    table: JobTable,
    id: Uuid,
    interval_secs: u64,
) -> (watch::Sender<bool>, CancellationToken, JoinHandle<()>) {
    let (stop_tx, mut stop_rx) = watch::channel(false);
    let kill_token = CancellationToken::new();
    let kill_token_inner = kill_token.clone();

    let handle = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(interval_secs));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut prev_snapshot: Option<serde_json::Value> = None;
        let mut stale_streak: u32 = 0;

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    // Touch updated_at AND read result_json in a single round-trip.
                    let curr = match touch_and_read_result_json(&pool, table, id).await {
                        Ok(value) => value,
                        Err(error) => {
                            log_warn(&format!(
                                "heartbeat read_result_json_failed job_id={id} table={} err={error}",
                                table.as_str(),
                            ));
                            continue;
                        }
                    };

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

                        if stale_streak >= STALE_STREAK_KILL_THRESHOLD {
                            let no_progress_secs = u64::from(stale_streak) * interval_secs;
                            log_warn(&format!(
                                "heartbeat kill_threshold_reached job_id={id} table={} \
                                 streak={stale_streak} no_progress_secs={no_progress_secs} \
                                 action=cancelling",
                                table.as_str(),
                            ));
                            kill_token_inner.cancel();
                            break;
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
    (stop_tx, kill_token, handle)
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

/// Touch `updated_at` and read `result_json` in a single round-trip.
/// Replaces the old two-query pattern (UPDATE + SELECT) to halve DB load per tick.
async fn touch_and_read_result_json(
    pool: &PgPool,
    table: JobTable,
    id: Uuid,
) -> Result<Option<serde_json::Value>, sqlx::Error> {
    let query = format!(
        "UPDATE {} SET updated_at = NOW() WHERE id = $1 RETURNING result_json",
        table.as_str()
    );
    let row = sqlx::query_scalar::<_, Option<serde_json::Value>>(&query)
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row.flatten())
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

    #[test]
    fn stale_streak_reaches_kill_threshold() {
        // Verify ordering at compile time via a const assertion instead of
        // a runtime assert!() on constant values, which clippy rejects.
        const _: () = assert!(
            STALE_STREAK_KILL_THRESHOLD > STALE_STREAK_WARN_THRESHOLD,
            "kill threshold must be greater than warn threshold",
        );
    }

    #[test]
    fn kill_threshold_is_bounded() {
        // Should kill after no more than 20 minutes at 30s cadence.
        let max_stall_secs = u64::from(STALE_STREAK_KILL_THRESHOLD) * 30;
        assert!(
            max_stall_secs <= 20 * 60,
            "kill threshold would allow stall of {max_stall_secs}s (>20min)",
        );
    }

    #[test]
    fn content_aware_heartbeat_detects_stale_content() {
        let snap1 = Some(serde_json::json!({"phase": "embedding_batch", "files_done": 150}));
        let snap2 = Some(serde_json::json!({"phase": "embedding_batch", "files_done": 150}));
        let snap3 = Some(serde_json::json!({"phase": "fetching_issues", "issues_fetched": 5}));

        assert!(
            is_content_stale(&snap1, &snap2),
            "identical snapshots should be stale"
        );
        assert!(
            !is_content_stale(&snap2, &snap3),
            "different snapshots should not be stale"
        );
    }
}
