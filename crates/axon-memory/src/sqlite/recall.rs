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
use std::collections::BTreeMap;

use rusqlite::{Connection, named_params};

use crate::record::age_days;
use crate::sqlite::SqliteMemoryStore;
use crate::sqlite::error::store_error;
use crate::store::Result;

const MAX_RECALL_CANDIDATES: i64 = 4_096;

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

    let filters = request.filters.clone();
    let candidate_query = request.query.clone();
    let candidate_limit = request.limit;
    let records = store
        .with_conn(move |conn| load_candidates(conn, &filters, &candidate_query, candidate_limit))
        .await?;

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
        warnings.push(contradiction_warning(
            "results include contradicted memories",
        ));
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
    // Context ranking needs lower-scoring fallback candidates so it can report
    // budget exclusions; the query affects scoring, not candidate eligibility.
    let candidate_query = String::new();
    let candidate_limit = request.token_budget.max(1);
    let candidate_filters = request.filters.clone();
    let records = store
        .with_conn(move |conn| {
            load_candidates(conn, &candidate_filters, &candidate_query, candidate_limit)
        })
        .await?;

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
    // Contract "Recall rules": "contradicted memories return only with
    // warning unless resolved" — this is a general recall rule, not specific
    // to `search()`. `context_visible` deliberately does not filter
    // `Contradicted` out (only forgotten/superseded/archived are in the
    // contract's default context-exclusion list), so — matching `search()`
    // above — flag it with the same `memory.contradicted` warning instead of
    // silently including it.
    let mut warnings = Vec::new();
    if memories
        .iter()
        .any(|record| record.status == MemoryStatus::Contradicted)
    {
        warnings.push(contradiction_warning(
            "context includes contradicted memories",
        ));
    }

    Ok(MemoryContextResult {
        token_estimate: estimate_tokens(&context),
        context,
        memories,
        exclusions,
        warnings,
    })
}

/// Shared `memory.contradicted` warning for recall paths that include
/// contradicted memories (contract "Recall rules": "contradicted memories
/// return only with warning unless resolved").
fn contradiction_warning(message: &str) -> SourceWarning {
    SourceWarning {
        code: "memory.contradicted".to_string(),
        severity: Severity::Warning,
        message: message.to_string(),
        source_item_key: None,
        retryable: false,
    }
}

/// The current review queue (open reviews joined to their memory records).
pub async fn review(
    store: &SqliteMemoryStore,
    request: MemoryReviewRequest,
) -> Result<MemoryReviewResult> {
    store
        .with_conn(move |conn| review_blocking(conn, request))
        .await
}

fn review_blocking(conn: &Connection, request: MemoryReviewRequest) -> Result<MemoryReviewResult> {
    let limit = request.limit.unwrap_or(50).max(1);
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
        if let Some(record) = SqliteMemoryStore::load_record(conn, &id)? {
            if let Some(mt) = request.memory_type
                && record.memory_type != mt
            {
                continue;
            }
            memories.push(record);
        }
    }
    Ok(MemoryReviewResult {
        memories,
        cursor: None,
        warnings: Vec::new(),
    })
}

fn load_candidates(
    conn: &Connection,
    filters: &MetadataMap,
    query: &str,
    requested_limit: u32,
) -> Result<Vec<MemoryRecord>> {
    let string_filter = |key: &str| filters.get(key).and_then(|value| value.as_str());
    let project = string_filter("project");
    let repo = string_filter("repo");
    let file = string_filter("file");
    let memory_type = string_filter("memory_type");
    let status = string_filter("status");
    let scope = string_filter("scope");
    let terms_json = (!query.trim().is_empty())
        .then(|| {
            serde_json::to_string(&query.split_whitespace().collect::<Vec<_>>())
                .map_err(|e| store_error(format!("encode memory search terms: {e}")))
        })
        .transpose()?;
    let candidate_limit = i64::from(requested_limit.max(1))
        .saturating_mul(8)
        .min(MAX_RECALL_CANDIDATES);
    let mut stmt = conn
        .prepare(
            "SELECT * FROM memory_records r
             WHERE (:project IS NULL OR EXISTS (
                       SELECT 1 FROM memory_links l WHERE l.memory_id = r.memory_id
                         AND l.link_type = 'project' AND l.target = :project))
               AND (:repo IS NULL OR EXISTS (
                       SELECT 1 FROM memory_links l WHERE l.memory_id = r.memory_id
                         AND l.link_type = 'repo' AND l.target = :repo))
               AND (:file IS NULL OR EXISTS (
                       SELECT 1 FROM memory_links l WHERE l.memory_id = r.memory_id
                         AND l.link_type = 'file' AND l.target = :file))
               AND (:memory_type IS NULL OR r.memory_type = :memory_type)
               AND (:status IS NULL OR r.status = :status)
               AND (:scope IS NULL OR r.scope_value = :scope)
               AND (:terms_json IS NULL OR EXISTS (
                       SELECT 1 FROM json_each(:terms_json) term
                       WHERE lower(r.body) LIKE '%' || lower(term.value) || '%'
                          OR lower(coalesce(r.title, '')) LIKE '%' || lower(term.value) || '%'))
             ORDER BY r.updated_at DESC, r.memory_id
             LIMIT :candidate_limit",
        )
        .map_err(|e| store_error(format!("prepare bounded memory recall: {e}")))?;
    let mut rows = stmt
        .query(named_params! {
            ":project": project,
            ":repo": repo,
            ":file": file,
            ":memory_type": memory_type,
            ":status": status,
            ":scope": scope,
            ":terms_json": terms_json,
            ":candidate_limit": candidate_limit,
        })
        .map_err(|e| store_error(format!("query bounded memory recall: {e}")))?;
    let mut records = Vec::new();
    while let Some(row) = rows
        .next()
        .map_err(|e| store_error(format!("bounded memory recall row: {e}")))?
    {
        records.push(crate::sqlite::rows::record_from_row(row, Vec::new())?);
    }
    drop(rows);
    drop(stmt);
    if records.is_empty() {
        return Ok(records);
    }
    let ids_json = serde_json::to_string(
        &records
            .iter()
            .map(|record| record.memory_id.0.as_str())
            .collect::<Vec<_>>(),
    )
    .map_err(|e| store_error(format!("encode bounded memory ids: {e}")))?;
    let mut links_stmt = conn
        .prepare(
            "SELECT memory_id, link_type, target, confidence, evidence_json
             FROM memory_links
             WHERE memory_id IN (SELECT value FROM json_each(?1))
             ORDER BY memory_id, id",
        )
        .map_err(|e| store_error(format!("prepare bounded memory links: {e}")))?;
    let mut links_by_id = BTreeMap::<String, Vec<MemoryLink>>::new();
    let link_rows = links_stmt
        .query_map([ids_json], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, f64>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .map_err(|e| store_error(format!("query bounded memory links: {e}")))?;
    for row in link_rows {
        let (memory_id, link_type, target, confidence, evidence_json) =
            row.map_err(|e| store_error(format!("bounded memory link row: {e}")))?;
        let evidence = serde_json::from_str(&evidence_json)
            .map_err(|e| store_error(format!("decode bounded memory evidence: {e}")))?;
        links_by_id.entry(memory_id).or_default().push(MemoryLink {
            link_type,
            target,
            confidence: confidence as f32,
            evidence,
        });
    }
    for record in &mut records {
        record.links = links_by_id.remove(&record.memory_id.0).unwrap_or_default();
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
pub(crate) fn scope_match_score(record: &MemoryRecord, scope_filter: Option<&str>) -> f32 {
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
