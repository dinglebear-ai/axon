//! SQLite-backed [`crate::boundary::WatchStore`] for source-request-backed
//! watches (WS-B / issue #298). This uses the canonical `axon_source_watches`
//! / `axon_source_watch_runs` table pair, not the retired `axon_watch_defs`.
//! `delete` stays inherent on [`SqliteWatchStore`] because the shared trait
//! does not include deletion.

use async_trait::async_trait;
use axon_api::source::*;
use sqlx::{Row, SqlitePool};

use crate::boundary::{Result, WatchStore};
use crate::limits::clamp_page_limit;
use crate::store::now_ms;
use crate::unified::pagination::{
    WatchCursor, WatchHistoryCursor, decode_watch_cursor, decode_watch_history_cursor,
    encode_watch_cursor, encode_watch_history_cursor,
};

#[path = "watch_store_scheduler.rs"]
mod scheduler;
pub(crate) use scheduler::LeasedSourceWatch;

#[path = "watch_store_rows.rs"]
mod rows;
use rows::{
    json_err, missing_job, missing_watch, row_to_auth_snapshot, row_to_request, row_to_result,
    row_to_summary, scope_to_str, sqlite_err, synth_descriptor, validate_source_watch_interval,
};

/// SQLite-backed [`WatchStore`]. Cheap to clone (wraps a pooled connection
/// handle); safe to share across worker tasks.
#[derive(Debug, Clone)]
pub struct SqliteWatchStore {
    pool: SqlitePool,
}

impl SqliteWatchStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Reconstruct the original request fields stored for a watch.
    pub async fn request(&self, watch_id: WatchId) -> Result<Option<WatchRequest>> {
        let row = sqlx::query("SELECT * FROM axon_source_watches WHERE watch_id = ?")
            .bind(&watch_id.0)
            .fetch_optional(&self.pool)
            .await
            .map_err(sqlite_err)?;
        row.as_ref().map(row_to_request).transpose()
    }

    pub async fn request_with_auth(
        &self,
        watch_id: WatchId,
    ) -> Result<Option<(WatchRequest, Option<AuthSnapshot>)>> {
        let row = sqlx::query("SELECT * FROM axon_source_watches WHERE watch_id = ?")
            .bind(&watch_id.0)
            .fetch_optional(&self.pool)
            .await
            .map_err(sqlite_err)?;
        row.as_ref()
            .map(|row| Ok((row_to_request(row)?, row_to_auth_snapshot(row)?)))
            .transpose()
    }

    /// Find the newest canonical watch whose stored source/canonical URI
    /// matches `source`.
    pub async fn find_by_source(&self, source: &str) -> Result<Option<WatchResult>> {
        let source = source.trim();
        if source.is_empty() {
            return Ok(None);
        }
        let row = sqlx::query(
            "SELECT * FROM axon_source_watches \
             WHERE source = ? OR canonical_uri = ? \
             ORDER BY created_at DESC, watch_id DESC LIMIT 1",
        )
        .bind(source)
        .bind(source)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlite_err)?;
        Ok(row.as_ref().map(row_to_result))
    }

    pub async fn create_with_auth(
        &self,
        request: WatchRequest,
        auth_snapshot: Option<AuthSnapshot>,
    ) -> Result<WatchResult> {
        self.insert_watch(request, auth_snapshot).await
    }

    pub async fn create_resolved_with_auth(
        &self,
        request: WatchRequest,
        source_id: SourceId,
        canonical_uri: String,
        adapter: AdapterRef,
        auth_snapshot: Option<AuthSnapshot>,
    ) -> Result<WatchResult> {
        self.insert_resolved_watch(request, auth_snapshot, source_id, canonical_uri, adapter)
            .await
    }

    /// Hard-delete a watch and its run history (`ON DELETE CASCADE`).
    /// Returns `true` if a row was deleted, `false` if the watch didn't exist.
    pub async fn delete(&self, watch_id: WatchId) -> Result<bool> {
        let deleted = sqlx::query("DELETE FROM axon_source_watches WHERE watch_id = ?")
            .bind(&watch_id.0)
            .execute(&self.pool)
            .await
            .map_err(sqlite_err)?
            .rows_affected();
        Ok(deleted > 0)
    }
}

