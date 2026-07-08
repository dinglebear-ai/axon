//! Memory lifecycle operations: links, reinforcement, supersession,
//! contradiction, and status transitions.
//!
//! Every mutation appends a `MemoryHistoryEvent` (contract: "status changes
//! append memory history events") and re-persists the record.

use axon_api::source::*;
use rusqlite::Connection;

use crate::record::age_days;
use crate::sqlite::error::{invalid, not_found, redaction_failed, store_error};
use crate::sqlite::{SqliteMemoryStore, age_and_bounds, result_from_record, update_record};
use crate::store::Result;

/// Load all links for a memory, ordered by insertion.
pub fn load_links(conn: &Connection, memory_id: &str) -> Result<Vec<MemoryLink>> {
    let mut stmt = conn
        .prepare(
            "SELECT link_type, target, confidence, evidence_json
             FROM memory_links WHERE memory_id = ?1 ORDER BY id",
        )
        .map_err(|e| store_error(format!("prepare links: {e}")))?;
    let rows = stmt
        .query_map([memory_id], |row| {
            let link_type: String = row.get(0)?;
            let target: String = row.get(1)?;
            let confidence: f64 = row.get(2)?;
            let evidence_json: String = row.get(3)?;
            Ok((link_type, target, confidence, evidence_json))
        })
        .map_err(|e| store_error(format!("query links: {e}")))?;
    let mut links = Vec::new();
    for row in rows {
        let (link_type, target, confidence, evidence_json) =
            row.map_err(|e| store_error(format!("link row: {e}")))?;
        let evidence: Vec<GraphEvidence> =
            serde_json::from_str(&evidence_json).map_err(|e| store_error(format!("json: {e}")))?;
        links.push(MemoryLink {
            link_type,
            target,
            confidence: confidence as f32,
            evidence,
        });
    }
    Ok(links)
}

/// Insert one link row for a memory.
pub fn insert_link(conn: &Connection, memory_id: &str, link: &MemoryLink, now: &str) -> Result<()> {
    let evidence_json =
        serde_json::to_string(&link.evidence).map_err(|e| store_error(format!("json: {e}")))?;
    conn.execute(
        "INSERT INTO memory_links (memory_id, link_type, target, confidence, evidence_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![memory_id, link.link_type, link.target, link.confidence as f64, evidence_json, now],
    )
    .map_err(|e| store_error(format!("insert link: {e}")))?;
    Ok(())
}

/// Record a positive-use reinforcement signal.
///
/// Contract: reinforcement increments `reinforcement_count`, updates
/// `last_reinforced_at` (which resets decay age), adjusts salience by the
/// signal amount, and appends history.
pub async fn reinforce(
    store: &SqliteMemoryStore,
    memory_id: MemoryId,
    signal: MemoryReinforcement,
) -> Result<MemoryResult> {
    let now_secs = store.clock().now_epoch_secs();
    let conn = store.conn().lock().await;
    let mut record = SqliteMemoryStore::load_record(&conn, &memory_id.0)?
        .ok_or_else(|| not_found(&memory_id.0))?;

    record.salience = (record.salience + signal.amount).clamp(0.0, 1.0);
    let mut decay = record.decay.take().unwrap_or(MemoryDecayPolicy {
        profile: crate::sqlite::decay_profile_str(record.memory_type.default_decay_profile())
            .to_string(),
        half_life_days: None,
        last_reinforced_at: None,
        reinforcement_count: 0,
        review_after: None,
        expires_at: None,
        pinned: false,
    });
    decay.reinforcement_count = decay.reinforcement_count.saturating_add(1);
    decay.last_reinforced_at = Some(signal.timestamp.clone());
    record.decay = Some(decay);
    record.history.push(MemoryHistoryEvent {
        status: record.status,
        message: signal.reason,
        timestamp: signal.timestamp.clone(),
    });

    let now = signal.timestamp.0.clone();
    update_record(&conn, &record, &now)?;

    // Age is measured from last_reinforced_at, so a fresh reinforcement gives
    // age 0 -> full decay multiplier.
    let age = age_days(&record, now_secs);
    let score = crate::decay::score_record(&record, age, 0.0, 1.0, false);
    let (_, created, _) = age_and_bounds(&record, now_secs);
    Ok(result_from_record(&record, score, &created, &now))
}

