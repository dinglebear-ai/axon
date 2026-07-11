//! `SqliteWatchStore` — the real, SQLite-backed [`crate::boundary::WatchStore`]
//! implementation for source-request-backed watches (WS-B / audit C4-04,
//! issue #298 contract: `docs/pipeline-unification/sources/source-pipeline.md`
//! Transport Crosswalk: watch).
//!
//! This is intentionally a NEW table pair (`axon_source_watches` /
//! `axon_source_watch_runs`, migration `0023`), not a rewrite of the legacy
//! `axon_watch_defs`/`axon_watch_runs` tables (migration `0002`) that back the
//! still-live `axon watch create|list|history|exec` task_type/task_payload
//! model and its scheduler
//! (`crates/axon-jobs/src/workers/watch_scheduler.rs`). Disturbing those was
//! explicitly out of scope for this slice. `SqliteWatchStore` models a watch
//! the way the contract does: a `source` string plus a `WatchSchedule` and
//! `AdapterOptions`, matching `axon_api::source::{WatchRequest, WatchResult}`.
//!
//! `delete` is intentionally an inherent method on the concrete
//! `SqliteWatchStore`, not part of the shared `WatchStore` trait — the trait
//! is owned by `axon-api`/`axon-jobs::boundary` and this slice does not edit
//! trait contracts. Callers that need delete (the CLI) hold the concrete type
//! rather than `Arc<dyn WatchStore>`.

use async_trait::async_trait;
use axon_api::source::*;
use sqlx::{Row, SqlitePool, sqlite::SqliteRow};

use crate::boundary::{Result, WatchStore};
use crate::limits::clamp_page_limit;
use crate::store::now_ms;

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

fn sqlite_err(err: sqlx::Error) -> ApiError {
    ApiError::new(
        "watch.storage_error",
        ErrorStage::Retrieving,
        format!("watch store error: {err}"),
    )
}

fn json_err(err: serde_json::Error) -> ApiError {
    ApiError::new(
        "watch.storage_error",
        ErrorStage::Retrieving,
        format!("watch store serialization error: {err}"),
    )
}

fn missing_watch(watch_id: &WatchId) -> ApiError {
    ApiError::new(
        "watch.not_found",
        ErrorStage::Retrieving,
        format!("watch {} not found", watch_id.0),
    )
}

fn missing_job(job_id: JobId) -> ApiError {
    ApiError::new(
        "job.not_found",
        ErrorStage::Retrieving,
        format!("job {} not found", job_id.0),
    )
}

fn parse_json_str<T: serde::de::DeserializeOwned>(raw: &str) -> Option<T> {
    serde_json::from_value(serde_json::Value::String(raw.to_string())).ok()
}

fn parse_scope(raw: &str) -> SourceScope {
    parse_json_str(raw).unwrap_or(SourceScope::Page)
}

/// Serialize a `SourceScope` to the bare snake_case string this store persists
/// (e.g. `"page"`, not the JSON-quoted `"\"page\""` `serde_json::to_string`
/// would produce) so it round-trips through [`parse_scope`] unchanged.
fn scope_to_str(scope: SourceScope) -> String {
    serde_json::to_value(scope)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "page".to_string())
}

fn row_to_result(row: &SqliteRow) -> WatchResult {
    let watch_id = WatchId::new(row.get::<String, _>("watch_id"));
    let source_id = SourceId::new(row.get::<String, _>("source_id"));
    let canonical_uri: String = row.get("canonical_uri");
    let adapter = AdapterRef {
        name: row.get("adapter_name"),
        version: row.get("adapter_version"),
    };
    let scope = parse_scope(&row.get::<String, _>("scope"));
    let enabled: i64 = row.get("enabled");
    let every_seconds: i64 = row.get("every_seconds");
    let cron: Option<String> = row.get("cron");
    let timezone: Option<String> = row.get("timezone");
    let last_job_id: Option<String> = row.get("last_job_id");
    let last_status: Option<String> = row.get("last_status");
    let latest_job = last_job_id.map(|job_id| synth_descriptor(&job_id, last_status.as_deref()));

    WatchResult {
        watch_id,
        source_id,
        canonical_uri,
        adapter,
        scope,
        enabled: enabled != 0,
        schedule: WatchSchedule {
            every_seconds: every_seconds.max(0) as u64,
            cron,
            timezone,
        },
        job: None,
        latest_job,
        warnings: Vec::new(),
    }
}

fn synth_descriptor(job_id: &str, status: Option<&str>) -> JobDescriptor {
    let status = status
        .and_then(parse_json_str::<LifecycleStatus>)
        .unwrap_or(LifecycleStatus::Queued);
    let job_id = JobId::new(uuid::Uuid::parse_str(job_id).unwrap_or_default());
    JobDescriptor {
        kind: JobKind::Source,
        id: job_id,
        status_url: format!("/v1/jobs/{}", job_id.0),
        events_url: format!("/v1/jobs/{}/events", job_id.0),
        stream_url: format!("/v1/jobs/{}/stream", job_id.0),
        poll_after_ms: 1000,
        cancel_url: Some(format!("/v1/jobs/{}/cancel", job_id.0)),
        retry_url: Some(format!("/v1/jobs/{}/retry", job_id.0)),
        job_id,
        status,
        poll: None,
        created_at: None,
        updated_at: None,
    }
}

