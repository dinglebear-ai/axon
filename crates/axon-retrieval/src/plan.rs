//! Retrieval planning DTOs for the boundary fake.

use axon_api::source::{SourceGenerationId, SourceId, Visibility};

use crate::query::RetrievalRequest;

pub const MODULE_NAME: &str = "plan";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetrievalPlan {
    pub source_kinds: Vec<axon_api::source::SourceKind>,
    pub source_urls: Vec<String>,
    pub collection: String,
    pub limit: u32,
    pub source_id: Option<SourceId>,
    pub generation: Option<SourceGenerationId>,
    pub allowed_visibility: Vec<Visibility>,
    pub namespace_filters: Vec<String>,
    pub excluded_source_kinds: Vec<String>,
    pub hybrid: bool,
    pub hybrid_candidates: Option<u32>,
    pub since: Option<String>,
    pub before: Option<String>,
    pub byte_budget: u64,
    pub token_budget: u32,
}

impl RetrievalPlan {
    pub(crate) fn from_request(
        request: &RetrievalRequest,
        allowed_visibility: Vec<Visibility>,
    ) -> Self {
        Self {
            collection: request.collection.clone(),
            limit: request.limit,
            source_id: request.source_id.clone(),
            generation: request.generation.clone(),
            allowed_visibility,
            namespace_filters: request.namespace_filters.clone(),
            source_kinds: request.source_kinds.clone(),
            source_urls: request.source_urls.clone(),
            excluded_source_kinds: request.excluded_source_kinds.clone(),
            hybrid: request.hybrid,
            hybrid_candidates: request.hybrid_candidates,
            since: request.since.clone(),
            before: request.before.clone(),
            byte_budget: request.byte_budget,
            token_budget: request.token_budget,
        }
    }
}
