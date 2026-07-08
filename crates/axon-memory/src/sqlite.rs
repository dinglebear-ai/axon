//! Real SQLite-backed durable memory store.
//!
//! `SqliteMemoryStore` implements [`MemoryStore`](crate::store::MemoryStore)
//! against the schema in [`crate::migration`]. It owns memory lifecycle
//! (remember/link/reinforce/supersede/contradict/status/review), computes the
//! contract `memory_score` via [`crate::decay`], and applies scope/status/decay
//! recall rules from `runtime/memory-contract.md`.
//!
//! Storage is a single `Mutex<Connection>`; the memory store is a single-writer
//! durable subsystem, so serializing access is correct and keeps the async
//! trait surface simple without a connection pool.

pub mod compact;
pub mod error;
pub mod lifecycle;
pub mod recall;
pub mod rows;

use std::sync::Arc;

use async_trait::async_trait;
use axon_api::source::*;
use axon_core::redact::{DefaultRedactor, RedactionContext, redact_text_checked};
use rusqlite::Connection;
use tokio::sync::Mutex;

use crate::migration::ensure_schema;
use crate::record::Clock;
use crate::store::{MemoryStore, Result};
use error::{invalid, not_found, redaction_failed, store_error};
use rows::{record_from_row, record_json_columns, status_to_str, type_to_str};

/// A durable memory store backed by SQLite.
pub struct SqliteMemoryStore {
    conn: Arc<Mutex<Connection>>,
    clock: Arc<dyn Clock>,
}

impl SqliteMemoryStore {
    /// Open (or create) a store at `path`, running the schema migration.
    pub fn open(path: &str, clock: Arc<dyn Clock>) -> Result<Self> {
        let conn = Connection::open(path).map_err(|e| store_error(format!("open: {e}")))?;
        Self::from_connection(conn, clock)
    }

    /// Create an in-memory store (for tests).
    pub fn in_memory(clock: Arc<dyn Clock>) -> Result<Self> {
        let conn = Connection::open_in_memory().map_err(|e| store_error(format!("open: {e}")))?;
        Self::from_connection(conn, clock)
    }

    fn from_connection(conn: Connection, clock: Arc<dyn Clock>) -> Result<Self> {
        ensure_schema(&conn).map_err(|e| store_error(format!("migrate: {e}")))?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            clock,
        })
    }

    /// Generate a fresh memory id. UUIDv4-based rather than a per-instance
    /// counter: `axon-services` opens a new `SqliteMemoryStore` handle on
    /// every dispatch call, so a process-local sequence resets each time and
    /// two `remember`s in the same wall-clock second could otherwise collide
    /// on `mem_<secs>_<seq>`.
    fn next_id(&self) -> MemoryId {
        MemoryId::new(format!("mem_{}", uuid::Uuid::new_v4().simple()))
    }

    pub(crate) fn conn(&self) -> &Arc<Mutex<Connection>> {
        &self.conn
    }

    pub(crate) fn clock(&self) -> &Arc<dyn Clock> {
        &self.clock
    }

    /// Load one record (with its links) inside an already-held connection.
    pub(crate) fn load_record(conn: &Connection, memory_id: &str) -> Result<Option<MemoryRecord>> {
        let links = lifecycle::load_links(conn, memory_id)?;
        let mut stmt = conn
            .prepare("SELECT * FROM memory_records WHERE memory_id = ?1")
            .map_err(|e| store_error(format!("prepare: {e}")))?;
        let mut query = stmt
            .query([memory_id])
            .map_err(|e| store_error(format!("query: {e}")))?;
        match query.next().map_err(|e| store_error(format!("row: {e}")))? {
            Some(row) => Ok(Some(record_from_row(row, links)?)),
            None => Ok(None),
        }
    }
}

