//! Compaction (merge N memories into one) and bulk import/export.

use async_trait::async_trait;
use axon_api::source::*;
use rusqlite::{Connection, TransactionBehavior};

use crate::observe::MemoryPhase;
use crate::record::age_days;
use crate::sqlite::error::{invalid, not_found, redaction_failed, store_error};
use crate::sqlite::{
    SqliteMemoryStore, age_and_bounds, insert_record, result_from_record, update_record,
};
use crate::store::Result;

/// Deterministic strategies need no injected dependency; `semantic_summary`
/// requires a [`CompactionSynthesizer`] to be wired via
/// `SqliteMemoryStore::with_compaction_synthesizer` and fails closed
/// (`memory.llm_unavailable`) when none is configured — it never silently
/// falls back to `concatenate`.
const SUPPORTED_STRATEGIES: &[&str] = &["concatenate", "semantic_summary"];

/// Injected LLM boundary for the `semantic_summary` compaction strategy
/// (contract "compaction: distillation rules ... LLM provider when synthesis
/// is needed"). `axon-memory` does not own an LLM provider implementation
/// (crate boundary) — real backends are wired in by `axon-services` via
/// `axon-core::llm`; tests use a fake.
#[async_trait]
pub trait CompactionSynthesizer: Send + Sync {
    /// Synthesize one distilled body from `sources`, optionally guided by
    /// caller `instructions`. Implementations must not fabricate facts not
    /// present in `sources` — this is a summarization boundary, not a
    /// generative one.
    async fn synthesize(
        &self,
        sources: &[MemoryRecord],
        instructions: Option<&str>,
    ) -> std::result::Result<String, String>;
}

fn llm_unavailable(strategy: &str) -> ApiError {
    ApiError::new(
        "memory.llm_unavailable",
        ErrorStage::Validation,
        format!(
            "compact strategy {strategy:?} requires an injected CompactionSynthesizer; \
             none is configured for this store"
        ),
    )
}

fn synthesis_failed(message: impl Into<String>) -> ApiError {
    ApiError::new(
        "memory.compaction_synthesis_failed",
        ErrorStage::Synthesizing,
        message.into(),
    )
}

/// Merge `request.memory_ids` into one new memory.
///
/// `"concatenate"` is deterministic — it joins each source memory's body
/// under a `[memory_id] body` heading. `"semantic_summary"` calls the
/// store's injected [`CompactionSynthesizer`] (contract R3-20); its output is
/// passed back through the same fail-closed redaction boundary as
/// `remember`/`update` before being persisted.
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
            ErrorStage::Validation,
            format!(
                "compact strategy {:?} is not implemented; supported: {SUPPORTED_STRATEGIES:?}",
                request.strategy
            ),
        ));
    }

    let now_secs = store.clock().now_epoch_secs();
    let now = request.timestamp.0.clone();

    // Read the synthesis inputs without holding the store mutex across the
    // potentially slow async synthesizer. The write phase below re-reads any
    // sources it must archive inside one IMMEDIATE transaction.
    let source_ids = request.memory_ids.clone();
    let sources = store
        .with_conn(move |conn| {
            let mut sources = Vec::with_capacity(source_ids.len());
            for memory_id in &source_ids {
                let record = SqliteMemoryStore::load_record(conn, &memory_id.0)?
                    .ok_or_else(|| not_found(&memory_id.0))?;
                sources.push(record);
            }
            Ok(sources)
        })
        .await?;

    let body = compute_compacted_body(store, &request, &sources).await?;

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
        visibility: most_restrictive_visibility(&sources),
        salience: sources
            .iter()
            .map(|record| record.salience)
            .fold(0.0f32, f32::max),
        scope: request.scope.clone(),
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
        title: request.title.clone(),
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
    // Compaction is one logical mutation: reserve SQLite's single writer once
    // and commit the new record plus all requested source archives atomically.
    // BEGIN IMMEDIATE also avoids repeatedly losing the writer race to job
    // heartbeats/status updates between independent autocommit statements.
    let compacted_for_write = compacted.clone();
    let request_for_write = request.clone();
    let memory_id_for_write = memory_id.clone();
    let now_for_write = now.clone();
    store
        .with_conn(move |conn| {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(|error| store_error(format!("begin compaction transaction: {error}")))?;

            let mut archive_sources = Vec::new();
            if request_for_write.archive_sources {
                archive_sources.reserve(request_for_write.memory_ids.len());
                for source_id in &request_for_write.memory_ids {
                    let source = SqliteMemoryStore::load_record(&tx, &source_id.0)?
                        .ok_or_else(|| not_found(&source_id.0))?;
                    archive_sources.push(source);
                }
            }

            insert_record(&tx, &compacted_for_write, &now_for_write)?;
            for mut source in archive_sources {
                source.status = MemoryStatus::Archived;
                source.history.push(MemoryHistoryEvent {
                    status: MemoryStatus::Archived,
                    message: format!("archived: compacted into {}", memory_id_for_write.0),
                    timestamp: request_for_write.timestamp.clone(),
                });
                update_record(&tx, &source, &now_for_write)?;
            }
            tx.commit()
                .map_err(|error| store_error(format!("commit compaction transaction: {error}")))?;
            Ok(())
        })
        .await?;

    finish_compaction(store, compacted, now_secs, now).await
}

