//! Stale-job reclaim: the watchdog's recovery path.
//!
//! A job left in `running` by a crashed or hung worker is detected here (its
//! `updated_at` is older than the stale threshold) and either re-queued for
//! another attempt or — once it has been reclaimed `max_attempts` times —
//! dead-lettered (`failed`) so a job that crashes on every attempt cannot cycle
//! running→pending→running forever. Split out of `store.rs` to keep that file
//! under the monolith cap; `store.rs` re-exports the public surface so callers
//! still use `crate::jobs::store::*` paths.

use sqlx::SqlitePool;
use sqlx::sqlite::SqliteConnection;
use uuid::Uuid;

use super::now_ms;
use crate::jobs::backend::JobKind;
use crate::jobs::tx::ImmediateTx;

/// Error text written into `error_text` when the watchdog reclaims a stale running job.
///
/// All read sites import this constant so a one-character change never
/// silently breaks the renderer.
pub(crate) const RECLAIMED_ERROR_TEXT: &str = "reclaimed after unexpected shutdown";

/// A stale job attempt reclaimed by the watchdog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReclaimedJob {
    pub id: Uuid,
    pub attempt_id: Option<String>,
}

type ReclaimedRunningRow = (String, Option<String>, Option<String>, Option<String>, i64);

/// Per-kind reclaimed jobs returned by `reclaim_stale_running_jobs_detailed`.
#[derive(Debug, Default, Clone)]
pub struct ReclaimedJobs {
    pub crawl: Vec<ReclaimedJob>,
    pub embed: Vec<ReclaimedJob>,
    pub extract: Vec<ReclaimedJob>,
    pub ingest: Vec<ReclaimedJob>,
}

impl ReclaimedJobs {
    pub fn total(&self) -> usize {
        self.crawl.len() + self.embed.len() + self.extract.len() + self.ingest.len()
    }

    pub fn count_for(&self, kind: JobKind) -> usize {
        self.jobs_for(kind).len()
    }

    pub fn jobs_for(&self, kind: JobKind) -> &[ReclaimedJob] {
        match kind {
            JobKind::Crawl => &self.crawl,
            JobKind::Embed => &self.embed,
            JobKind::Extract => &self.extract,
            JobKind::Ingest => &self.ingest,
        }
    }
}

/// Reclaim jobs stuck in `running` state from a previous crashed process.
/// Returns the total count for backwards compatibility; the watchdog uses
/// `reclaim_stale_running_jobs_detailed` to drive per-kind worker wakeups.
pub async fn reclaim_stale_running_jobs(
    pool: &SqlitePool,
    stale_threshold_ms: i64,
    max_attempts: u32,
) -> Result<u64, sqlx::Error> {
    Ok(
        reclaim_stale_running_jobs_detailed(pool, stale_threshold_ms, max_attempts)
            .await
            .total() as u64,
    )
}

pub async fn reclaim_stale_running_jobs_detailed(
    pool: &SqlitePool,
    stale_threshold_ms: i64,
    max_attempts: u32,
) -> ReclaimedJobs {
    let mut reclaimed = ReclaimedJobs::default();
    for kind in JobKind::all() {
        match reclaim_stale_running_jobs_for_table_jobs(
            pool,
            *kind,
            stale_threshold_ms,
            max_attempts,
        )
        .await
        {
            Ok(jobs) => match kind {
                JobKind::Crawl => reclaimed.crawl = jobs,
                JobKind::Embed => reclaimed.embed = jobs,
                JobKind::Extract => reclaimed.extract = jobs,
                JobKind::Ingest => reclaimed.ingest = jobs,
            },
            Err(e) if crate::jobs::ops::is_lock_busy(&e) => {
                tracing::warn!(table = kind.table_name(), error = %e, "watchdog: per-table sweep skipped — DB busy");
            }
            Err(e) => {
                tracing::error!(table = kind.table_name(), error = %e, "watchdog: per-table sweep failed");
            }
        }
    }
    let total = reclaimed.total();
    if total > 0 {
        tracing::info!(reclaimed = total, "watchdog: sweep complete");
    }
    reclaimed
}