#[async_trait]
impl MemoryStore for SqliteMemoryStore {
    async fn remember(&self, request: MemoryRequest) -> Result<MemoryResult> {
        if request.body.trim().is_empty() {
            return Err(invalid("memory body must not be empty"));
        }
        if request.confidence.is_nan() || request.salience.is_nan() {
            return Err(invalid("confidence/salience must be numbers"));
        }
        let memory_id = self.next_id();
        let now = self.clock.now_rfc3339();

        // Fail-closed redaction boundary: a remembered body/title is durable
        // and recalled back through CLI/MCP/REST in later sessions, so a
        // secret pasted into "remember this" content must not persist
        // verbatim. Scrub before the write, not after. When the boundary
        // cannot safely scan the input (oversized text — unbounded regex
        // scanning over attacker-controlled content is itself a DoS
        // surface), block the write entirely rather than persist it
        // unscrubbed.
        let redactor = DefaultRedactor::new();
        let redaction_context = RedactionContext::memory_record();
        let request_body = redact_text_checked(&redactor, &request.body, &redaction_context)
            .map_err(|_| {
                redaction_failed(format!(
                    "memory body exceeds {} bytes; redaction cannot be safely verified",
                    axon_core::redact::MAX_REDACTABLE_TEXT_BYTES
                ))
            })?;
        let request_title = request
            .title
            .as_deref()
            .map(|title| redact_text_checked(&redactor, title, &redaction_context))
            .transpose()
            .map_err(|_| {
                redaction_failed(format!(
                    "memory title exceeds {} bytes; redaction cannot be safely verified",
                    axon_core::redact::MAX_REDACTABLE_TEXT_BYTES
                ))
            })?;

        // Default decay policy from the memory type when none supplied.
        let decay = request.decay.clone().or_else(|| {
            let profile = request.memory_type.default_decay_profile();
            Some(MemoryDecayPolicy {
                profile: decay_profile_str(profile).to_string(),
                half_life_days: profile.half_life_days().map(|d| d as u32),
                last_reinforced_at: None,
                reinforcement_count: 0,
                review_after: None,
                expires_at: None,
                pinned: false,
            })
        });

        let record = MemoryRecord {
            memory_id: memory_id.clone(),
            memory_type: request.memory_type,
            status: MemoryStatus::Active,
            body: request_body,
            confidence: request.confidence,
            salience: request.salience,
            scope: request.scope,
            history: vec![MemoryHistoryEvent {
                status: MemoryStatus::Active,
                message: "created".to_string(),
                timestamp: Timestamp(now.clone()),
            }],
            title: request_title,
            links: request.links.clone(),
            decay,
            embedding_refs: Vec::new(),
            superseded_by: None,
            contradicts: None,
        };

        let conn = self.conn.lock().await;
        insert_record(&conn, &record, &now)?;
        for link in &request.links {
            lifecycle::insert_link(&conn, &memory_id.0, link, &now)?;
        }
        let age = 0.0;
        let score = crate::decay::score_record(&record, age, 0.0, 1.0, false);
        Ok(result_from_record(&record, score, &now, &now))
    }

    async fn get(&self, memory_id: MemoryId) -> Result<Option<MemoryRecord>> {
        let conn = self.conn.lock().await;
        Self::load_record(&conn, &memory_id.0)
    }

    async fn load_many(&self, memory_ids: Vec<MemoryId>) -> Result<Vec<Option<MemoryRecord>>> {
        let conn = self.conn.lock().await;
        memory_ids
            .into_iter()
            .map(|memory_id| Self::load_record(&conn, &memory_id.0))
            .collect()
    }

    async fn search(&self, request: MemorySearchRequest) -> Result<MemorySearchResult> {
        recall::search(self, request).await
    }

    async fn context(&self, request: MemoryContextRequest) -> Result<MemoryContextResult> {
        recall::context(self, request).await
    }

    async fn link(&self, request: MemoryLinkRequest) -> Result<MemoryResult> {
        let now = self.clock.now_rfc3339();
        let conn = self.conn.lock().await;
        let mut record = Self::load_record(&conn, &request.memory_id.0)?
            .ok_or_else(|| not_found(&request.memory_id.0))?;
        lifecycle::insert_link(&conn, &request.memory_id.0, &request.link, &now)?;
        record.links.push(request.link);
        record.history.push(MemoryHistoryEvent {
            status: record.status,
            message: "linked".to_string(),
            timestamp: Timestamp(now.clone()),
        });
        update_history(&conn, &record, &now)?;
        let (age, created, updated) = age_and_bounds(&record, self.clock.now_epoch_secs());
        let score = crate::decay::score_record(&record, age, 0.0, 1.0, false);
        Ok(result_from_record(&record, score, &created, &updated))
    }

