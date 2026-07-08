//! Compaction (merge N memories into one) and bulk import/export.

use axon_api::source::*;
use rusqlite::Connection;

use crate::record::age_days;
use crate::sqlite::error::{invalid, not_found, store_error};
use crate::sqlite::{
    SqliteMemoryStore, age_and_bounds, insert_record, result_from_record, update_record,
};
use crate::store::Result;

const SUPPORTED_STRATEGIES: &[&str] = &["concatenate"];

/// Merge `request.memory_ids` into one new memory.
///
/// Only the deterministic `"concatenate"` strategy is implemented — it joins
/// each source memory's body under a `[memory_id] body` heading. Other
/// strategies (e.g. `"semantic_summary"`, which would need an injected LLM
/// completer) are rejected rather than silently falling back.
pub async fn compact(
    store: &SqliteMemoryStore,
    request: MemoryCompactRequest,
) -> Result<MemoryResult> {
    if request.memory_ids.len() < 2 {
        return Err(invalid("compact requires at least 2 source memories"));
    }
    if !SUPPORTED_STRATEGIES.contains(&request.strategy.as_str()) {
        return Err(ApiError::new(
            "memory.unsupported_strategy",
            axon_error::ErrorStage::Validation,
            format!(
                "compact strategy {:?} is not implemented; supported: {SUPPORTED_STRATEGIES:?}",
                request.strategy
            ),
        ));
    }

    let now_secs = store.clock().now_epoch_secs();
    let now = request.timestamp.0.clone();
    let conn = store.conn().lock().await;

    let mut sources = Vec::with_capacity(request.memory_ids.len());
    for memory_id in &request.memory_ids {
        let record = SqliteMemoryStore::load_record(&conn, &memory_id.0)?
            .ok_or_else(|| not_found(&memory_id.0))?;
        sources.push(record);
    }

    let body = sources
        .iter()
        .map(|record| format!("[{}] {}", record.memory_id.0, record.body))
        .collect::<Vec<_>>()
        .join("\n\n");

    let memory_id = store.next_id();
    let decay_profile = request.result_type.default_decay_profile();
    let compacted = MemoryRecord {
        memory_id: memory_id.clone(),
        memory_type: request.result_type,
        status: MemoryStatus::Active,
        body,
        confidence: sources
            .iter()
            .map(|record| record.confidence)
            .fold(0.0f32, f32::max),
        salience: sources
            .iter()
            .map(|record| record.salience)
            .fold(0.0f32, f32::max),
        scope: request.scope,
        history: vec![MemoryHistoryEvent {
            status: MemoryStatus::Active,
            message: format!(
                "compacted from {}",
                request
                    .memory_ids
                    .iter()
                    .map(|id| id.0.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            timestamp: request.timestamp.clone(),
        }],
        title: request.title,
        links: Vec::new(),
        decay: Some(MemoryDecayPolicy {
            profile: crate::sqlite::decay_profile_str(decay_profile).to_string(),
            half_life_days: decay_profile.half_life_days().map(|d| d as u32),
            last_reinforced_at: None,
            reinforcement_count: 0,
            review_after: None,
            expires_at: None,
            pinned: false,
        }),
        embedding_refs: Vec::new(),
        superseded_by: None,
        contradicts: None,
    };
    insert_record(&conn, &compacted, &now)?;

    if request.archive_sources {
        for mut source in sources {
            source.status = MemoryStatus::Archived;
            source.history.push(MemoryHistoryEvent {
                status: MemoryStatus::Archived,
                message: format!("archived: compacted into {}", memory_id.0),
                timestamp: request.timestamp.clone(),
            });
            update_record(&conn, &source, &now)?;
        }
    }

    let age = age_days(&compacted, now_secs);
    let score = crate::decay::score_record(&compacted, age, 0.0, 1.0, false);
    let (_, created, _) = age_and_bounds(&compacted, now_secs);
    Ok(result_from_record(&compacted, score, &created, &now))
}

/// Bulk-import memory records, deduping by (body, memory_type, scope).
///
/// `dry_run` reports what would happen without writing. `ReplaceScope` first
/// archives every existing memory whose scope matches an incoming record's
/// scope, then inserts every incoming record (no dedup in replace mode —
/// the scope's prior content is being deliberately superseded).
pub async fn import(
    store: &SqliteMemoryStore,
    request: MemoryImportRequest,
) -> Result<MemoryImportResult> {
    let now = store.clock().now_rfc3339();
    let conn = store.conn().lock().await;

    let mut created = 0u32;
    let updated = 0u32;
    let mut skipped = 0u32;
    let mut created_ids = Vec::new();
    let warnings = Vec::new();

    if request.mode == MemoryImportMode::ReplaceScope && !request.dry_run {
        let mut archived_scopes = std::collections::HashSet::new();
        for record in &request.records {
            let scope_key = (record.scope.kind.clone(), record.scope.value.clone());
            if !archived_scopes.insert(scope_key) {
                continue;
            }
            archive_scope(&conn, &record.scope, &now)?;
        }
    }

    for incoming in request.records {
        let duplicate =
            request.mode == MemoryImportMode::Merge && content_duplicate_exists(&conn, &incoming)?;
        if duplicate {
            skipped += 1;
            continue;
        }
        if request.dry_run {
            created += 1;
            continue;
        }
        let memory_id = store.next_id();
        let mut record = incoming;
        record.memory_id = memory_id.clone();
        insert_record(&conn, &record, &now)?;
        created += 1;
        created_ids.push(memory_id);
    }

    Ok(MemoryImportResult {
        created,
        updated,
        skipped,
        dry_run: request.dry_run,
        created_ids,
        warnings,
    })
}

/// Export memory records matching an optional scope filter.
pub async fn export(
    store: &SqliteMemoryStore,
    request: MemoryExportRequest,
) -> Result<MemoryExportResult> {
    let conn = store.conn().lock().await;
    let mut stmt = conn
        .prepare("SELECT memory_id FROM memory_records ORDER BY created_at, memory_id")
        .map_err(|e| store_error(format!("prepare export: {e}")))?;
    let ids: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| store_error(format!("query export: {e}")))?
        .collect::<std::result::Result<_, _>>()
        .map_err(|e| store_error(format!("export row: {e}")))?;

    let mut records = Vec::new();
    for id in ids {
        let Some(record) = SqliteMemoryStore::load_record(&conn, &id)? else {
            continue;
        };
        if !request.include_archived && record.status == MemoryStatus::Archived {
            continue;
        }
        if record.status == MemoryStatus::Forgotten {
            continue;
        }
        if let Some(scope) = &request.scope
            && record.scope != *scope
        {
            continue;
        }
        records.push(record);
    }
    let count = records.len() as u32;
    Ok(MemoryExportResult { records, count })
}

fn content_duplicate_exists(conn: &Connection, record: &MemoryRecord) -> Result<bool> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM memory_records
             WHERE body = ?1 AND memory_type = ?2 AND scope_kind = ?3 AND scope_value = ?4",
            rusqlite::params![
                record.body,
                crate::sqlite::rows::type_to_str(record.memory_type),
                record.scope.kind,
                record.scope.value,
            ],
            |row| row.get(0),
        )
        .map_err(|e| store_error(format!("dedup lookup: {e}")))?;
    Ok(count > 0)
}

fn archive_scope(conn: &Connection, scope: &MemoryScope, now: &str) -> Result<()> {
    conn.execute(
        "UPDATE memory_records SET status = 'archived', updated_at = ?3
         WHERE scope_kind = ?1 AND scope_value = ?2 AND status != 'archived'",
        rusqlite::params![scope.kind, scope.value, now],
    )
    .map_err(|e| store_error(format!("archive scope: {e}")))?;
    Ok(())
}
