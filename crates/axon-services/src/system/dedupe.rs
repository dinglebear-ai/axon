//! Dedupe near-identical chunks within a Qdrant collection.
//!
//! Per the pruning contract (`docs/pipeline-unification/runtime/pruning-contract.md`
//! §"Dedupe"), dedupe is a prune operation with a non-source selector and must
//! go through `axon-prune`'s plan/execute path. This facade wraps the
//! duplicate-detection/deletion scan in a single-step `PrunePlan` driven by
//! `PruneExecutor` so it gets the same admin gate and execution accounting
//! every other destructive prune passes through.
//!
//! The two-pass duplicate scan is driven by `axon-vectors`'
//! [`axon_vectors::qdrant::QdrantVectorStore`] scroll primitives (ports legacy
//! `axon-vector`'s `dedupe_payload`, which drove the identical algorithm over
//! the legacy client) and deletes matched duplicates via the store's generic
//! [`axon_vectors::store::VectorStore::delete`] with a
//! `VectorDeleteSelector::Points` batch — the duplicate-detection/deletion
//! LOGIC itself (FNV-keyed two-pass scan, keep-newest-by-`scraped_at`) is
//! unchanged, and the wire response is still the same `DedupeResult` shape.

use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use axon_api::source::ids::{JobId, SourceGenerationId, VectorPointId};
use axon_api::source::prune::{PrunePlan, PruneSelector, PruneStep, PruneTargetKind};
use axon_api::source::vector::VectorDeleteSelector;
use axon_core::config::Config;
use axon_prune::{PruneAuthz, PruneExecutor, PruneTarget, StepExecution};
use axon_vectors::qdrant::QdrantVectorStore;
use axon_vectors::store::VectorStore;
use uuid::Uuid;

use super::canonical_uri_from_payload;
use crate::events::{LogLevel, ServiceEvent, emit};
use crate::types::DedupeResult;

/// Points deleted per `points/delete` request — mirrors legacy
/// `qdrant_delete_points`'s 1000-id batch chunking.
const DELETE_BATCH_SIZE: usize = 1000;

/// Payload page size for the two dedupe scroll passes — matches legacy
/// `qdrant_scroll_pages_selective`'s fixed 256-point page.
const SCROLL_PAGE_LIMIT: usize = 256;

static DEDUPE_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

/// RAII guard that resets DEDUPE_IN_PROGRESS to false when dropped.
/// Ensures the flag is cleared even if the scan returns an error.
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

