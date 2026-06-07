use super::{
    WATCH_RUN_STATUS_COMPLETED, WATCH_RUN_STATUS_FAILED, WATCH_RUN_STATUS_RUNNING, WatchDef,
    WatchRun, create_watch_run_with_pool, finish_watch_run_with_pool, get_watch_run_with_pool,
    lease_watch_for_manual_run, orchestrate, watch_lease_ttl_ms_from_env,
};
use crate::core::config::Config;
use crate::jobs::store::{now_ms, open_config_pool};
use sqlx::SqlitePool;
use std::error::Error;
use std::time::Duration as StdDuration;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub async fn run_watch_now(cfg: &Config, watch: &WatchDef) -> Result<WatchRun, Box<dyn Error>> {
    let pool = open_config_pool(cfg).await?;
    run_watch_now_with_pool(cfg, &pool, watch).await
}

pub async fn run_watch_now_with_pool(
    cfg: &Config,
    pool: &SqlitePool,
    watch: &WatchDef,
) -> Result<WatchRun, Box<dyn Error>> {
    let leased =
        lease_watch_for_manual_run(pool, watch.id, now_ms(), watch_lease_ttl_ms_from_env())
            .await?
            .ok_or_else(|| {
                format!(
                    "watch {} is already running or disabled; wait for the active run to finish",
                    watch.id
                )
            })?;
    run_leased_watch_now_with_pool(cfg, pool, &leased).await
}

pub(crate) async fn run_leased_watch_now_with_pool(
    cfg: &Config,
    pool: &SqlitePool,
    watch: &WatchDef,
) -> Result<WatchRun, Box<dyn Error>> {
    let run = create_watch_run_with_pool(pool, watch.id, None).await?;
    let heartbeat_shutdown = CancellationToken::new();
    let heartbeat_handle = tokio::spawn(watch_run_heartbeat(
        pool.clone(),
        watch.id,
        run.id,
        watch_lease_ttl_ms_from_env(),
        heartbeat_shutdown.clone(),
    ));

    // Execute first (no DB writes), then finalize exactly once. `err_text` is a
    // `String`, not a boxed `dyn Error`, so the box never crosses an await and
    // the future stays `Send` for the axum runtime behind `/v1/watch/{id}/run`.
    // A COMPLETED write that fails falls through to the FAILED finalize below so
    // the run is never wedged in `running` — nothing reclaims stale runs.
    let outcome: Result<serde_json::Value, String> = run_watch_task(cfg, pool, run.id, watch).await;
    heartbeat_shutdown.cancel();
    let _ = heartbeat_handle.await;
    let err_text = match outcome {
        Ok(payload) => match finalize_completed(pool, watch, run.id, &payload).await {
            Ok(()) => return Ok(get_watch_run_with_pool(pool, run.id).await?.unwrap_or(run)),
            Err(text) => text,
        },
        Err(text) => text,
    };
    if let Err(persist_err) = finish_watch_run_with_pool(
        pool,
        watch.id,
        run.id,
        WATCH_RUN_STATUS_FAILED,
        None,
        Some(&err_text),
    )
    .await
    {
        // The FAILED-status write itself failed: the run row stays in `running`
        // and nothing reclaims stale watch runs, so it is wedged permanently.
        // Surface why instead of dropping the error silently.
        tracing::warn!(
            watch_id = %watch.id,
            run_id = %run.id,
            persist_error = %persist_err,
            task_error = %err_text,
            "watch run: FAILED-status write failed; run may be wedged in running",
        );
    }
    Err(err_text.into())
}

async fn watch_run_heartbeat(
    pool: SqlitePool,
    watch_id: Uuid,
    run_id: Uuid,
    lease_ttl_ms: i64,
    shutdown: CancellationToken,
) {
    let heartbeat_ms = (lease_ttl_ms / 3).clamp(1_000, 30_000) as u64;
    let mut interval = tokio::time::interval(StdDuration::from_millis(heartbeat_ms));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = shutdown.cancelled() => break,
            _ = interval.tick() => {
                let now = now_ms();
                let lease_until = now + lease_ttl_ms;
                if let Err(err) = sqlx::query(
                    "UPDATE axon_watch_defs \
                     SET lease_expires_at = ?, updated_at = ? \
                     WHERE id = ? AND lease_expires_at IS NOT NULL",
                )
                .bind(lease_until)
                .bind(now)
                .bind(watch_id.to_string())
                .execute(&pool)
                .await
                {
                    tracing::warn!(watch_id = %watch_id, run_id = %run_id, error = %err, "watch heartbeat: failed to extend lease");
                }

                if let Err(err) = sqlx::query(
                    "UPDATE axon_watch_runs \
                     SET updated_at = ? \
                     WHERE id = ? AND status = ?",
                )
                .bind(now)
                .bind(run_id.to_string())
                .bind(WATCH_RUN_STATUS_RUNNING)
                .execute(&pool)
                .await
                {
                    tracing::warn!(watch_id = %watch_id, run_id = %run_id, error = %err, "watch heartbeat: failed to touch run");
                }
            }
        }
    }
}

/// Persist a COMPLETED run, mapping any error to a `String` so the non-`Send` box is dropped before the caller's next await.
async fn finalize_completed(
    pool: &SqlitePool,
    watch: &WatchDef,
    run_id: Uuid,
    payload: &serde_json::Value,
) -> Result<(), String> {
    finish_watch_run_with_pool(
        pool,
        watch.id,
        run_id,
        WATCH_RUN_STATUS_COMPLETED,
        Some(payload),
        None,
    )
    .await
    .map(|_| ())
    .map_err(|err| err.to_string())
}

/// Execute a watch's task → result payload, or a human-readable failure message.
/// Pure compute + scrape; the caller owns the single finalize write. Receives
/// the caller's `pool` and the real `run_id` so the orchestrator never has to
/// re-derive the current run (which was racy when a `run-now` overlapped a
/// scheduled run) or open a fresh per-run pool.
async fn run_watch_task(
    cfg: &Config,
    pool: &SqlitePool,
    run_id: Uuid,
    watch: &WatchDef,
) -> Result<serde_json::Value, String> {
    match watch.task_type.as_str() {
        // "refresh" is the prior release's task_type for this same handler.
        // Accepted here (EXECUTION only) for back-compat so persisted rows keep
        // running; new creates still require "watch" (see SUPPORTED_TASK_TYPES).
        "watch" | "refresh" => orchestrate::run_url_watch(cfg, pool, run_id, watch).await,
        other => Err(format!("unsupported watch task_type: {other}")),
    }
}