impl SqliteWatchStore {
    async fn insert_watch(
        &self,
        request: WatchRequest,
        auth_snapshot: Option<AuthSnapshot>,
    ) -> Result<WatchResult> {
        let source_id = SourceId::new(format!("source_{}", uuid::Uuid::new_v4()));
        let canonical_uri = request.source.clone();
        let adapter = AdapterRef {
            name: "sqlite-watch-store".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        };
        self.insert_resolved_watch(request, auth_snapshot, source_id, canonical_uri, adapter)
            .await
    }

    async fn insert_resolved_watch(
        &self,
        request: WatchRequest,
        auth_snapshot: Option<AuthSnapshot>,
        source_id: SourceId,
        canonical_uri: String,
        adapter: AdapterRef,
    ) -> Result<WatchResult> {
        let watch_id = WatchId::new(format!("watch_{}", uuid::Uuid::new_v4()));
        let scope = request.scope.unwrap_or(SourceScope::Page);
        let enabled = request.enabled.unwrap_or(true);
        let every_seconds = validate_source_watch_interval(request.schedule.every_seconds)?;
        let options_json = serde_json::to_string(&request.options).map_err(json_err)?;
        let auth_snapshot_json = auth_snapshot
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(json_err)?;
        let now = now_ms();
        let next_run_at = now + every_seconds * 1000;

        sqlx::query(
            "INSERT INTO axon_source_watches \
             (watch_id, source, source_id, canonical_uri, adapter_name, adapter_version, \
              scope, embed, options_json, collection, enabled, every_seconds, cron, timezone, \
              next_run_at, last_job_id, last_status, created_at, updated_at, auth_snapshot_json) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&watch_id.0)
        .bind(&request.source)
        .bind(&source_id.0)
        .bind(&canonical_uri)
        .bind(&adapter.name)
        .bind(&adapter.version)
        .bind(scope_to_str(scope))
        .bind(request.embed)
        .bind(&options_json)
        .bind(&request.collection)
        .bind(enabled)
        .bind(every_seconds)
        .bind(&request.schedule.cron)
        .bind(&request.schedule.timezone)
        .bind(next_run_at)
        .bind(None::<String>)
        .bind(None::<String>)
        .bind(now)
        .bind(now)
        .bind(&auth_snapshot_json)
        .execute(&self.pool)
        .await
        .map_err(sqlite_err)?;

        Ok(WatchResult {
            watch_id,
            source_id,
            canonical_uri,
            adapter,
            scope,
            enabled,
            schedule: request.schedule,
            job: None,
            latest_job: None,
            warnings: Vec::new(),
        })
    }
}

#[async_trait]
impl WatchStore for SqliteWatchStore {
    async fn create(&self, request: WatchRequest) -> Result<WatchResult> {
        self.insert_watch(request, None).await
    }

    async fn update(&self, watch_id: WatchId, request: WatchUpdateRequest) -> Result<WatchResult> {
        let existing = sqlx::query("SELECT * FROM axon_source_watches WHERE watch_id = ?")
            .bind(&watch_id.0)
            .fetch_optional(&self.pool)
            .await
            .map_err(sqlite_err)?
            .ok_or_else(|| missing_watch(&watch_id))?;

        let mut every_seconds: i64 = existing.get("every_seconds");
        let mut cron: Option<String> = existing.get("cron");
        let mut timezone: Option<String> = existing.get("timezone");
        if let Some(schedule) = &request.schedule {
            every_seconds = validate_source_watch_interval(schedule.every_seconds)?;
            cron = schedule.cron.clone();
            timezone = schedule.timezone.clone();
        }

        let enabled: i64 = request
            .enabled
            .map(|value| if value { 1 } else { 0 })
            .unwrap_or_else(|| existing.get("enabled"));

        let embed: i64 = match request.embed {
            Some(value) => {
                if value {
                    1
                } else {
                    0
                }
            }
            None => existing.get::<i64, _>("embed"),
        };

        let options_json: String = match &request.options {
            Some(options) => serde_json::to_string(options).map_err(json_err)?,
            None => existing.get("options_json"),
        };

        let collection: Option<String> = request
            .collection
            .clone()
            .or_else(|| existing.get::<Option<String>, _>("collection"));

        let scope: String = match request.scope {
            Some(scope) => scope_to_str(scope),
            None => existing.get("scope"),
        };

        let now = now_ms();
        let next_run_at: i64 = if request.schedule.is_some() {
            now + every_seconds * 1000
        } else {
            existing.get("next_run_at")
        };
        sqlx::query(
            "UPDATE axon_source_watches \
             SET enabled = ?, every_seconds = ?, cron = ?, timezone = ?, embed = ?, \
                 options_json = ?, collection = ?, scope = ?, next_run_at = ?, updated_at = ? \
             WHERE watch_id = ?",
        )
        .bind(enabled)
        .bind(every_seconds)
        .bind(&cron)
        .bind(&timezone)
        .bind(embed)
        .bind(&options_json)
        .bind(&collection)
        .bind(&scope)
        .bind(next_run_at)
        .bind(now)
        .bind(&watch_id.0)
        .execute(&self.pool)
        .await
        .map_err(sqlite_err)?;

        self.get(watch_id.clone())
            .await?
            .ok_or_else(|| missing_watch(&watch_id))
    }