async fn finish_compaction(
    store: &SqliteMemoryStore,
    compacted: MemoryRecord,
    now_secs: i64,
    now: String,
) -> Result<MemoryResult> {
    let score =
        crate::decay::score_record(&compacted, age_days(&compacted, now_secs), 0.0, 1.0, false);
    let (_, created, _) = age_and_bounds(&compacted, now_secs);
    crate::observe::emit(
        store.sink(),
        JobId::from(uuid::Uuid::new_v4()),
        MemoryPhase::Compacting,
        &compacted,
        Severity::Info,
        None,
        Some(score),
        None,
    )
    .await;
    Ok(result_from_record(&compacted, score, &created, &now))
}

/// Compute the compacted body per the requested strategy. Split out of
/// `compact` to keep it under the monolith function-length cap. The
/// `semantic_summary` path runs LLM output back through the same fail-closed
/// redaction boundary as every other durable memory write.
async fn compute_compacted_body(
    store: &SqliteMemoryStore,
    request: &MemoryCompactRequest,
    sources: &[MemoryRecord],
) -> Result<String> {
    if request.strategy == "semantic_summary" {
        let synthesizer = store
            .synthesizer()
            .ok_or_else(|| llm_unavailable(&request.strategy))?;
        let synthesized = synthesizer
            .synthesize(sources, request.instructions.as_deref())
            .await
            .map_err(synthesis_failed)?;
        let redactor = axon_core::redact::DefaultRedactor::new();
        let redaction_context = axon_core::redact::RedactionContext::memory_record();
        axon_core::redact::redact_text_checked(&redactor, &synthesized, &redaction_context).map_err(
            |_| {
                redaction_failed(format!(
                    "compacted body exceeds {} bytes; redaction cannot be safely verified",
                    axon_core::redact::MAX_REDACTABLE_TEXT_BYTES
                ))
            },
        )
    } else {
        Ok(sources
            .iter()
            .map(|record| format!("[{}] {}", record.memory_id.0, record.body))
            .collect::<Vec<_>>()
            .join("\n\n"))
    }
}

/// Bulk-import memory records, deduping by (body, memory_type, scope).
///
/// `dry_run` reports what would happen without writing. `ReplaceScope` first
/// archives every existing memory whose scope matches an incoming record's
/// scope, then inserts every incoming record (no dedup in replace mode —
/// the scope's prior content is being deliberately superseded).
///
/// Contract "Import and Export": "imported memories are marked with
/// provenance and may enter review state." Every non-dry-run imported record
/// is force-set to [`MemoryStatus::Review`] (regardless of the incoming
/// record's own `status`) with a provenance history event recording the
/// original status and scope it arrived with, so a caller cannot use import
/// to silently seed an `active` memory that skipped the redaction/review
/// path. Body/title are also run through the same fail-closed redaction
/// boundary as `remember` (contract "Security and Redaction": "avoid
/// bypassing `RedactionProvider`") — a record whose content cannot be
/// safely redacted is skipped (not persisted unredacted) and reported as a
/// warning.
pub async fn import(
    store: &SqliteMemoryStore,
    request: MemoryImportRequest,
) -> Result<MemoryImportResult> {
    let now = store.clock().now_rfc3339();
    let generated_ids: Vec<_> = (0..request.records.len())
        .map(|_| store.next_id())
        .collect();
    let now_for_write = now.clone();
    let (result, imported_records) = store
        .with_conn(move |conn| {
            let mode = request.mode;
            let dry_run = request.dry_run;
            let mut generated_ids = generated_ids.into_iter();
            let mut created = 0u32;
            let updated = 0u32;
            let mut skipped = 0u32;
            let mut created_ids = Vec::new();
            let mut warnings = Vec::new();
            let mut imported_records = Vec::new();
            let redactor = axon_core::redact::DefaultRedactor::new();
            let redaction_context = axon_core::redact::RedactionContext::memory_record();

            if mode == MemoryImportMode::ReplaceScope && !dry_run {
                let mut archived_scopes = std::collections::HashSet::new();
                for record in &request.records {
                    let scope_key = (record.scope.kind.clone(), record.scope.value.clone());
                    if !archived_scopes.insert(scope_key) {
                        continue;
                    }
                    archive_scope(conn, &record.scope, &now_for_write)?;
                }
            }

            // One `job_id` for the whole import call, shared by every imported
            // record's emit below — they all belong to the same logical import
            // request, not N unrelated single-event jobs.
            for incoming in request.records {
                let duplicate =
                    mode == MemoryImportMode::Merge && content_duplicate_exists(conn, &incoming)?;
                if duplicate {
                    skipped += 1;
                    continue;
                }
                if dry_run {
                    created += 1;
                    continue;
                }

                let Some(mut record) = prepare_import_record(
                    incoming,
                    &now_for_write,
                    &redactor,
                    &redaction_context,
                    &mut warnings,
                ) else {
                    skipped += 1;
                    continue;
                };
                let memory_id = generated_ids
                    .next()
                    .expect("one generated id per import record");
                record.memory_id = memory_id.clone();
                insert_record(conn, &record, &now_for_write)?;
                created += 1;
                created_ids.push(memory_id);
                imported_records.push(record);
            }
            Ok((
                MemoryImportResult {
                    created,
                    updated,
                    skipped,
                    dry_run,
                    created_ids,
                    warnings,
                },
                imported_records,
            ))
        })
        .await?;

    let job_id = JobId::from(uuid::Uuid::new_v4());
    for record in &imported_records {
        crate::observe::emit(
            store.sink(),
            job_id,
            MemoryPhase::Reviewing,
            record,
            Severity::Info,
            None,
            None,
            Some("imported"),
        )
        .await;
    }
    Ok(result)
}