/// Replace `memory_id` with `replacement_id`.
///
/// Contract: supersede hides the old memory (status `superseded`), points it at
/// the replacement, and preserves history.
pub async fn supersede(
    store: &SqliteMemoryStore,
    request: MemorySupersedeRequest,
) -> Result<MemoryResult> {
    if request.memory_id == request.replacement_id {
        return Err(invalid("a memory cannot supersede itself"));
    }
    let now_secs = store.clock().now_epoch_secs();
    let now = request.timestamp.0.clone();
    let conn = store.conn().lock().await;

    // Replacement must exist.
    if SqliteMemoryStore::load_record(&conn, &request.replacement_id.0)?.is_none() {
        return Err(not_found(&request.replacement_id.0));
    }
    let mut record = SqliteMemoryStore::load_record(&conn, &request.memory_id.0)?
        .ok_or_else(|| not_found(&request.memory_id.0))?;

    record.status = MemoryStatus::Superseded;
    record.superseded_by = Some(request.replacement_id.clone());
    let reason = request
        .reason
        .unwrap_or_else(|| format!("superseded by {}", request.replacement_id.0));
    record.history.push(MemoryHistoryEvent {
        status: MemoryStatus::Superseded,
        message: reason,
        timestamp: request.timestamp,
    });
    update_record(&conn, &record, &now)?;

    let age = age_days(&record, now_secs);
    let score = crate::decay::score_record(&record, age, 0.0, 1.0, false);
    let (_, created, _) = age_and_bounds(&record, now_secs);
    Ok(result_from_record(&record, score, &created, &now))
}

/// Flag two memories as conflicting; both transition to `contradicted` and are
/// enqueued for review.
pub async fn contradict(
    store: &SqliteMemoryStore,
    request: MemoryContradictRequest,
) -> Result<MemoryResult> {
    if request.memory_id == request.conflicting_id {
        return Err(invalid("a memory cannot contradict itself"));
    }
    let now_secs = store.clock().now_epoch_secs();
    let now = request.timestamp.0.clone();
    let conn = store.conn().lock().await;

    let mut other = SqliteMemoryStore::load_record(&conn, &request.conflicting_id.0)?
        .ok_or_else(|| not_found(&request.conflicting_id.0))?;
    let mut record = SqliteMemoryStore::load_record(&conn, &request.memory_id.0)?
        .ok_or_else(|| not_found(&request.memory_id.0))?;

    let reason = request
        .reason
        .clone()
        .unwrap_or_else(|| "contradiction detected".to_string());

    for (rec, other_id) in [
        (&mut record, request.conflicting_id.clone()),
        (&mut other, request.memory_id.clone()),
    ] {
        rec.status = MemoryStatus::Contradicted;
        rec.contradicts = Some(other_id);
        rec.history.push(MemoryHistoryEvent {
            status: MemoryStatus::Contradicted,
            message: reason.clone(),
            timestamp: request.timestamp.clone(),
        });
        update_record(&conn, rec, &now)?;
        enqueue_review(&conn, &rec.memory_id.0, Some(&reason), &now)?;
    }

    let age = age_days(&record, now_secs);
    let score = crate::decay::score_record(&record, age, 0.0, 1.0, false);
    let (_, created, _) = age_and_bounds(&record, now_secs);
    Ok(result_from_record(&record, score, &created, &now))
}

/// Transition a memory to a new status (archive/forget/pin/review/active).
///
/// Pin/unpin is modeled through the decay policy `pinned` flag when the target
/// status stays recallable; `pin` sets status `active` + `pinned=true`.
pub async fn set_status(
    store: &SqliteMemoryStore,
    request: MemoryStatusRequest,
) -> Result<MemoryResult> {
    let now_secs = store.clock().now_epoch_secs();
    let now = request.timestamp.0.clone();
    let conn = store.conn().lock().await;
    let mut record = SqliteMemoryStore::load_record(&conn, &request.memory_id.0)?
        .ok_or_else(|| not_found(&request.memory_id.0))?;

    record.status = request.status;
    if request.status == MemoryStatus::Review {
        enqueue_review(&conn, &request.memory_id.0, request.reason.as_deref(), &now)?;
    }
    let message = request
        .reason
        .unwrap_or_else(|| format!("status -> {:?}", request.status));
    record.history.push(MemoryHistoryEvent {
        status: request.status,
        message,
        timestamp: request.timestamp,
    });
    update_record(&conn, &record, &now)?;

    let age = age_days(&record, now_secs);
    let score = crate::decay::score_record(&record, age, 0.0, 1.0, false);
    let (_, created, _) = age_and_bounds(&record, now_secs);
    Ok(result_from_record(&record, score, &created, &now))
}

