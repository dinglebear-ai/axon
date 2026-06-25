//! In-process cache for full-document chunks fetched from Qdrant.
//!
//! Wraps [`moka::future::Cache`] with:
//!
//! - **Single-flight** via `try_get_with`: concurrent misses on the same key
//!   share one fetcher future. Without this, N concurrent asks targeting the
//!   same URL would each dispatch a separate Qdrant scroll.
//! - **Generation-counter invalidation**: keys embed the per-collection
//!   generation captured at read time (see `generation.rs`). Any write that
//!   bumps the generation makes stale entries unreachable; they fall out via
//!   LRU or TTL.
//! - **Hard 300s TTL**: a security primitive, not just a freshness signal.
//!   Bounds how long deleted content can be served if a write site forgets to
//!   bump (e.g. `axon dedupe`/`migrate` are out of this bead's file ownership).
//! - **Byte-weighted capacity**: `weigher` returns the summed `chunk_text`
//!   length per entry; `max_capacity` is in bytes (default 256 MiB).
//!
//! ## Process-local — only useful in long-lived parents
//!
//! In short-lived CLI one-shots, hit rate is zero by definition. Enable only
//! in `axon serve` / `axon mcp`. The
//! enable-gate lives in `cfg.ask_cache_enabled`.
//!
//! ## Security
//!
//! `chunk_text` lives in the process heap as a sensitive-data exposure
//! surface. If this cache ever runs inside a daemon process, set
//! `RLIMIT_CORE=0` at startup to prevent coredumps from leaking text.
//! Logs in this module deliberately exclude `chunk_text` — only the key
//! components (collection, url, generation) and counters.

use crate::ops::qdrant::QdrantPoint;
use anyhow::Result;
use moka::future::Cache;
use std::collections::HashMap;
use std::future::Future;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

/// Hard upper bound on how long any cached entry can be served, regardless of
/// LRU pressure. Security primitive: bounds staleness of deleted content.
pub const CACHE_TTL_HARD_CAP_SECS: u64 = 300;

/// Cache key. Generation is embedded so a bump produces a different key —
/// stale (collection, url, old_gen) entries fall out via LRU / TTL.
///
/// Note: deliberately excludes `chunk_count` / `doc_chunk_limit`. These are
/// process-constant in practice and including them would block reuse for the
/// common case (a request for `chunk_count=50` not hitting a cached
/// `chunk_count=100`). Callers that ask for fewer chunks can truncate the
/// returned vector.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct DocCacheKey {
    pub collection: String,
    pub url: String,
    pub generation: u64,
}

/// Cached per-key counters. Cheap to read; never logged with chunk_text.
#[derive(Default, Debug)]
pub struct DocCacheStats {
    pub hits: AtomicU64,
    pub misses: AtomicU64,
    pub evicted: AtomicU64,
}

impl DocCacheStats {
    #[must_use]
    pub fn hits(&self) -> u64 {
        self.hits.load(Ordering::Relaxed)
    }
    #[must_use]
    pub fn misses(&self) -> u64 {
        self.misses.load(Ordering::Relaxed)
    }
    #[must_use]
    pub fn evicted(&self) -> u64 {
        self.evicted.load(Ordering::Relaxed)
    }
}

/// Compute the byte weight of a cached value. Sums `chunk_text` lengths;
/// avoids the entry-count blow-up at high traffic with mixed doc sizes.
fn weigh_points(points: &[QdrantPoint]) -> u32 {
    let total: usize = points.iter().map(|p| p.payload.chunk_text.len()).sum();
    u32::try_from(total).unwrap_or(u32::MAX)
}

/// Configuration for [`DocCache::new`].
#[derive(Clone, Debug, Eq)]
pub struct DocCacheConfig {
    pub max_capacity_bytes: u64,
    pub ttl_secs: u64,
}

impl PartialEq for DocCacheConfig {
    fn eq(&self, other: &Self) -> bool {
        self.max_capacity_bytes == other.max_capacity_bytes
            && self.effective_ttl_secs() == other.effective_ttl_secs()
    }
}

impl Hash for DocCacheConfig {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.max_capacity_bytes.hash(state);
        self.effective_ttl_secs().hash(state);
    }
}

impl Default for DocCacheConfig {
    fn default() -> Self {
        Self {
            max_capacity_bytes: 256 * 1024 * 1024,
            ttl_secs: CACHE_TTL_HARD_CAP_SECS,
        }
    }
}