fn prepare_import_record(
    mut record: MemoryRecord,
    now: &str,
    redactor: &axon_core::redact::DefaultRedactor,
    context: &axon_core::redact::RedactionContext,
    warnings: &mut Vec<SourceWarning>,
) -> Option<MemoryRecord> {
    let original_status = record.status;
    let scope = record.scope.kind.clone();
    let body = axon_core::redact::redact_text_checked(redactor, &record.body, context);
    let title = record
        .title
        .as_deref()
        .map(|value| axon_core::redact::redact_text_checked(redactor, value, context))
        .transpose();
    let (body, title) = match (body, title) {
        (Ok(body), Ok(title)) => (body, title),
        (Err(_), _) => {
            push_import_redaction_warning(warnings, &scope, "body");
            return None;
        }
        (_, Err(_)) => {
            push_import_redaction_warning(warnings, &scope, "title");
            return None;
        }
    };
    record.body = body;
    record.title = title;
    record.status = MemoryStatus::Review;
    record.history.push(MemoryHistoryEvent {
        status: MemoryStatus::Review,
        message: format!("imported: provenance=import original_status={original_status:?}"),
        timestamp: Timestamp(now.to_string()),
    });
    Some(record)
}

fn push_import_redaction_warning(warnings: &mut Vec<SourceWarning>, scope: &str, field: &str) {
    warnings.push(SourceWarning {
        code: "memory.import_redaction_failed".to_string(),
        severity: Severity::Warning,
        message: format!(
            "skipped import record (scope={scope}): {field} could not be safely redacted"
        ),
        source_item_key: None,
        retryable: false,
    });
}

/// Export memory records matching an optional scope filter.
///
/// `working`-status memories are excluded by default (contract "Type
/// rules": "working memories are excluded from long-term exports by
/// default"), toggled via `request.include_working`.
pub async fn export(
    store: &SqliteMemoryStore,
    request: MemoryExportRequest,
) -> Result<MemoryExportResult> {
    store
        .with_conn(move |conn| {
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
                let Some(record) = SqliteMemoryStore::load_record(conn, &id)? else {
                    continue;
                };
                if !request.include_archived && record.status == MemoryStatus::Archived {
                    continue;
                }
                if !request.include_working && record.status == MemoryStatus::Working {
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
            // Artifact backing (contract "Import and Export": "export writes an
            // artifact or stream ... according to caller scope") is layered on at
            // the `axon-services` boundary, which owns artifact-root resolution and
            // caller-scope redaction policy — this crate does not own filesystem
            // artifact writing (see the crate's "Boundary — keep OUT" list).
            Ok(MemoryExportResult {
                records,
                count,
                artifact: None,
            })
        })
        .await
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

/// Compacted memories inherit the most restrictive visibility of their
/// sources — merging never widens who can see the combined body (a
/// `sensitive` source memory folded into a `public` compaction result would
/// silently leak it).
fn most_restrictive_visibility(sources: &[MemoryRecord]) -> Visibility {
    fn rank(v: Visibility) -> u8 {
        match v {
            Visibility::Sensitive => 4,
            Visibility::Redacted => 3,
            Visibility::Internal => 2,
            Visibility::Derived => 1,
            Visibility::Public => 0,
        }
    }
    sources
        .iter()
        .map(|record| record.visibility)
        .max_by_key(|v| rank(*v))
        .unwrap_or(Visibility::Internal)
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