pub async fn reclaim_stale_running_jobs_for_table(
    pool: &SqlitePool,
    kind: JobKind,
    stale_threshold_ms: i64,
    max_attempts: u32,
) -> Result<u64, sqlx::Error> {
    Ok(
        reclaim_stale_running_jobs_for_table_ids(pool, kind, stale_threshold_ms, max_attempts)
            .await?
            .len() as u64,
    )
}

pub async fn reclaim_stale_running_jobs_for_table_ids(
    pool: &SqlitePool,
    kind: JobKind,
    stale_threshold_ms: i64,
    max_attempts: u32,
) -> Result<Vec<Uuid>, sqlx::Error> {
    Ok(
        reclaim_stale_running_jobs_for_table_jobs(pool, kind, stale_threshold_ms, max_attempts)
            .await?
            .into_iter()
            .map(|job| job.id)
            .collect(),
    )
}

pub async fn reclaim_stale_running_jobs_for_table_jobs(
    pool: &SqlitePool,
    kind: JobKind,
    stale_threshold_ms: i64,
    max_attempts: u32,
) -> Result<Vec<ReclaimedJob>, sqlx::Error> {
    // SAFETY: `kind.table_name()` returns a compile-time `&'static str` from
    // a closed enum dispatch; no caller-controlled value reaches `format!`.
    // Status literals come from a closed enum too.
    let table = kind.table_name();
    let threshold = now_ms() - stale_threshold_ms;
    let reclaimed_at = now_ms();
    let mut tx = ImmediateTx::begin(pool).await?;
    let reclaimed_rows: Vec<ReclaimedRunningRow> = match sqlx::query_as(&format!(
        "SELECT id, active_attempt_id, progress_json, result_json, attempt_count FROM {} WHERE status='running' AND updated_at < ?",
        table
    ))
    .bind(threshold)
    .fetch_all(tx.conn())
    .await
    {
        Ok(rows) => rows,
        Err(err) => {
            tx.rollback().await;
            return Err(err);
        }
    };
    if reclaimed_rows.is_empty() {
        tx.rollback().await;
        return Ok(Vec::new());
    }
    let mut updated_rows: Vec<(String, Option<String>)> = Vec::new();
    for (job_id, attempt_id, previous_progress_json, previous_result_json, attempt_count) in
        &reclaimed_rows
    {
        // A job that has already been reclaimed `max_attempts` times is
        // dead-lettered (marked failed) instead of re-queued, so a job that
        // crashes or hangs on every attempt cannot cycle forever. On any per-row
        // SQL error the `?` early-return drops `tx`, and the pool's
        // after_release hook rolls the whole sweep back.
        //
        // Dead-lettered rows are intentionally NOT added to `updated_rows` / the
        // returned `ReclaimedJobs`. That set drives lane wakeups and local
        // `CancellationToken` firing — neither of which a now-`failed` job needs:
        // a terminal job will not be re-claimed, and a row is only reclaimable
        // once its heartbeat has already stopped (its runner task, and thus the
        // token's await point, is gone or wedged), so firing the local token
        // would have no live observer.
        if max_attempts > 0 && *attempt_count >= max_attempts as i64 {
            dead_letter_stale_row(
                tx.conn(),
                table,
                job_id,
                *attempt_count,
                max_attempts,
                reclaimed_at,
                threshold,
            )
            .await?;
            continue;
        }
        let previous_progress = previous_progress_json
            .as_deref()
            .or(previous_result_json.as_deref());
        if requeue_stale_row(
            tx.conn(),
            table,
            job_id,
            previous_progress,
            reclaimed_at,
            threshold,
        )
        .await?
        {
            updated_rows.push((job_id.clone(), attempt_id.clone()));
        }
    }
    tx.commit().await?;
    Ok(finalize_reclaimed_jobs(table, updated_rows))
}

