//! Recall paths: keyword search, bounded context assembly, and review queue.
//!
//! Scoring uses [`crate::decay::score_record`]. Semantic score is a keyword
//! overlap proxy here (real vector recall is layered in by `axon-services`
//! through `VectorStore`; this crate owns the ranking blend, not embeddings).
//!
//! Recall rules enforced (contract "Scoring and Recall"):
//! - forgotten memories never return
//! - superseded memories return only when explicitly requested
//! - archived memories excluded unless `include_archived`
//! - working memories excluded from context unless `include_working`

use axon_api::source::*;
use rusqlite::Connection;

use crate::record::age_days;
use crate::sqlite::SqliteMemoryStore;
use crate::sqlite::error::store_error;
use crate::store::Result;

/// Keyword search with contract recall filtering + scoring.
pub async fn search(
    store: &SqliteMemoryStore,
    request: MemorySearchRequest,
) -> Result<MemorySearchResult> {
    let now_secs = store.clock().now_epoch_secs();
    let scope_filter = request
        .filters
        .get("scope")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let conn = store.conn().lock().await;
    let records = load_all(&conn)?;
    drop(conn);

    let query = request.query.to_lowercase();
    let query_terms: Vec<&str> = query.split_whitespace().collect();

    let mut matches: Vec<MemorySearchMatch> = records
        .into_iter()
        .filter(|record| {
            recall_visible(record, request.include_archived, &request.include_statuses)
        })
        .filter(|record| scope_matches_filter(record, scope_filter.as_deref()))
        .filter_map(|record| {
            let semantic = keyword_semantic(&record, &query, &query_terms);
            // Require some textual relevance for a non-empty query.
            if !query.trim().is_empty() && semantic <= 0.0 {
                return None;
            }
            let age = age_days(&record, now_secs);
            let scope_match = scope_match_score(&record, scope_filter.as_deref());
            let score = crate::decay::score_record(
                &record,
                age,
                semantic,
                scope_match,
                request.include_archived,
            );
            Some(MemorySearchMatch { record, score })
        })
        .collect();

    matches.sort_by(|a, b| b.score.total_cmp(&a.score));
    matches.truncate(request.limit.max(1) as usize);

    let mut warnings = Vec::new();
    if matches
        .iter()
        .any(|m| m.record.status == MemoryStatus::Contradicted)
    {
        warnings.push(SourceWarning {
            code: "memory.contradicted".to_string(),
            severity: Severity::Warning,
            message: "results include contradicted memories".to_string(),
            source_item_key: None,
            retryable: false,
        });
    }

    Ok(MemorySearchResult {
        results: matches,
        query_embedding_model: Some("keyword-overlap".to_string()),
        graph: None,
        warnings,
    })
}

/// Bounded context assembly for ask/session flows.
pub async fn context(
    store: &SqliteMemoryStore,
    request: MemoryContextRequest,
) -> Result<MemoryContextResult> {
    let now_secs = store.clock().now_epoch_secs();
    let conn = store.conn().lock().await;
    let records = load_all(&conn)?;
    drop(conn);

    let query = request.query.clone().unwrap_or_default().to_lowercase();
    let query_terms: Vec<&str> = query.split_whitespace().collect();

    let mut scored: Vec<(f32, MemoryRecord)> = records
        .into_iter()
        .filter(|record| context_visible(record, request.include_working))
        .map(|record| {
            let semantic = keyword_semantic(&record, &query, &query_terms);
            let age = age_days(&record, now_secs);
            let score = crate::decay::score_record(&record, age, semantic, 1.0, false);
            (score, record)
        })
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));

    let mut memories = Vec::new();
    let mut fragments = Vec::new();
    let mut used_tokens: u32 = 0;
    let mut exclusions = Vec::new();
    for (_, record) in scored {
        let fragment = format!("[{}] {}", record.memory_id.0, record.body);
        let cost = estimate_tokens(&fragment);
        if used_tokens + cost > request.token_budget {
            if !exclusions.contains(&"token_budget".to_string()) {
                exclusions.push("token_budget".to_string());
            }
            continue;
        }
        used_tokens += cost;
        fragments.push(fragment);
        memories.push(record);
    }

    let context = fragments.join("\n");
    Ok(MemoryContextResult {
        token_estimate: estimate_tokens(&context),
        context,
        memories,
        exclusions,
        warnings: Vec::new(),
    })
}

