use axon_api::source::{AuthSnapshot, SourceId, WatchId, WatchRequest};
use sqlx::{Row, sqlite::SqliteRow};

use super::mutations::retry_watch_write;
use super::rows::{row_to_auth_snapshot, row_to_request, sqlite_err};
use super::{Result, SqliteWatchStore};
use crate::store::now_ms;

#[derive(Debug, Clone)]
pub(crate) struct LeasedSourceWatch {
    pub watch_id: WatchId,
    pub source_id: SourceId,
    pub request: WatchRequest,
    pub auth_snapshot: Option<AuthSnapshot>,
}

impl SqliteWatchStore {
    /// Acquire the same durable per-watch execution lease used by the due
    /// scheduler for an explicit `watch exec` request.
    ///
    /// The conditional UPDATE is the authority: concurrent callers cannot
    /// both pass it, even when they are served by different Axon processes.
    pub async fn acquire_exec_lease(&self, watch_id: &WatchId, lease_ttl_ms: i64) -> Result<bool> {
        let now = now_ms();
        retry_watch_write("watch acquire exec lease", || async {
            let updated = sqlx::query(
                "UPDATE axon_source_watches SET lease_expires_at = ?, updated_at = ? \
                 WHERE watch_id = ? \
                   AND (lease_expires_at IS NULL OR lease_expires_at < ?) \
                   AND NOT EXISTS ( \
                       SELECT 1 FROM axon_source_watch_runs r \
                       JOIN jobs j ON j.job_id = r.job_id \
                       WHERE r.watch_id = ? \
                         AND j.status NOT IN ('completed', 'completed_degraded', 'failed', 'canceled', 'expired', 'skipped') \
                   )",
            )
            .bind(now + lease_ttl_ms)
            .bind(now)
            .bind(&watch_id.0)
            .bind(now)
            .bind(&watch_id.0)
            .execute(&self.pool)
            .await
            .map_err(sqlite_err)?;
            Ok(updated.rows_affected() == 1)
        })
        .await
    }

    /// Atomically lease enabled source watches whose schedule is due.
    ///
    /// This is the canonical recurring scheduler path. It reads only
    /// `axon_source_watches` and uses `axon_source_watch_runs` + `jobs` to avoid
    /// enqueueing a duplicate while a previous source job for the same watch is
    /// still live.
    pub(crate) async fn lease_due(
        &self,
        now: i64,
        lease_ttl_ms: i64,
        limit: i64,
    ) -> Result<Vec<LeasedSourceWatch>> {
        // The atomic UPDATE below must acquire SQLite's single-writer lock
        // even when it ultimately returns zero rows. Most scheduler ticks have
        // no due watches, so avoid joining unrelated write contention on that
        // common path. This read is only a conservative fast-path hint: when
        // it finds a candidate, the UPDATE remains the sole lease authority
        // and rechecks every eligibility predicate atomically.
        let has_due_watch = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS( \
                 SELECT 1 FROM axon_source_watches \
                 WHERE enabled = 1 AND every_seconds >= 30 AND next_run_at <= ? \
                   AND (lease_expires_at IS NULL OR lease_expires_at < ?) \
                 LIMIT 1 \
             )",
        )
        .bind(now)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(sqlite_err)?;
        if !has_due_watch {
            return Ok(Vec::new());
        }

        retry_watch_write("watch lease due", || {
            self.lease_due_once(now, lease_ttl_ms, limit)
        })
        .await
    }

    async fn lease_due_once(
        &self,
        now: i64,
        lease_ttl_ms: i64,
        limit: i64,
    ) -> Result<Vec<LeasedSourceWatch>> {
        let lease_until = now + lease_ttl_ms;
        let rows = sqlx::query(
            "UPDATE axon_source_watches \
             SET lease_expires_at = ?, next_run_at = ? + (every_seconds * 1000), updated_at = ? \
             WHERE watch_id IN ( \
                 SELECT w.watch_id FROM axon_source_watches w \
                 WHERE w.enabled = 1 AND w.every_seconds >= 30 AND w.next_run_at <= ? \
                   AND (w.lease_expires_at IS NULL OR w.lease_expires_at < ?) \
                   AND w.watch_id = ( \
                       SELECT w2.watch_id FROM axon_source_watches w2 \
                       WHERE w2.source_id = w.source_id \
                         AND w2.enabled = 1 AND w2.every_seconds >= 30 \
                         AND w2.next_run_at <= ? \
                         AND (w2.lease_expires_at IS NULL OR w2.lease_expires_at < ?) \
                       ORDER BY w2.next_run_at ASC, w2.watch_id ASC LIMIT 1 \
                   ) \
                   AND NOT EXISTS ( \
                       SELECT 1 FROM axon_source_watch_runs r \
                       JOIN jobs j ON j.job_id = r.job_id \
                       JOIN axon_source_watches active_w ON active_w.watch_id = r.watch_id \
                       WHERE active_w.source_id = w.source_id \
                         AND j.status NOT IN ('completed', 'completed_degraded', 'failed', 'canceled', 'expired', 'skipped') \
                   ) \
                   AND NOT EXISTS ( \
                       SELECT 1 FROM jobs j \
                       WHERE j.idempotency_key IS NOT NULL \
                         AND substr(j.idempotency_key, 1, length('source-watch:' || w.watch_id || ':')) = 'source-watch:' || w.watch_id || ':' \
                         AND j.status NOT IN ('completed', 'completed_degraded', 'failed', 'canceled', 'expired', 'skipped') \
                   ) \
                 ORDER BY w.next_run_at ASC LIMIT ? \
             ) \
             RETURNING *",
        )
        .bind(lease_until)
        .bind(now)
        .bind(now)
        .bind(now)
        .bind(now)
        .bind(now)
        .bind(now)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(sqlite_err)?;

        rows.iter().map(row_to_leased_source_watch).collect()
    }

    /// Release a scheduler lease after an enqueue attempt failed before a job
    /// could be recorded. `next_run_at` has already moved forward at lease
    /// time, so this avoids a tight retry loop while still allowing the next
    /// scheduled interval to run.
    pub async fn release_lease(&self, watch_id: &WatchId) -> Result<()> {
        retry_watch_write("watch release lease", || self.release_lease_once(watch_id)).await
    }

    async fn release_lease_once(&self, watch_id: &WatchId) -> Result<()> {
        let now = now_ms();
        sqlx::query(
            "UPDATE axon_source_watches SET lease_expires_at = NULL, updated_at = ? \
             WHERE watch_id = ?",
        )
        .bind(now)
        .bind(&watch_id.0)
        .execute(&self.pool)
        .await
        .map_err(sqlite_err)?;
        Ok(())
    }
}

fn row_to_leased_source_watch(row: &SqliteRow) -> Result<LeasedSourceWatch> {
    let watch_id = WatchId::new(row.get::<String, _>("watch_id"));
    let source_id = SourceId::new(row.get::<String, _>("source_id"));
    let request = row_to_request(row)?;
    let auth_snapshot = row_to_auth_snapshot(row)?;
    Ok(LeasedSourceWatch {
        watch_id,
        source_id,
        request,
        auth_snapshot,
    })
}
