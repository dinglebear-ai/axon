use crate::core::config::Config;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::sync::atomic::{AtomicBool, Ordering};

use super::super::client::{qdrant_delete_points, qdrant_scroll_pages_selective};
use super::super::utils::payload_url;

static DEDUPE_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

/// RAII guard that resets DEDUPE_IN_PROGRESS to false when dropped.
/// Ensures the flag is cleared even if dedupe_payload returns an error.
struct DedupeGuard;

impl Drop for DedupeGuard {
    fn drop(&mut self) {
        DEDUPE_IN_PROGRESS.store(false, Ordering::Release);
    }
}

/// Compact per-point record, only allocated for duplicate keys (pass 2).
struct DedupeRecord {
    id: String,
    /// RFC3339 string — lexicographic ordering is correct for ISO8601 timestamps.
    scraped_at: String,
}

/// FNV-1a 64-bit hash of a URL string used as a compact map key.
/// Avoids heap-allocating the full URL string per map entry. Fixed seed
/// ensures stability within a single dedupe run (keys are never persisted).
#[inline]
fn fnv64_url(s: &str) -> u64 {
    const FNV_OFFSET: u64 = 14_695_981_039_346_656_037;
    const FNV_PRIME: u64 = 1_099_511_628_211;
    let mut hash = FNV_OFFSET;
    for b in s.bytes() {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// Remove duplicate points that share the same `(url, chunk_index)` key.
///
/// **Memory**: Two-pass approach — pass 1 counts occurrences using a compact
/// `(fnv64(url), chunk_index)` key with no per-point String allocations; pass 2
/// scrolls again and allocates `DedupeRecord`s only for keys with count > 1.
/// At 2.5M points with ~1% duplicates this saves roughly 10× peak RSS compared
/// to a single-pass approach that stores records for every point.
///
/// **Performance**: O(n) full collection scroll — on large collections (millions
/// of points) this can take 60-120+ seconds. This is inherent to deduplication
/// and cannot be replaced with a facet query.
pub async fn dedupe_payload(
    cfg: &Config,
) -> Result<serde_json::Value, Box<dyn Error + Send + Sync>> {
    // Prevent concurrent deduplication runs — two simultaneous full-collection
    // scrolls race on deletes and produce misleading duplicate counts.
    if DEDUPE_IN_PROGRESS
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        return Err("deduplication already in progress for this process".into());
    }
    let _guard = DedupeGuard;

    // Selective payload: only fetch the fields needed for dedup (url,
    // chunk_index, scraped_at). Avoids transferring multi-KB chunk_text
    // per point — ~28x less data on a 7M-point collection.
    let with_payload = serde_json::json!({"include": ["url", "chunk_index", "scraped_at"]});

    // Pass 1: count occurrences per compact key — no record storage.
    // (fnv64(url), chunk_index) avoids heap-allocating one String per key entry.
    let mut counts: HashMap<(u64, i64), u32> = HashMap::new();
    qdrant_scroll_pages_selective(cfg, with_payload.clone(), |points| {
        for p in points {
            let Some(payload) = p.get("payload") else {
                continue;
            };
            let url = payload_url(payload);
            if url.is_empty() {
                continue;
            }
            let ci = payload
                .get("chunk_index")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            *counts.entry((fnv64_url(&url), ci)).or_insert(0) += 1;
        }
        true
    })
    .await?;

    // Identify keys with duplicates (count > 1). Keys with count == 1
    // (~99%+ of keys) are filtered out so pass 2 skips allocating records
    // for unique points — the primary memory saving of this approach.
    let dup_keys: HashSet<(u64, i64)> = counts
        .into_iter()
        .filter_map(|(k, n)| if n > 1 { Some(k) } else { None })
        .collect();

    if dup_keys.is_empty() {
        return Ok(serde_json::json!({
            "duplicate_groups": 0,
            "deleted": 0,
            "collection": cfg.collection,
        }));
    }

    // Pass 2: collect records only for duplicate keys.
    let mut by_key: HashMap<(u64, i64), Vec<DedupeRecord>> = HashMap::new();
    qdrant_scroll_pages_selective(cfg, with_payload, |points| {
        for p in points {
            let id = p
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if id.is_empty() {
                continue;
            }
            let Some(payload) = p.get("payload") else {
                continue;
            };
            let url = payload_url(payload);
            if url.is_empty() {
                continue;
            }
            let ci = payload
                .get("chunk_index")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let key = (fnv64_url(&url), ci);
            if !dup_keys.contains(&key) {
                continue;
            }
            let scraped_at = payload
                .get("scraped_at")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            by_key
                .entry(key)
                .or_default()
                .push(DedupeRecord { id, scraped_at });
        }
        true
    })
    .await?;

    let mut to_delete: Vec<String> = Vec::new();
    let mut dup_groups = 0usize;
    for mut records in by_key.into_values() {
        if records.len() <= 1 {
            continue;
        }
        dup_groups += 1;
        // Keep the most-recently-scraped copy; delete the rest.
        // RFC3339 strings sort lexicographically in chronological order.
        records.sort_unstable_by(|a, b| b.scraped_at.cmp(&a.scraped_at));
        to_delete.extend(records.into_iter().skip(1).map(|r| r.id));
    }

    let deleted = qdrant_delete_points(cfg, &to_delete).await?;

    Ok(serde_json::json!({
        "duplicate_groups": dup_groups,
        "deleted": deleted,
        "collection": cfg.collection,
    }))
}

/// Select the IDs to delete from a duplicate group.
///
/// Given a list of `(id, scraped_at)` pairs sharing the same `(url, chunk_index)`
/// key, returns the IDs of all but the most-recently-scraped entry (those are
/// the stale copies to remove). Returns an empty Vec when there is ≤1 record.
///
/// Extracted for unit-testability — the actual deletion is deferred to the
/// caller so this function has no I/O side-effects.
#[cfg(test)]
pub(crate) fn select_stale_ids(records: Vec<(String, String)>) -> Vec<String> {
    if records.len() <= 1 {
        return Vec::new();
    }
    let mut recs = records;
    // Sort descending by scraped_at (RFC3339 — lexicographic = chronological).
    recs.sort_unstable_by(|a, b| b.1.cmp(&a.1));
    recs.into_iter().skip(1).map(|(id, _)| id).collect()
}

#[cfg(test)]
#[path = "dedupe_tests.rs"]
mod tests;