    async fn reinforce(
        &self,
        memory_id: MemoryId,
        signal: MemoryReinforcement,
    ) -> Result<MemoryResult> {
        lifecycle::reinforce(self, memory_id, signal).await
    }

    async fn supersede(&self, request: MemorySupersedeRequest) -> Result<MemoryResult> {
        lifecycle::supersede(self, request).await
    }

    async fn contradict(&self, request: MemoryContradictRequest) -> Result<MemoryResult> {
        lifecycle::contradict(self, request).await
    }

    async fn set_status(&self, request: MemoryStatusRequest) -> Result<MemoryResult> {
        lifecycle::set_status(self, request).await
    }

    async fn review(&self, request: MemoryReviewRequest) -> Result<MemoryReviewResult> {
        recall::review(self, request).await
    }

    async fn update(&self, request: MemoryUpdateRequest) -> Result<MemoryResult> {
        lifecycle::update(self, request).await
    }

    async fn pin(&self, request: MemoryPinRequest) -> Result<MemoryResult> {
        lifecycle::pin(self, request).await
    }

    async fn archive(&self, request: MemoryArchiveRequest) -> Result<MemoryResult> {
        lifecycle::set_status(
            self,
            MemoryStatusRequest {
                memory_id: request.memory_id,
                status: MemoryStatus::Archived,
                reason: request.reason,
                timestamp: request.timestamp,
            },
        )
        .await
    }

    async fn forget(&self, request: MemoryForgetRequest) -> Result<MemoryResult> {
        lifecycle::set_status(
            self,
            MemoryStatusRequest {
                memory_id: request.memory_id,
                status: MemoryStatus::Forgotten,
                reason: request.reason,
                timestamp: request.timestamp,
            },
        )
        .await
    }

    async fn compact(&self, request: MemoryCompactRequest) -> Result<MemoryResult> {
        compact::compact(self, request).await
    }

    async fn import(&self, request: MemoryImportRequest) -> Result<MemoryImportResult> {
        compact::import(self, request).await
    }

    async fn export(&self, request: MemoryExportRequest) -> Result<MemoryExportResult> {
        compact::export(self, request).await
    }

    async fn reset(&self) -> Result<()> {
        let conn = self.conn.lock().await;
        conn.execute_batch(
            "DELETE FROM memory_reviews; DELETE FROM memory_reinforcement; \
             DELETE FROM memory_links; DELETE FROM memory_records;",
        )
        .map_err(|e| store_error(format!("reset: {e}")))?;
        Ok(())
    }

    async fn capabilities(&self) -> Result<MemoryStoreCapability> {
        Ok(CapabilityBase {
            name: "sqlite-memory".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            owner_crate: "axon-memory".to_string(),
            health: HealthStatus::Healthy,
            features: vec![
                "remember".to_string(),
                "search".to_string(),
                "context".to_string(),
                "link".to_string(),
                "reinforce".to_string(),
                "supersede".to_string(),
                "contradict".to_string(),
                "set_status".to_string(),
                "review".to_string(),
                "decay".to_string(),
            ],
            limits: MetadataMap::new(),
        }
        .into())
    }
}

/// Snake_case wire string for a decay profile.
pub fn decay_profile_str(profile: DecayProfile) -> &'static str {
    match profile {
        DecayProfile::VeryFast => "very_fast",
        DecayProfile::Fast => "fast",
        DecayProfile::Normal => "normal",
        DecayProfile::Slow => "slow",
        DecayProfile::VerySlow => "very_slow",
        DecayProfile::None => "none",
    }
}