impl DocCacheConfig {
    /// Build the document cache config from the parsed runtime config.
    #[must_use]
    pub fn from_ask_config(cfg: &axon_core::config::Config) -> Self {
        Self {
            max_capacity_bytes: cfg.ask_cache_max_capacity_bytes,
            ttl_secs: cfg.ask_cache_ttl_secs,
        }
    }

    /// TTL after applying the hard security cap.
    #[must_use]
    pub fn effective_ttl_secs(&self) -> u64 {
        self.ttl_secs.min(CACHE_TTL_HARD_CAP_SECS)
    }
}

/// In-process moka cache for full-document chunk fetches.
pub struct DocCache {
    inner: Cache<DocCacheKey, Arc<Vec<QdrantPoint>>>,
    stats: Arc<DocCacheStats>,
    config: DocCacheConfig,
}

impl DocCache {
    /// Build a new cache. Respects [`CACHE_TTL_HARD_CAP_SECS`] regardless of
    /// the configured `ttl_secs` value (security backstop).
    pub fn new(config: DocCacheConfig) -> Self {
        let stats = Arc::new(DocCacheStats::default());
        let stats_for_eviction = Arc::clone(&stats);
        let ttl = Duration::from_secs(config.effective_ttl_secs());
        let inner = Cache::builder()
            .max_capacity(config.max_capacity_bytes)
            .weigher(|_k: &DocCacheKey, v: &Arc<Vec<QdrantPoint>>| weigh_points(v))
            .time_to_live(ttl)
            .support_invalidation_closures()
            .eviction_listener(move |_k, _v, _cause| {
                stats_for_eviction.evicted.fetch_add(1, Ordering::Relaxed);
            })
            .build();
        Self {
            inner,
            stats,
            config,
        }
    }

    /// Returns the effective cache configuration.
    #[must_use]
    pub fn config(&self) -> &DocCacheConfig {
        &self.config
    }

    /// Returns the shared stats handle for diagnostics.
    #[must_use]
    pub fn stats(&self) -> Arc<DocCacheStats> {
        Arc::clone(&self.stats)
    }

    /// Single-flight fetch. On hit returns the cached `Arc`. On miss, the
    /// first caller runs `fetch`; concurrent waiters block on the same future.
    pub async fn get_or_fetch<F, Fut>(
        &self,
        key: DocCacheKey,
        fetch: F,
    ) -> Result<Arc<Vec<QdrantPoint>>>
    where
        F: FnOnce() -> Fut + Send,
        Fut: Future<Output = Result<Vec<QdrantPoint>>> + Send,
    {
        let result = self
            .inner
            .entry(key)
            .or_try_insert_with(async move { fetch().await.map(Arc::new) })
            .await;
        match result {
            Ok(entry) => {
                if entry.is_fresh() {
                    self.stats.misses.fetch_add(1, Ordering::Relaxed);
                } else {
                    self.stats.hits.fetch_add(1, Ordering::Relaxed);
                }
                Ok(entry.into_value())
            }
            Err(arc_err) => {
                self.stats.misses.fetch_add(1, Ordering::Relaxed);
                // moka returns Arc<E> on the failure path; stringify into
                // a fresh anyhow error so the boundary stays anyhow::Error.
                Err(anyhow::anyhow!("doc cache fetch failed: {arc_err}"))
            }
        }
    }

    /// Drop every entry whose collection matches. Useful for explicit
    /// invalidation hooks (`axon dedupe`, `axon migrate`) — though the
    /// generation bump is the primary mechanism.
    pub fn invalidate_collection(&self, collection: &str) {
        let needle = collection.to_string();
        let _ = self
            .inner
            .invalidate_entries_if(move |k, _v| k.collection == needle);
    }

    /// Wait for any in-flight invalidation work to finalize. Test-only.
    #[cfg(test)]
    pub async fn run_pending_tasks(&self) {
        self.inner.run_pending_tasks().await;
    }
}

/// Process-global cache registry keyed by effective cache config. This keeps
/// the common daemon hit path process-local while making `[ask.cache]`
/// capacity/TTL changes take effect without a hardcoded singleton.
static CACHE_REGISTRY: LazyLock<Mutex<HashMap<DocCacheConfig, Arc<DocCache>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Returns the process-global cache instance matching the supplied config.
#[must_use]
pub fn doc_cache_for_config(config: DocCacheConfig) -> Arc<DocCache> {
    let mut registry = CACHE_REGISTRY
        .lock()
        .expect("doc cache registry mutex poisoned");
    Arc::clone(
        registry
            .entry(config.clone())
            .or_insert_with(|| Arc::new(DocCache::new(config))),
    )
}
