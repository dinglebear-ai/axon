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
pub use commands::{VectorSearchRequest, dispatch_vector_search_request};
pub use commands::{dedupe_payload, domains_payload, retrieve_result, sources_payload};
pub use dual_search::{DualSearchArm, DualSearchResult, qdrant_dual_search};
pub use filter::{build_local_project_code_filter, exclude_local_code_filter};
pub use hybrid::{qdrant_hybrid_search, qdrant_named_dense_search};
pub use types::DirectRetrieveResult;
#[cfg(any(test, feature = "test-util"))]
pub use types::RetrieveVariantError;
pub use types::{QdrantPayload, QdrantPoint, QdrantSearchHit};
pub use utils::{
    PAYLOAD_SCHEMA_VERSION, base_url, payload_text_typed, payload_url_typed, qdrant_base,
    query_snippet, rank_points_by_query_overlap, render_full_doc_filtered,
    render_full_doc_from_points, render_points_in_doc_order,
};

#[cfg(any(test, feature = "test-util"))]
pub use client::local_code_batch_delete_body_for_test;
pub use client::{
    QdrantDeleteByUrlResult, qdrant_batch_retrieve_by_urls, qdrant_delete_by_url,
    qdrant_delete_by_url_filter, qdrant_delete_local_code_files_for_generation,
    qdrant_delete_local_file_fragments, qdrant_delete_repo_file_fragments,
    qdrant_delete_stale_repo_file_urls, qdrant_delete_stale_tail, qdrant_domain_facets,
    qdrant_facet, qdrant_facet_filtered, qdrant_retrieve_by_url, qdrant_scroll_pages_selective,
};
pub use utils::{env_usize_clamped, payload_domain, payload_url};
