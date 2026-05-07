//! In-process document-chunk cache.
//!
//! Wraps [`moka::future::Cache`] with single-flight (`try_get_with`) +
//! per-collection generation-counter invalidation. See `doc_cache.rs` and
//! `generation.rs` for details.
//!
//! Process-local: only useful in long-lived parents (`axon serve`,
//! `axon mcp`). CLI one-shots see zero hit rate by definition.

mod doc_cache;
mod generation;
#[cfg(test)]
mod tests;

pub use doc_cache::{
    CACHE_TTL_HARD_CAP_SECS, DocCache, DocCacheConfig, DocCacheKey, DocCacheStats, global_doc_cache,
};
pub use generation::{bump_generation, current_generation};
