//! SQLite-backed production observability sink.
//!
//! The contract-mandated [`ObservabilitySink`](crate::collector::ObservabilitySink)
//! signature returns `Result<(), ApiError>`; `ApiError` is a rich shared error
//! shape, so `result_large_err` is expected across this boundary and allowed
//! module-wide (matching the precedent in `axon-web`).
#![allow(clippy::result_large_err)]

pub const MODULE_NAME: &str = "sink::sqlite";

use std::sync::Arc;

use async_trait::async_trait;
use axon_api::source::{
    ApiError, JobHeartbeat, JobId, ProviderId, ProviderKind, SourceProgressEvent, Timestamp,
};
use axon_error::ErrorStage;
use chrono::Utc;
use serde_json::json;
use sqlx::{Row, SqlitePool};

use crate::collector::{ObservabilitySink, Result};
use crate::metric::MetricSample;
use crate::sequence::SequenceRegistry;

/// Durable, standalone observability sink.
///
/// Persists progress events, the latest heartbeat per job, and provider health
/// to SQLite. Sequence assignment is centralized through a shared
/// [`SequenceRegistry`] so every emitted event on this sink carries a strictly
/// increasing per-`job_id` sequence, and the `(job_id, sequence)` unique index
/// is the durable backstop for that invariant.
#[derive(Clone)]
pub struct SqliteObservabilitySink {
    pool: SqlitePool,
    sequences: Arc<SequenceRegistry>,
}

/// Provider degradation snapshot recorded via
/// [`SqliteObservabilitySink::record_provider_health`].
#[derive(Debug, Clone, PartialEq)]
pub struct ProviderHealthRecord {
    pub provider_id: ProviderId,
    pub provider_kind: ProviderKind,
    /// Contract status: `ready`, `degraded`, `cooling`, `unavailable`, `disabled`.
    pub status: String,
    pub cooldown_until: Option<Timestamp>,
    pub last_error_code: Option<String>,
}

impl SqliteObservabilitySink {
    /// Open a pool at `path` (`":memory:"` for tests), run the in-crate
    /// migration, and build the sink. Enables foreign keys + WAL when on disk.
    pub async fn connect(path: &str) -> Result<Self> {
        let pool = open_pool(path).await.map_err(map_sqlx)?;
        Self::from_pool(pool).await
    }

    /// Build a sink from an existing pool, running the migration on it.
    pub async fn from_pool(pool: SqlitePool) -> Result<Self> {
        sqlx::migrate!("src/migrations")
            .run(&pool)
            .await
            .map_err(|e| map_str("observe.migrate_failed", e.to_string()))?;
        Ok(Self {
            pool,
            sequences: Arc::new(SequenceRegistry::new()),
        })
    }

    /// Shared sequence registry, so callers can assign sequences consistently
    /// with what this sink will persist.
    pub fn sequences(&self) -> Arc<SequenceRegistry> {
        Arc::clone(&self.sequences)
    }

