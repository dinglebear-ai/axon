mod client;
mod commands;
mod filter;
mod hybrid;
mod search;
#[cfg(test)]
mod tests;
mod types;
mod utils;

pub use client::{qdrant_delete_stale_domain_urls, qdrant_indexed_urls, qdrant_urls_for_domain};
pub(crate) use commands::dispatch_vector_search;
pub use commands::{dedupe_payload, domains_payload, retrieve_result, sources_payload};
pub use types::{QdrantPayload, QdrantPoint, QdrantSearchHit};
pub use utils::{
    base_url, payload_text_typed, payload_url_typed, qdrant_base, query_snippet,
    render_full_doc_from_points,
};

pub(crate) use client::{
    qdrant_delete_stale_tail, qdrant_domain_facets, qdrant_retrieve_by_url,
    qdrant_scroll_pages_selective,
};
pub(crate) use utils::{env_usize_clamped, payload_domain, payload_url};