#[async_trait]
impl WatchStore for SqliteWatchStore {
    async fn create(&self, request: WatchRequest) -> Result<WatchResult> {
        let watch_id = WatchId::new(format!("watch_{}", uuid::Uuid::new_v4()));
        let source_id = SourceId::new(format!("source_{}", uuid::Uuid::new_v4()));
        let adapter = AdapterRef {
            name: "sqlite-watch-store".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        };
        let scope = request.scope.unwrap_or(SourceScope::Page);
        let enabled = request.enabled.unwrap_or(true);
        let options_json = serde_json::to_string(&request.options).map_err(json_err)?;
        let now = now_ms();
        let next_run_at = now + (request.schedule.every_seconds as i64) * 1000;

        sqlx::query(
            "INSERT INTO axon_source_watches \
             (watch_id, source, source_id, canonical_uri, adapter_name, adapter_version, \
              scope, embed, options_json, collection, enabled, every_seconds, cron, timezone, \
              next_run_at, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&watch_id.0)
        .bind(&request.source)
        .bind(&source_id.0)
        .bind(&request.source)
        .bind(&adapter.name)
        .bind(&adapter.version)
        .bind(scope_to_str(scope))
        .bind(request.embed)
        .bind(&options_json)
        .bind(&request.collection)
        .bind(enabled)
        .bind(request.schedule.every_seconds as i64)
        .bind(&request.schedule.cron)
        .bind(&request.schedule.timezone)
        .bind(next_run_at)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(sqlite_err)?;

        Ok(WatchResult {
            watch_id,
            source_id,
            canonical_uri: request.source,
            adapter,
            scope,
            enabled,
            schedule: request.schedule,
            job: None,
            latest_job: None,
            warnings: Vec::new(),
        })
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
            every_seconds = schedule.every_seconds as i64;
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
        sqlx::query(
            "UPDATE axon_source_watches \
             SET enabled = ?, every_seconds = ?, cron = ?, timezone = ?, embed = ?, \
                 options_json = ?, collection = ?, scope = ?, updated_at = ? \
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
        if request.cursor.is_some() {
            return Err(ApiError::new(
                "watch.cursor_unsupported",
                ErrorStage::Retrieving,
                "sqlite watch store does not implement cursor pagination",
            ));
        }

        let mut sql = String::from("SELECT * FROM axon_source_watches WHERE 1 = 1");
        if request.enabled.is_some() {
            sql.push_str(" AND enabled = ?");
        }
        if request.source_id.is_some() {
            sql.push_str(" AND source_id = ?");
        }
        if request.adapter.is_some() {
            sql.push_str(" AND adapter_name = ?");
        }
        sql.push_str(" ORDER BY created_at ASC");

        let mut query = sqlx::query(&sql);
        if let Some(enabled) = request.enabled {
            query = query.bind(if enabled { 1 } else { 0 });
        }
        if let Some(source_id) = &request.source_id {
            query = query.bind(&source_id.0);
        }
        if let Some(adapter) = &request.adapter {
            query = query.bind(adapter);
        }

        let rows = query.fetch_all(&self.pool).await.map_err(sqlite_err)?;
        let total = rows.len() as u64;
        let limit = clamp_page_limit(request.limit);
        let items = rows
            .iter()
            .map(row_to_result)
            .map(|watch| WatchSummary {
                watch_id: watch.watch_id,
                source_id: watch.source_id,
                enabled: watch.enabled,
                schedule: watch.schedule,
                next_run_at: Timestamp::from(chrono::Utc::now()),
                last_job_id: watch.latest_job.as_ref().map(|job| job.job_id),
                last_status: watch.latest_job.as_ref().map(|job| job.status),
            })
            .take(limit as usize)
            .collect::<Vec<_>>();

        Ok(Page {
            total: Some(total),
            limit,
            next_cursor: None,
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
            "UPDATE axon_source_watches SET last_job_id = ?, last_status = ?, updated_at = ? \
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
        let rows = sqlx::query(
            "SELECT r.job_id AS job_id, j.status AS status \
             FROM axon_source_watch_runs r \
             LEFT JOIN jobs j ON j.job_id = r.job_id \
             WHERE r.watch_id = ? \
             ORDER BY r.created_at DESC, r.id DESC \
             LIMIT ?",
        )
        .bind(&request.watch_id.0)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(sqlite_err)?;

        let jobs = rows
            .iter()
            .map(|row| {
                let job_id: String = row.get("job_id");
                let status: Option<String> = row.get("status");
                synth_descriptor(&job_id, status.as_deref())
            })
            .filter(|job| request.status.is_none_or(|status| job.status == status))
            .collect();

        Ok(WatchHistoryResult {
            watch_id: request.watch_id,
            jobs,
            next_cursor: None,
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