/// Parse the re-queued rows into `ReclaimedJob`s (dropping any with a corrupt
/// UUID) and log the sweep. Split out of
/// `reclaim_stale_running_jobs_for_table_jobs` to keep that function under the
/// line cap.
fn finalize_reclaimed_jobs(
    table: &str,
    updated_rows: Vec<(String, Option<String>)>,
) -> Vec<ReclaimedJob> {
    let jobs: Vec<ReclaimedJob> = updated_rows
        .into_iter()
        .filter_map(|(job_id, attempt_id)| match Uuid::parse_str(&job_id) {
            Ok(id) => Some(ReclaimedJob { id, attempt_id }),
            Err(e) => {
                tracing::warn!(table, raw = %job_id, error = %e, "watchdog: reclaimed row had corrupt UUID");
                None
            }
        })
        .collect();
    if !jobs.is_empty() {
        for job in &jobs {
            tracing::warn!(
                table,
                job_id = %job.id,
                attempt_id = job.attempt_id.as_deref().unwrap_or("unknown"),
                "watchdog: reclaimed stale running job and reset it to pending"
            );
        }
        tracing::info!(
            table,
            reclaimed = jobs.len(),
            "watchdog: reclaimed stale running jobs"
        );
    }
    jobs
}

/// Reset a single stale `running` row back to `pending` for another attempt.
/// Returns `true` when the row was actually updated; the `updated_at < ?`
/// re-check guards the race where the row finished between the SELECT and this
/// UPDATE.
async fn requeue_stale_row(
    conn: &mut SqliteConnection,
    table: &str,
    job_id: &str,
    previous_progress: Option<&str>,
    reclaimed_at: i64,
    threshold: i64,
) -> Result<bool, sqlx::Error> {
    let progress_json = requeued_progress_json(previous_progress);
    let result = sqlx::query(&format!(
        "UPDATE {} SET status='pending', error_text=?, progress_json=?, \
         result_json=NULL, updated_at=?, active_attempt_id=NULL, last_reclaimed_at=?, last_reclaimed_reason=? \
         WHERE id=? AND status='running' AND updated_at < ?",
        table
    ))
    .bind(RECLAIMED_ERROR_TEXT)
    .bind(progress_json.to_string())
    .bind(reclaimed_at)
    .bind(reclaimed_at)
    .bind("stale running job exceeded watchdog threshold")
    .bind(job_id)
    .bind(threshold)
    .execute(conn)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Dead-letter a single stale `running` row whose attempt count has reached the
/// configured cap: mark it `failed` (not `pending`) so a job that crashes or
/// hangs on every attempt stops cycling. Logged at ERROR so the give-up is
/// visible to operators.
async fn dead_letter_stale_row(
    conn: &mut SqliteConnection,
    table: &str,
    job_id: &str,
    attempt_count: i64,
    max_attempts: u32,
    reclaimed_at: i64,
    threshold: i64,
) -> Result<(), sqlx::Error> {
    let error_text = format!(
        "dead-lettered after {attempt_count} attempts (max_job_attempts={max_attempts}): \
         repeatedly reclaimed from a stale 'running' state without completing"
    );
    let progress_json = serde_json::json!({
        "phase": "failed",
        "lifecycle_progress": 1.0,
        "error": error_text,
    });
    let result = sqlx::query(&format!(
        "UPDATE {} SET status='failed', error_text=?, progress_json=?, \
         result_json=NULL, finished_at=?, updated_at=?, active_attempt_id=NULL, \
         last_reclaimed_at=?, last_reclaimed_reason=? \
         WHERE id=? AND status='running' AND updated_at < ?",
        table
    ))
    .bind(&error_text)
    .bind(progress_json.to_string())
    .bind(reclaimed_at)
    .bind(reclaimed_at)
    .bind(reclaimed_at)
    .bind("dead-lettered: exceeded max_job_attempts")
    .bind(job_id)
    .bind(threshold)
    .execute(conn)
    .await?;
    if result.rows_affected() > 0 {
        tracing::error!(
            table,
            job_id,
            attempt_count,
            max_attempts,
            "watchdog: dead-lettered job exceeding max_job_attempts — marked failed, not requeued"
        );
    }
    Ok(())
}

fn requeued_progress_json(previous_progress_json: Option<&str>) -> serde_json::Value {
    let previous_attempt_progress = previous_progress_json.and_then(|json| {
        serde_json::from_str::<serde_json::Value>(json)
            .map_err(|e| {
                tracing::warn!(error = %e, "watchdog: corrupt progress_json on reclaimed job");
                e
            })
            .ok()
    });
    serde_json::json!({
        "phase": "requeued",
        "lifecycle_progress": 0.0,
        "previous_attempt_progress": previous_attempt_progress,
    })
}