/// Insert a fresh record row.
pub(crate) fn insert_record(conn: &Connection, record: &MemoryRecord, now: &str) -> Result<()> {
    let (decay_json, history_json, embedding_json) = record_json_columns(record)?;
    conn.execute(
        "INSERT INTO memory_records (
            memory_id, memory_type, status, body, title, confidence, salience,
            scope_kind, scope_value, decay_json, history_json, embedding_refs_json,
            superseded_by, contradicts, created_at, updated_at
         ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16
         )",
        rusqlite::params![
            record.memory_id.0,
            type_to_str(record.memory_type),
            status_to_str(record.status),
            record.body,
            record.title,
            record.confidence as f64,
            record.salience as f64,
            record.scope.kind,
            record.scope.value,
            decay_json,
            history_json,
            embedding_json,
            record.superseded_by.as_ref().map(|m| m.0.clone()),
            record.contradicts.as_ref().map(|m| m.0.clone()),
            now,
            now,
        ],
    )
    .map_err(|e| store_error(format!("insert: {e}")))?;
    Ok(())
}

/// Persist a record's mutable columns (status/decay/history/scope/refs).
pub(crate) fn update_record(conn: &Connection, record: &MemoryRecord, now: &str) -> Result<()> {
    let (decay_json, history_json, embedding_json) = record_json_columns(record)?;
    conn.execute(
        "UPDATE memory_records SET
            memory_type = ?2, status = ?3, body = ?4, title = ?5,
            confidence = ?6, salience = ?7, scope_kind = ?8, scope_value = ?9,
            decay_json = ?10, history_json = ?11, embedding_refs_json = ?12,
            superseded_by = ?13, contradicts = ?14, updated_at = ?15
         WHERE memory_id = ?1",
        rusqlite::params![
            record.memory_id.0,
            type_to_str(record.memory_type),
            status_to_str(record.status),
            record.body,
            record.title,
            record.confidence as f64,
            record.salience as f64,
            record.scope.kind,
            record.scope.value,
            decay_json,
            history_json,
            embedding_json,
            record.superseded_by.as_ref().map(|m| m.0.clone()),
            record.contradicts.as_ref().map(|m| m.0.clone()),
            now,
        ],
    )
    .map_err(|e| store_error(format!("update: {e}")))?;
    Ok(())
}

/// Update only history + updated_at (used by link).
fn update_history(conn: &Connection, record: &MemoryRecord, now: &str) -> Result<()> {
    let history_json =
        serde_json::to_string(&record.history).map_err(|e| store_error(format!("json: {e}")))?;
    conn.execute(
        "UPDATE memory_records SET history_json = ?2, updated_at = ?3 WHERE memory_id = ?1",
        rusqlite::params![record.memory_id.0, history_json, now],
    )
    .map_err(|e| store_error(format!("update history: {e}")))?;
    Ok(())
}

/// Build a `MemoryResult` from a record + computed score + created/updated ts.
pub(crate) fn result_from_record(
    record: &MemoryRecord,
    memory_score: f32,
    created_at: &str,
    updated_at: &str,
) -> MemoryResult {
    MemoryResult {
        memory_id: record.memory_id.clone(),
        memory_type: record.memory_type,
        status: record.status,
        memory_score,
        confidence: record.confidence,
        salience: record.salience,
        created_at: Timestamp(created_at.to_string()),
        updated_at: Timestamp(updated_at.to_string()),
        graph_node_id: None,
        document_id: None,
        vector_point_ids: record.embedding_refs.clone(),
        warnings: Vec::new(),
    }
}

/// Return `(age_days, created_at, updated_at)` for a record given now.
pub(crate) fn age_and_bounds(record: &MemoryRecord, now_secs: i64) -> (f64, String, String) {
    let created = record
        .history
        .first()
        .map(|e| e.timestamp.0.clone())
        .unwrap_or_default();
    let updated = record
        .history
        .last()
        .map(|e| e.timestamp.0.clone())
        .unwrap_or_else(|| created.clone());
    (crate::record::age_days(record, now_secs), created, updated)
}

#[cfg(test)]
#[path = "sqlite_tests.rs"]
mod tests;
