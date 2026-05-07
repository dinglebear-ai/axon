use crate::core::config::Config;
use std::collections::HashMap;
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

struct DedupeRecord {
    id: String,
    scraped_at: String,
}

/// Remove duplicate points that share the same (url, chunk_index) key.
///
/// **Performance**: O(n) full collection scroll -- on large collections (millions of
/// points) this can take 60-120+ seconds. This is inherent to deduplication and
/// cannot be replaced with a facet query.
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

    let mut by_key: HashMap<(String, i64), Vec<DedupeRecord>> = HashMap::new();
    // Selective payload: only fetch the fields needed for dedup (url, chunk_index,
    // scraped_at). Avoids transferring multi-KB chunk_text per point — ~28x less
    // data on a 7M-point collection.
    qdrant_scroll_pages_selective(
        cfg,
        serde_json::json!({"include": ["url", "chunk_index", "scraped_at"]}),
        |points| {
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
                let chunk_index = payload
                    .get("chunk_index")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                let scraped_at = payload
                    .get("scraped_at")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                by_key
                    .entry((url, chunk_index))
                    .or_default()
                    .push(DedupeRecord { id, scraped_at });
            }
            true
        },
    )
    .await?;

    let mut to_delete: Vec<String> = Vec::new();
    let mut dup_groups = 0usize;
    for mut records in by_key.into_values() {
        if records.len() <= 1 {
            continue;
        }
        dup_groups += 1;
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