/// The current review queue (open reviews joined to their memory records).
pub async fn review(
    store: &SqliteMemoryStore,
    request: MemoryReviewRequest,
) -> Result<MemoryReviewResult> {
    let now_secs = store.clock().now_epoch_secs();
    let limit = request.limit.unwrap_or(50).max(1);
    let conn = store.conn().lock().await;
    let mut stmt = conn
        .prepare(
            "SELECT DISTINCT memory_id FROM memory_reviews
             WHERE resolved = 0 ORDER BY id LIMIT ?1",
        )
        .map_err(|e| store_error(format!("prepare review: {e}")))?;
    let ids: Vec<String> = stmt
        .query_map([limit], |row| row.get::<_, String>(0))
        .map_err(|e| store_error(format!("query review: {e}")))?
        .collect::<std::result::Result<_, _>>()
        .map_err(|e| store_error(format!("review row: {e}")))?;

    let mut memories = Vec::new();
    for id in ids {
        if let Some(record) = SqliteMemoryStore::load_record(&conn, &id)? {
            if let Some(mt) = request.memory_type
                && record.memory_type != mt
            {
                continue;
            }
            memories.push(record);
        }
    }
    let _ = now_secs;
    Ok(MemoryReviewResult {
        memories,
        cursor: None,
        warnings: Vec::new(),
    })
}

fn load_all(conn: &Connection) -> Result<Vec<MemoryRecord>> {
    let mut stmt = conn
        .prepare("SELECT memory_id FROM memory_records ORDER BY created_at, memory_id")
        .map_err(|e| store_error(format!("prepare all: {e}")))?;
    let ids: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| store_error(format!("query all: {e}")))?
        .collect::<std::result::Result<_, _>>()
        .map_err(|e| store_error(format!("all row: {e}")))?;
    let mut records = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(record) = SqliteMemoryStore::load_record(conn, &id)? {
            records.push(record);
        }
    }
    Ok(records)
}

/// Search visibility (contract "Recall rules"): forgotten never returns;
/// superseded/contradicted return only when explicitly opted in via
/// `include_statuses`; archived only when `include_archived` or explicitly
/// requested.
fn recall_visible(
    record: &MemoryRecord,
    include_archived: bool,
    include_statuses: &[MemoryStatus],
) -> bool {
    match record.status {
        MemoryStatus::Forgotten => false,
        MemoryStatus::Superseded => include_statuses.contains(&MemoryStatus::Superseded),
        MemoryStatus::Archived => {
            include_archived || include_statuses.contains(&MemoryStatus::Archived)
        }
        _ => true,
    }
}

/// Context visibility: exclude forgotten/superseded/archived always; exclude
/// working unless requested.
fn context_visible(record: &MemoryRecord, include_working: bool) -> bool {
    match record.status {
        MemoryStatus::Forgotten | MemoryStatus::Superseded | MemoryStatus::Archived => false,
        MemoryStatus::Working => include_working,
        _ => true,
    }
}

/// Keyword-overlap proxy for semantic similarity, in `0.0..=1.0`.
fn keyword_semantic(record: &MemoryRecord, query: &str, terms: &[&str]) -> f32 {
    if query.trim().is_empty() {
        return 0.0;
    }
    let body = record.body.to_lowercase();
    if body.contains(query) {
        return 1.0;
    }
    if terms.is_empty() {
        return 0.0;
    }
    let hits = terms.iter().filter(|t| body.contains(**t)).count();
    hits as f32 / terms.len() as f32
}

/// When a `scope` filter is set, only records with a matching scope value pass.
fn scope_matches_filter(record: &MemoryRecord, scope_filter: Option<&str>) -> bool {
    match scope_filter {
        Some(value) => record.scope.value == value,
        None => true,
    }
}

/// Scope-match input to scoring: exact scope hit = 1.0, global = 0.5, else 0.25.
/// Narrower scope matches rank higher (contract "Scope rules").
fn scope_match_score(record: &MemoryRecord, scope_filter: Option<&str>) -> f32 {
    if let Some(value) = scope_filter
        && record.scope.value == value
    {
        return 1.0;
    }
    match record.scope.kind.as_str() {
        "global" | "" => 0.5,
        _ => 0.25,
    }
}

/// Rough token estimate: whitespace-delimited word count.
fn estimate_tokens(text: &str) -> u32 {
    text.split_whitespace().count() as u32
}
