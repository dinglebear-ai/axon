mod client;
mod commands;
mod dual_search;
mod filter;
mod hybrid;
mod search;
#[cfg(test)]
#[path = "qdrant_tests.rs"]
mod tests;
mod types;
mod utils;

pub use client::{
    qdrant_delete_stale_domain_urls, qdrant_domain_has_indexed_url, qdrant_indexed_urls,
    qdrant_urls_for_domain, qdrant_urls_for_domain_limited, qdrant_urls_for_domain_page,
};
pub(crate) use commands::{VectorSearchRequest, dispatch_vector_search_request};
pub use commands::{dedupe_payload, domains_payload, retrieve_result, sources_payload};
pub(crate) use dual_search::{DualSearchArm, DualSearchResult, qdrant_dual_search};
pub(crate) use types::DirectRetrieveResult;
#[cfg(test)]
pub(crate) use types::RetrieveVariantError;
pub use types::{QdrantPayload, QdrantPoint, QdrantSearchHit};
pub use utils::{
    PAYLOAD_SCHEMA_VERSION, base_url, payload_text_typed, payload_url_typed, qdrant_base,
    query_snippet, render_full_doc_filtered, render_full_doc_from_points,
};

pub(crate) use client::{
    qdrant_batch_retrieve_by_urls, qdrant_delete_stale_tail, qdrant_domain_facets,
    qdrant_retrieve_by_url, qdrant_scroll_pages_selective,
};
pub(crate) use utils::{env_usize_clamped, payload_domain, payload_url};