    async fn get(&self, watch_id: WatchId) -> Result<Option<WatchResult>> {
        let row = sqlx::query("SELECT * FROM axon_source_watches WHERE watch_id = ?")
            .bind(&watch_id.0)
            .fetch_optional(&self.pool)
            .await
            .map_err(sqlite_err)?;
        Ok(row.as_ref().map(row_to_result))
    }

    async fn list(&self, request: WatchListRequest) -> Result<Page<WatchSummary>> {
        let cursor = request
            .cursor
            .as_deref()
            .map(decode_watch_cursor)
            .transpose()
            .map_err(|message| {
                ApiError::new("watch.cursor_invalid", ErrorStage::Retrieving, message)
            })?;

        let mut where_sql = String::from(" WHERE 1 = 1");
        if request.enabled.is_some() {
            where_sql.push_str(" AND enabled = ?");
        }
        if request.source_id.is_some() {
            where_sql.push_str(" AND source_id = ?");
        }
        if request.adapter.is_some() {
            where_sql.push_str(" AND adapter_name = ?");
        }
        if cursor.is_some() {
            where_sql.push_str(" AND (created_at < ? OR (created_at = ? AND watch_id < ?))");
        }
        let count_sql = format!("SELECT COUNT(*) FROM axon_source_watches{where_sql}");
        let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql);
        if let Some(enabled) = request.enabled {
            count_query = count_query.bind(if enabled { 1 } else { 0 });
        }
        if let Some(source_id) = &request.source_id {
            count_query = count_query.bind(&source_id.0);
        }
        if let Some(adapter) = &request.adapter {
            count_query = count_query.bind(adapter);
        }
        let total = if cursor.is_none() {
            Some(
                count_query
                    .fetch_one(&self.pool)
                    .await
                    .map_err(sqlite_err)? as u64,
            )
        } else {
            None
        };

        let limit = clamp_page_limit(request.limit);
        let list_sql = format!(
            "SELECT * FROM axon_source_watches{where_sql} \
             ORDER BY created_at DESC, watch_id DESC LIMIT ?"
        );
        let mut query = sqlx::query(&list_sql);
        if let Some(enabled) = request.enabled {
            query = query.bind(if enabled { 1 } else { 0 });
        }
        if let Some(source_id) = &request.source_id {
            query = query.bind(&source_id.0);
        }
        if let Some(adapter) = &request.adapter {
            query = query.bind(adapter);
        }
        if let Some(cursor) = cursor.as_ref() {
            query = query
                .bind(cursor.created_at)
                .bind(cursor.created_at)
                .bind(&cursor.watch_id);
        }
        query = query.bind((limit + 1) as i64);

        let rows = query.fetch_all(&self.pool).await.map_err(sqlite_err)?;
        let has_more = rows.len() > limit as usize;
        let mut items = rows.iter().map(row_to_summary).collect::<Vec<_>>();
        if has_more {
            items.truncate(limit as usize);
        }
        let next_cursor = if has_more {
            rows.get(limit as usize - 1).map(|row| {
                encode_watch_cursor(&WatchCursor {
                    created_at: row.get("created_at"),
                    watch_id: row.get("watch_id"),
                })
            })
        } else {
            None
        };

        Ok(Page {
            total,
            limit,
            next_cursor,
            items,
        })
    }

