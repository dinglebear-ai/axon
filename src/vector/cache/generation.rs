//! Per-collection generation counter for cache invalidation.
//!
//! A monotonically-increasing `AtomicU64` is maintained per collection name.
//! Reads on the cache hot path call [`current_generation`] (lock-free fast path
//! after the per-collection `Arc<AtomicU64>` exists). Write paths in
//! `tei/qdrant_store.rs` call [`bump_generation`] after a successful mutation.
//!
//! ## Invalidation strategy
//!
//! Cache keys embed the generation observed at read time. After a bump, the
//! next read sees a higher generation, lookups miss, and stale entries fall
//! out via LRU/TTL. There are no explicit cache-eviction calls on the write
//! path — the design is intentionally lock-free and write-cheap.
//!
//! ## Sibling pattern
//!
//! Mirrors `COLLECTION_MODES` in `tei/qdrant_store.rs`:
//! `LazyLock<RwLock<HashMap<String, Arc<AtomicU64>>>>`. Reads take the read
//! lock, clone the inner `Arc<AtomicU64>`, drop the lock, then load. The
//! per-collection counter itself is fully lock-free.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, RwLock};

static GENERATIONS: LazyLock<RwLock<HashMap<String, Arc<AtomicU64>>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Return-or-insert the per-collection counter `Arc`.
fn counter_for(collection: &str) -> Arc<AtomicU64> {
    if let Ok(map) = GENERATIONS.read()
        && let Some(arc) = map.get(collection)
    {
        return Arc::clone(arc);
    }
    // Slow path: take the write lock and insert if still missing.
    let mut map = match GENERATIONS.write() {
        Ok(m) => m,
        Err(poisoned) => poisoned.into_inner(),
    };
    Arc::clone(
        map.entry(collection.to_string())
            .or_insert_with(|| Arc::new(AtomicU64::new(0))),
    )
}

/// Read the current generation for `collection`. Lock-free after first call.
pub fn current_generation(collection: &str) -> u64 {
    counter_for(collection).load(Ordering::Acquire)
}

/// Increment the generation for `collection`. Call this after any successful
/// mutation that changes the chunks Qdrant returns for some URL: collection
/// create, schema patch, upsert. Returns the new generation value.
pub fn bump_generation(collection: &str) -> u64 {
    counter_for(collection).fetch_add(1, Ordering::AcqRel) + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_collection_starts_at_zero() {
        let unique = format!("test_gen_fresh_{}", uuid::Uuid::new_v4());
        assert_eq!(current_generation(&unique), 0);
    }

    #[test]
    fn bump_increments_monotonically() {
        let unique = format!("test_gen_bump_{}", uuid::Uuid::new_v4());
        assert_eq!(current_generation(&unique), 0);
        let g1 = bump_generation(&unique);
        let g2 = bump_generation(&unique);
        assert_eq!(g1, 1);
        assert_eq!(g2, 2);
        assert_eq!(current_generation(&unique), 2);
    }

    #[test]
    fn collections_are_independent() {
        let a = format!("test_gen_iso_a_{}", uuid::Uuid::new_v4());
        let b = format!("test_gen_iso_b_{}", uuid::Uuid::new_v4());
        bump_generation(&a);
        bump_generation(&a);
        assert_eq!(current_generation(&a), 2);
        assert_eq!(current_generation(&b), 0);
    }
}
