mod client;
mod commands;
mod hybrid;
#[cfg(test)]
mod tests;
mod types;
mod utils;

pub use client::{qdrant_delete_stale_domain_urls, qdrant_indexed_urls};
pub use commands::{dedupe_payload, domains_payload, retrieve_result, sources_payload};
pub use types::{QdrantPayload, QdrantPoint, QdrantSearchHit};
pub use utils::{
    base_url, payload_text_typed, payload_url_typed, qdrant_base, query_snippet,
    render_full_doc_from_points,
};

pub(crate) use client::{
    qdrant_delete_stale_tail, qdrant_domain_facets, qdrant_retrieve_by_url, qdrant_scroll_pages,
    qdrant_search,
};
pub(crate) use hybrid::{qdrant_hybrid_search, qdrant_named_dense_search};
pub(crate) use utils::{env_usize_clamped, payload_domain, payload_url};