/// FNV-1a 64-bit hash of a canonical URI string used as a compact map key.
/// Avoids heap-allocating the full URI string per map entry. Fixed seed
/// ensures stability within a single dedupe run (keys are never persisted).
#[inline]
fn fnv64_uri(s: &str) -> u64 {
    const FNV_OFFSET: u64 = 14_695_981_039_346_656_037;
    const FNV_PRIME: u64 = 1_099_511_628_211;
    let mut hash = FNV_OFFSET;
    for b in s.bytes() {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

#[must_use = "dedupe returns a Result that should be handled"]
pub async fn dedupe(
    cfg: &Config,
    tx: Option<tokio::sync::mpsc::Sender<ServiceEvent>>,
) -> Result<DedupeResult, Box<dyn Error>> {
    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: "starting dedupe".to_string(),
        },
    )
    .await;

    let plan = PrunePlan {
        job_id: JobId::new(Uuid::new_v4()),
        selector: PruneSelector::Collection {
            collection: cfg.collection.clone(),
        },
        destructive: true,
        requires_admin: true,
        estimated: Default::default(),
        steps: vec![PruneStep {
            target: PruneTargetKind::Vector,
            description: "dedupe near-identical chunks".to_string(),
            estimated_deletes: 0,
            vector_selector: None,
            source_id: None,
            generation: None,
            graph_stable_keys: None,
            graph_edge_ids: None,
            memory_ids: None,
        }],
        warnings: Vec::new(),
    };

    let out: Mutex<Option<(usize, usize)>> = Mutex::new(None);
    let executor = PruneExecutor::new(DedupeExecTarget { cfg, out: &out });

    // System-trusted authorization: both callers of this facade — REST
    // `/v1/prune/dedupe` (router-level `require_admin_scope` layer in
    // `axon-web`'s `admin_routes`) and MCP `prune subaction=dedupe` (the
    // `CURRENT_PRUNE_AUTHZ` task-local resolved from the caller's real scopes
    // in `axon-mcp`'s `call_tool`) — already enforce `axon:admin` *before*
    // this function is ever reached. Passing `PruneAuthz::admin()` explicitly
    // here (never implicitly defaulted) mirrors the same documented,
    // system-trusted pattern used by the cleanup-debt drain in
    // `crate::source::prune`.
    let authz = PruneAuthz::admin();

    let outcome = executor.execute(&plan, &authz).await;

    match outcome {
        Ok(_) => {
            let (duplicate_groups, deleted) = out
                .into_inner()
                .expect("dedupe mutex poisoned")
                .unwrap_or((0, 0));
            emit(
                &tx,
                ServiceEvent::Log {
                    level: LogLevel::Info,
                    message: format!(
                        "completed dedupe: {duplicate_groups} groups, {deleted} deleted"
                    ),
                },
            )
            .await;
            Ok(DedupeResult {
                completed: true,
                duplicate_groups,
                deleted,
            })
        }
        Err(denied) => {
            let msg = format!("dedupe failed: {denied}");
            emit(
                &tx,
                ServiceEvent::Log {
                    level: LogLevel::Error,
                    message: msg.clone(),
                },
            )
            .await;
            Err(msg.into())
        }
    }
}

/// [`PruneTarget`] that drives the real duplicate scan+delete over
/// `axon-vectors`. Single-step, so `apply()` is called exactly once;
/// `duplicate_groups` (which doesn't fit [`StepExecution`]'s plain delete
/// count) is stashed in `out` for the caller to read back after `execute()`
/// returns.
struct DedupeExecTarget<'a> {
    cfg: &'a Config,
    out: &'a Mutex<Option<(usize, usize)>>,
}

#[async_trait]
impl PruneTarget for DedupeExecTarget<'_> {
    async fn current_generation(
        &self,
        _source_id: Option<&str>,
    ) -> Result<Option<SourceGenerationId>, String> {
        // Dedupe is collection-wide, not source/generation scoped — nothing
        // to fence against.
        Ok(None)
    }

    async fn apply(&self, _step: &PruneStep) -> Result<StepExecution, String> {
        let (duplicate_groups, deleted) = dedupe_collection(self.cfg)
            .await
            .map_err(|e| e.to_string())?;
        *self.out.lock().expect("dedupe mutex poisoned") = Some((duplicate_groups, deleted));
        Ok(StepExecution::deleted(deleted as u64))
    }
}

