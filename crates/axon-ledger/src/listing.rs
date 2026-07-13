//! Shared filter/paginate helpers for `LedgerStore::list_sources`.
//!
//! Both [`crate::store::FakeLedgerStore`] and [`crate::sqlite::SqliteLedgerStore`]
//! decode their registered sources into an in-memory `Vec<SourceSummary>` and
//! then call [`list_page`] so the two implementations can never drift apart on
//! filter or pagination semantics — see the `list_sources` entry in
//! `docs/pipeline-unification/runtime/ledger-contract.md`'s Public Boundary.

use axon_api::source::*;

/// Default page size when `request.limit` is unset.
pub const DEFAULT_LIST_SOURCES_LIMIT: u32 = 50;
/// Hard cap on page size regardless of what the caller requests.
pub const MAX_LIST_SOURCES_LIMIT: u32 = 500;

/// Clamp a requested page size into `[1, MAX_LIST_SOURCES_LIMIT]`, defaulting
/// to [`DEFAULT_LIST_SOURCES_LIMIT`] when unset.
pub fn resolve_limit(requested: Option<u32>) -> u32 {
    requested
        .unwrap_or(DEFAULT_LIST_SOURCES_LIMIT)
        .clamp(1, MAX_LIST_SOURCES_LIMIT)
}

/// True when `source` satisfies every filter set on `request`. Filters left
/// unset (`None`/empty) act as wildcards; all set filters are ANDed together.
pub fn matches_request(source: &SourceSummary, request: &SourceListRequest) -> bool {
    if let Some(kind) = request.source_kind
        && source.source_kind != kind
    {
        return false;
    }
    if let Some(adapter) = request.adapter.as_deref()
        && !source.adapter.name.eq_ignore_ascii_case(adapter)
    {
        return false;
    }
    if let Some(status) = request.status
        && source.status != status
    {
        return false;
    }
    if let Some(authority) = request.authority
        && source.authority != authority
    {
        return false;
    }
    if let Some(watch_enabled) = request.watch_enabled
        && source.watch_id.is_some() != watch_enabled
    {
        return false;
    }
    if let Some(tag) = request.tag.as_deref()
        && !source.tags.iter().any(|t| t.eq_ignore_ascii_case(tag))
    {
        return false;
    }
    if let Some(query) = request.query.as_deref() {
        let needle = query.trim().to_lowercase();
        if !needle.is_empty() {
            let haystack =
                format!("{} {}", source.canonical_uri, source.display_name).to_lowercase();
            if !haystack.contains(&needle) {
                return false;
            }
        }
    }
    true
}

/// Filter `sources` against `request`, sort by `source_id` ascending for a
/// stable cursor order, then slice out one page.
///
/// The cursor is the last-returned `source_id` from the previous page: this
/// page starts with the first source strictly greater than it. `total`
/// reports the full filtered count, not just the count in this page.
pub fn list_page(sources: Vec<SourceSummary>, request: &SourceListRequest) -> Page<SourceSummary> {
    let mut matched: Vec<SourceSummary> = sources
        .into_iter()
        .filter(|source| matches_request(source, request))
        .collect();
    matched.sort_by(|a, b| a.source_id.0.cmp(&b.source_id.0));
    let total = matched.len() as u64;
    let limit = resolve_limit(request.limit);

    let start = match request.cursor.as_deref() {
        Some(cursor) => matched.partition_point(|source| source.source_id.0.as_str() <= cursor),
        None => 0,
    };
    let remaining_before_page = matched.len() - start;
    let items: Vec<SourceSummary> = matched
        .into_iter()
        .skip(start)
        .take(limit as usize)
        .collect();
    let next_cursor = if remaining_before_page > items.len() {
        items.last().map(|source| source.source_id.0.clone())
    } else {
        None
    };

    Page {
        items,
        next_cursor,
        limit,
        total: Some(total),
    }
}

#[cfg(test)]
#[path = "listing_tests.rs"]
mod tests;
