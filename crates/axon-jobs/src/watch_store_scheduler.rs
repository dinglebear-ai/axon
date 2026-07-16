use axon_api::source::{AuthSnapshot, SourceId, WatchId, WatchRequest};
use sqlx::{Row, sqlite::SqliteRow};

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
    pub(crate) async fn release_lease(&self, watch_id: &WatchId) -> Result<()> {
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