    /// Assign the next monotonic sequence for the event's job and persist it.
    ///
    /// The event's builder-supplied placeholder sequence is overwritten with the
    /// registry-assigned value before serialization, so the durable row and the
    /// returned/serialized event agree.
    async fn persist_event(&self, mut event: SourceProgressEvent) -> Result<()> {
        let job_id = event.job_id;
        event.sequence = self.sequences.next(job_id);
        let sequence = i64::try_from(event.sequence)
            .map_err(|_| map_str("observe.sequence_overflow", "sequence exceeds i64"))?;
        let event_json = serde_json::to_string(&event)
            .map_err(|e| map_str("observe.serialize_failed", e.to_string()))?;

        sqlx::query(
            "INSERT INTO axon_observe_events \
             (event_id, job_id, sequence, phase, status, severity, visibility, message, \
              timestamp, event_json, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&event.event_id)
        .bind(job_id.0.to_string())
        .bind(sequence)
        .bind(enum_str(&event.phase))
        .bind(enum_str(&event.status))
        .bind(enum_str(&event.severity))
        .bind(enum_str(&event.visibility))
        .bind(&event.message)
        .bind(&event.timestamp.0)
        .bind(event_json)
        .bind(now_ms())
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(())
    }

    /// Read back persisted events for a job in sequence order (test/inspection).
    pub async fn events_for(&self, job_id: JobId) -> Result<Vec<SourceProgressEvent>> {
        let rows = sqlx::query(
            "SELECT event_json FROM axon_observe_events \
             WHERE job_id = ? ORDER BY sequence ASC",
        )
        .bind(job_id.0.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?;

        rows.into_iter()
            .map(|row| {
                let raw: String = row.get(0);
                serde_json::from_str(&raw)
                    .map_err(|e| map_str("observe.deserialize_failed", e.to_string()))
            })
            .collect()
    }

    /// Read back the latest heartbeat row for a job, if any.
    pub async fn heartbeat_for(&self, job_id: JobId) -> Result<Option<JobHeartbeat>> {
        let row =
            sqlx::query("SELECT heartbeat_json FROM axon_observe_heartbeats WHERE job_id = ?")
                .bind(job_id.0.to_string())
                .fetch_optional(&self.pool)
                .await
                .map_err(map_sqlx)?;

        match row {
            None => Ok(None),
            Some(row) => {
                let raw: String = row.get(0);
                let hb = serde_json::from_str(&raw)
                    .map_err(|e| map_str("observe.deserialize_failed", e.to_string()))?;
                Ok(Some(hb))
            }
        }
    }

    /// Record provider degradation state (upsert by provider id).
    pub async fn record_provider_health(&self, record: ProviderHealthRecord) -> Result<()> {
        sqlx::query(
            "INSERT INTO axon_observe_provider_health \
             (provider_id, provider_kind, status, cooldown_until, last_error_code, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?) \
             ON CONFLICT(provider_id) DO UPDATE SET \
               provider_kind = excluded.provider_kind, \
               status = excluded.status, \
               cooldown_until = excluded.cooldown_until, \
               last_error_code = excluded.last_error_code, \
               updated_at = excluded.updated_at",
        )
        .bind(&record.provider_id.0)
        .bind(enum_str(&record.provider_kind))
        .bind(&record.status)
        .bind(record.cooldown_until.as_ref().map(|t| t.0.clone()))
        .bind(&record.last_error_code)
        .bind(now_ms())
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(())
    }

    /// Read back provider health for inspection/tests.
    pub async fn provider_health_for(
        &self,
        provider_id: &ProviderId,
    ) -> Result<Option<ProviderHealthRecord>> {
        let row = sqlx::query(
            "SELECT provider_id, provider_kind, status, cooldown_until, last_error_code \
             FROM axon_observe_provider_health WHERE provider_id = ?",
        )
        .bind(&provider_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?;

        Ok(row.map(|row| ProviderHealthRecord {
            provider_id: ProviderId(row.get(0)),
            provider_kind: parse_provider_kind(row.get::<String, _>(1).as_str()),
            status: row.get(2),
            cooldown_until: row.get::<Option<String>, _>(3).map(Timestamp),
            last_error_code: row.get(4),
        }))
    }
}

#[async_trait]
impl ObservabilitySink for SqliteObservabilitySink {
    async fn emit(&self, event: SourceProgressEvent) -> Result<()> {
        self.persist_event(event).await
    }

    async fn heartbeat(&self, mut heartbeat: JobHeartbeat) -> Result<()> {
        // Stamp the last durable sequence observed for this stream so heartbeat
        // consumers can detect progress even between events.
        if heartbeat.last_event_sequence.is_none() {
            heartbeat.last_event_sequence = self.sequences.last(heartbeat.job_id);
        }
        let heartbeat_json = serde_json::to_string(&heartbeat)
            .map_err(|e| map_str("observe.serialize_failed", e.to_string()))?;
        let last_seq = heartbeat
            .last_event_sequence
            .map(i64::try_from)
            .transpose()
            .map_err(|_| map_str("observe.sequence_overflow", "sequence exceeds i64"))?;

        sqlx::query(
            "INSERT INTO axon_observe_heartbeats \
             (job_id, attempt, worker_id, phase, status, heartbeat_at, last_event_sequence, \
              heartbeat_json, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(job_id) DO UPDATE SET \
               attempt = excluded.attempt, \
               worker_id = excluded.worker_id, \
               phase = excluded.phase, \
               status = excluded.status, \
               heartbeat_at = excluded.heartbeat_at, \
               last_event_sequence = excluded.last_event_sequence, \
               heartbeat_json = excluded.heartbeat_json, \
               updated_at = excluded.updated_at",
        )
        .bind(heartbeat.job_id.0.to_string())
        .bind(i64::from(heartbeat.attempt))
        .bind(&heartbeat.worker_id)
        .bind(enum_str(&heartbeat.phase))
        .bind(enum_str(&heartbeat.status))
        .bind(&heartbeat.heartbeat_at.0)
        .bind(last_seq)
        .bind(heartbeat_json)
        .bind(now_ms())
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(())
    }

    async fn metric(&self, metric: MetricSample) -> Result<()> {
        // Metric labels must stay bounded; reject unbounded/high-cardinality
        // label keys rather than persisting them. Metric samples are not stored
        // durably here (they belong to a metrics exporter) but the contract's
        // bounded-label rule is enforced at the boundary.
        reject_unbounded_labels(&metric)
    }

    async fn flush(&self) -> Result<()> {
        // SQLite writes are synchronous per statement; there is no in-process
        // buffer to drain. flush is a no-op that must still succeed after
        // terminal events per the contract.
        Ok(())
    }
}

fn reject_unbounded_labels(metric: &MetricSample) -> Result<()> {
    const FORBIDDEN: [&str; 5] = ["url", "path", "query", "document_id", "chunk_id"];
    for key in metric.labels.keys() {
        if FORBIDDEN.contains(&key.as_str()) {
            return Err(map_str(
                "observe.unbounded_label",
                format!("metric '{}' uses unbounded label '{key}'", metric.name),
            ));
        }
    }
    Ok(())
}

async fn open_pool(path: &str) -> std::result::Result<SqlitePool, sqlx::Error> {
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::str::FromStr;

    let options = if path == ":memory:" {
        SqliteConnectOptions::from_str("sqlite::memory:")?
    } else {
        SqliteConnectOptions::from_str(&format!("sqlite://{path}"))?
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
    };

    // A single connection for :memory: keeps the shared in-memory database
    // alive across the sink's lifetime; on-disk pools can fan out.
    let max = if path == ":memory:" { 1 } else { 8 };
    SqlitePoolOptions::new()
        .max_connections(max)
        .connect_with(options)
        .await
}

fn enum_str<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "unknown".to_string())
}

fn parse_provider_kind(raw: &str) -> ProviderKind {
    serde_json::from_value(json!(raw)).unwrap_or(ProviderKind::HealthProbe)
}

fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

fn map_sqlx(err: sqlx::Error) -> ApiError {
    map_str("observe.sqlite_error", err.to_string())
}

fn map_str(code: &str, message: impl Into<String>) -> ApiError {
    ApiError::new(code, ErrorStage::Planning, message.into())
}

#[cfg(test)]
#[path = "sqlite_tests.rs"]
mod tests;