    async fn record_run(&self, watch_id: WatchId, job_id: JobId) -> Result<()> {
        let watch_exists =
            sqlx::query_scalar::<_, i64>("SELECT 1 FROM axon_source_watches WHERE watch_id = ?")
                .bind(&watch_id.0)
                .fetch_optional(&self.pool)
                .await
                .map_err(sqlite_err)?
                .is_some();
        if !watch_exists {
            return Err(missing_watch(&watch_id));
        }

        let job_row = sqlx::query("SELECT kind, status FROM jobs WHERE job_id = ?")
            .bind(job_id.0.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(sqlite_err)?
            .ok_or_else(|| missing_job(job_id))?;
        let status: String = job_row.get("status");

        let now = now_ms();
        sqlx::query(
            "INSERT INTO axon_source_watch_runs (watch_id, job_id, created_at) VALUES (?, ?, ?)",
        )
        .bind(&watch_id.0)
        .bind(job_id.0.to_string())
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(sqlite_err)?;

        sqlx::query(
            "UPDATE axon_source_watches \
             SET last_job_id = ?, last_status = ?, lease_expires_at = NULL, updated_at = ? \
             WHERE watch_id = ?",
        )
        .bind(job_id.0.to_string())
        .bind(&status)
        .bind(now)
        .bind(&watch_id.0)
        .execute(&self.pool)
        .await
        .map_err(sqlite_err)?;

        Ok(())
    }

    async fn history(&self, request: WatchHistoryRequest) -> Result<WatchHistoryResult> {
        let watch_exists =
            sqlx::query_scalar::<_, i64>("SELECT 1 FROM axon_source_watches WHERE watch_id = ?")
                .bind(&request.watch_id.0)
                .fetch_optional(&self.pool)
                .await
                .map_err(sqlite_err)?
                .is_some();
        if !watch_exists {
            return Err(missing_watch(&request.watch_id));
        }

        let limit = clamp_page_limit(request.limit);
        let cursor = request
            .cursor
            .as_deref()
            .map(decode_watch_history_cursor)
            .transpose()
            .map_err(|message| {
                ApiError::new("watch.cursor_invalid", ErrorStage::Retrieving, message)
            })?;
        let mut sql = String::from(
            "SELECT r.id AS run_id, r.created_at AS run_created_at, \
                    r.job_id AS job_id, j.status AS status \
             FROM axon_source_watch_runs r \
             LEFT JOIN jobs j ON j.job_id = r.job_id \
             WHERE r.watch_id = ?",
        );
        if request.status.is_some() {
            sql.push_str(" AND j.status = ?");
        }
        if cursor.is_some() {
            sql.push_str(" AND (r.created_at < ? OR (r.created_at = ? AND r.id < ?))");
        }
        sql.push_str(" ORDER BY r.created_at DESC, r.id DESC LIMIT ?");
        let mut query = sqlx::query(&sql).bind(&request.watch_id.0);
        if let Some(status) = request.status {
            let status = serde_json::to_value(status)
                .ok()
                .and_then(|value| value.as_str().map(str::to_string))
                .unwrap_or_default();
            query = query.bind(status);
        }
        if let Some(cursor) = cursor.as_ref() {
            query = query
                .bind(cursor.created_at)
                .bind(cursor.created_at)
                .bind(cursor.run_id);
        }
        let rows = query
            .bind((limit + 1) as i64)
            .fetch_all(&self.pool)
            .await
            .map_err(sqlite_err)?;

        let has_more = rows.len() > limit as usize;
        let jobs = rows
            .iter()
            .take(limit as usize)
            .map(|row| {
                let job_id: String = row.get("job_id");
                let status: Option<String> = row.get("status");
                synth_descriptor(&job_id, status.as_deref())
            })
            .collect();
        let next_cursor = if has_more {
            rows.get(limit as usize - 1).map(|row| {
                encode_watch_history_cursor(&WatchHistoryCursor {
                    created_at: row.get("run_created_at"),
                    run_id: row.get("run_id"),
                    job_id: None,
                })
            })
        } else {
            None
        };

        Ok(WatchHistoryResult {
            watch_id: request.watch_id,
            jobs,
            next_cursor,
        })
    }

    async fn reset(&self) -> Result<()> {
        sqlx::query("DELETE FROM axon_source_watches")
            .execute(&self.pool)
            .await
            .map_err(sqlite_err)?;
        Ok(())
    }

    async fn capabilities(&self) -> Result<WatchStoreCapability> {
        Ok(CapabilityBase {
            name: "sqlite-watch-store".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            owner_crate: "axon-jobs".to_string(),
            health: HealthStatus::Healthy,
            features: vec!["sqlite".to_string()],
            limits: MetadataMap::new(),
        }
        .into())
    }
}

#[cfg(test)]
#[path = "watch_store_tests.rs"]
mod tests;