/// Edit a memory's editable fields (body/title/type/confidence/salience/
/// scope) in place. Only fields present in `request` change.
pub async fn update(
    store: &SqliteMemoryStore,
    request: MemoryUpdateRequest,
) -> Result<MemoryResult> {
    let now_secs = store.clock().now_epoch_secs();
    let now = request.timestamp.0.clone();
    let conn = store.conn().lock().await;
    let mut record = SqliteMemoryStore::load_record(&conn, &request.memory_id.0)?
        .ok_or_else(|| not_found(&request.memory_id.0))?;

    // Fail-closed redaction boundary: an updated body/title is durable and
    // recalled back through CLI/MCP/REST in later sessions — same rule as
    // `remember`. Oversized input blocks the write rather than persisting
    // unscrubbed content (see `redact_text_checked`'s doc comment).
    let redactor = axon_core::redact::DefaultRedactor::new();
    let redaction_context = axon_core::redact::RedactionContext::memory_record();
    if let Some(body) = request.body {
        record.body = axon_core::redact::redact_text_checked(&redactor, &body, &redaction_context)
            .map_err(|_| {
                redaction_failed(format!(
                    "memory body exceeds {} bytes; redaction cannot be safely verified",
                    axon_core::redact::MAX_REDACTABLE_TEXT_BYTES
                ))
            })?;
    }
    if let Some(title) = request.title {
        record.title = Some(
            axon_core::redact::redact_text_checked(&redactor, &title, &redaction_context).map_err(
                |_| {
                    redaction_failed(format!(
                        "memory title exceeds {} bytes; redaction cannot be safely verified",
                        axon_core::redact::MAX_REDACTABLE_TEXT_BYTES
                    ))
                },
            )?,
        );
    }
    if let Some(memory_type) = request.memory_type {
        record.memory_type = memory_type;
    }
    if let Some(confidence) = request.confidence {
        if confidence.is_nan() {
            return Err(invalid("confidence must be a number"));
        }
        record.confidence = confidence;
    }
    if let Some(salience) = request.salience {
        if salience.is_nan() {
            return Err(invalid("salience must be a number"));
        }
        record.salience = salience;
    }
    if let Some(scope) = request.scope {
        record.scope = scope;
    }

    let message = request.reason.unwrap_or_else(|| "updated".to_string());
    record.history.push(MemoryHistoryEvent {
        status: record.status,
        message,
        timestamp: request.timestamp,
    });
    update_record(&conn, &record, &now)?;

    let age = age_days(&record, now_secs);
    let score = crate::decay::score_record(&record, age, 0.0, 1.0, false);
    let (_, created, _) = age_and_bounds(&record, now_secs);
    Ok(result_from_record(&record, score, &created, &now))
}

/// Pin or unpin a memory: sets the decay policy `pinned` flag without
/// changing status. A memory with no decay policy yet gets the type's
/// default profile stamped alongside the flag.
pub async fn pin(store: &SqliteMemoryStore, request: MemoryPinRequest) -> Result<MemoryResult> {
    let now_secs = store.clock().now_epoch_secs();
    let now = request.timestamp.0.clone();
    let conn = store.conn().lock().await;
    let mut record = SqliteMemoryStore::load_record(&conn, &request.memory_id.0)?
        .ok_or_else(|| not_found(&request.memory_id.0))?;

    let mut decay = record.decay.clone().unwrap_or_else(|| {
        let profile = record.memory_type.default_decay_profile();
        MemoryDecayPolicy {
            profile: crate::sqlite::decay_profile_str(profile).to_string(),
            half_life_days: profile.half_life_days().map(|d| d as u32),
            last_reinforced_at: None,
            reinforcement_count: 0,
            review_after: None,
            expires_at: None,
            pinned: false,
        }
    });
    decay.pinned = request.pinned;
    record.decay = Some(decay);

    let message = request.reason.unwrap_or_else(|| {
        if request.pinned {
            "pinned".to_string()
        } else {
            "unpinned".to_string()
        }
    });
    record.history.push(MemoryHistoryEvent {
        status: record.status,
        message,
        timestamp: request.timestamp,
    });
    update_record(&conn, &record, &now)?;

    let age = age_days(&record, now_secs);
    let score = crate::decay::score_record(&record, age, 0.0, 1.0, false);
    let (_, created, _) = age_and_bounds(&record, now_secs);
    Ok(result_from_record(&record, score, &created, &now))
}

/// Insert an open review-queue entry.
pub fn enqueue_review(
    conn: &Connection,
    memory_id: &str,
    reason: Option<&str>,
    now: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO memory_reviews (memory_id, reason, resolved, created_at)
         VALUES (?1, ?2, 0, ?3)",
        rusqlite::params![memory_id, reason, now],
    )
    .map_err(|e| store_error(format!("enqueue review: {e}")))?;
    Ok(())
}
