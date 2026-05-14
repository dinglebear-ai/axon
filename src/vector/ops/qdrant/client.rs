//! Low-level Qdrant HTTP client operations.
//!
//! ## Key invariants
//! - Use [`qdrant_url_facets`] (O(1) `/facet` POST) for URL counting and aggregation.
//!   Never use full scroll for aggregation — it loads the entire collection into memory.
//! - [`ensure_collection`](super::tei::ensure_collection) issues GET-first, PUT only on 404.
//!   Safe to call on every embed.
//! - All delete operations use [`qdrant_delete_with_retry`] with exponential backoff.

pub mod delete;
pub mod facets;
pub mod retrieve;
pub mod scroll;

// Re-exports for convenience (public API)
pub use delete::{qdrant_delete_points, qdrant_delete_stale_domain_urls, qdrant_delete_stale_tail};
pub use facets::{qdrant_domain_facets, qdrant_url_facets};
pub use retrieve::{qdrant_retrieve_by_url, qdrant_retrieve_by_url_details};
pub use scroll::{qdrant_indexed_urls, qdrant_scroll_pages_selective, qdrant_urls_for_domain};

#[cfg(test)]
pub(crate) use delete::qdrant_delete_by_url_filter;
#[cfg(test)]
pub(crate) use scroll::qdrant_scroll_pages;