/// Remove duplicate points that share the same `(canonical_uri, chunk_index)` key.
///
/// **Memory**: Two-pass approach — pass 1 counts occurrences using a compact
/// `(fnv64(uri), chunk_index)` key with no per-point String allocations; pass 2
/// scrolls again and allocates `DedupeRecord`s only for keys with count > 1.
/// At 2.5M points with ~1% duplicates this saves roughly 10× peak RSS compared
/// to a single-pass approach that stores records for every point.
///
/// **Performance**: O(n) full collection scroll — on large collections (millions
/// of points) this can take 60-120+ seconds. This is inherent to deduplication
/// and cannot be replaced with a facet query.
async fn dedupe_collection(cfg: &Config) -> Result<(usize, usize), Box<dyn Error>> {
    // Prevent concurrent deduplication runs — two simultaneous full-collection
    // scrolls race on deletes and produce misleading duplicate counts.
    if DEDUPE_IN_PROGRESS
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        return Err("deduplication already in progress for this process".into());
    }
    let _guard = DedupeGuard;

    let store = QdrantVectorStore::new(cfg.qdrant_url.clone(), "qdrant".to_string());
    let collection = cfg.collection.clone();

    // Selective payload: only fetch the fields needed for dedup (canonical URI,
    // chunk_index, scraped_at). Avoids transferring multi-KB chunk_text
    // per point — ~28x less data on a 7M-point collection.
    let with_payload = serde_json::json!({"include": [
        "item_canonical_uri",
        "source_canonical_uri",
        "source_item_key",
        "chunk_locator",
        "chunk_index",
        "scraped_at"
    ]});

    // Pass 1: count occurrences per compact key — no record storage.
    let mut counts: HashMap<(u64, i64), u32> = HashMap::new();
    store
        .scroll_pages(
            &collection,
            None,
            with_payload.clone(),
            SCROLL_PAGE_LIMIT,
            |points| {
                for point in points {
                    let Some(uri) = canonical_uri_from_payload(&point.payload) else {
                        continue;
                    };
                    let ci = point
                        .payload
                        .get("chunk_index")
                        .and_then(serde_json::Value::as_i64)
                        .unwrap_or(0);
                    *counts.entry((fnv64_uri(uri), ci)).or_insert(0) += 1;
                }
                true
            },
        )
        .await
        .map_err(|e| -> Box<dyn Error> { format!("dedupe pass 1 scroll failed: {e}").into() })?;

    // Identify keys with duplicates (count > 1). Keys with count == 1
    // (~99%+ of keys) are filtered out so pass 2 skips allocating records
    // for unique points — the primary memory saving of this approach.
    let dup_keys: HashSet<(u64, i64)> = counts
        .into_iter()
        .filter_map(|(k, n)| if n > 1 { Some(k) } else { None })
        .collect();

    if dup_keys.is_empty() {
        return Ok((0, 0));
    }

    // Pass 2: collect records only for duplicate keys.
    let mut by_key: HashMap<(u64, i64), Vec<DedupeRecord>> = HashMap::new();
    store
        .scroll_pages(
            &collection,
            None,
            with_payload,
            SCROLL_PAGE_LIMIT,
            |points| {
                for point in points {
                    let id = point.id.as_str().unwrap_or("").to_string();
                    if id.is_empty() {
                        continue;
                    }
                    let Some(uri) = canonical_uri_from_payload(&point.payload) else {
                        continue;
                    };
                    let ci = point
                        .payload
                        .get("chunk_index")
                        .and_then(serde_json::Value::as_i64)
                        .unwrap_or(0);
                    let key = (fnv64_uri(uri), ci);
                    if !dup_keys.contains(&key) {
                        continue;
                    }
                    let scraped_at = point
                        .payload
                        .get("scraped_at")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    by_key
                        .entry(key)
                        .or_default()
                        .push(DedupeRecord { id, scraped_at });
                }
                true
            },
        )
        .await
        .map_err(|e| -> Box<dyn Error> { format!("dedupe pass 2 scroll failed: {e}").into() })?;

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

    delete_point_ids(&store, &collection, &to_delete).await?;

    Ok((dup_groups, to_delete.len()))
}

/// Batch-delete points by id via the generic [`VectorStore::delete`] with a
/// `Points` selector — `points/delete` acknowledges the operation but does
/// not report a real deleted count (see `VectorDeleteSelector::Points`'s
/// handling in `axon-vectors`' `delete_inner`), so the caller's own
/// `to_delete.len()` is the source of truth for how many were removed, same
/// as legacy `qdrant_delete_points` returning `ids.len()`.
async fn delete_point_ids(
    store: &QdrantVectorStore,
    collection: &str,
    ids: &[String],
) -> Result<(), Box<dyn Error>> {
    if ids.is_empty() {
        return Ok(());
    }
    for batch in ids.chunks(DELETE_BATCH_SIZE) {
        let point_ids = batch
            .iter()
            .cloned()
            .map(VectorPointId::new)
            .collect::<Vec<_>>();
        store
            .delete(VectorDeleteSelector::Points {
                collection: collection.to_string(),
                point_ids,
            })
            .await
            .map_err(|e| -> Box<dyn Error> { format!("dedupe delete batch failed: {e}").into() })?;
    }
    Ok(())
}
