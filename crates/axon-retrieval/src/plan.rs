//! Retrieval planning DTOs for the boundary fake.

use axon_api::source::{SourceGenerationId, SourceId, Visibility};

use crate::query::RetrievalRequest;

pub const MODULE_NAME: &str = "plan";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetrievalPlan {
    pub collection: String,
    pub limit: u32,
    pub source_id: Option<SourceId>,
    pub generation: Option<SourceGenerationId>,
    pub allowed_visibility: Vec<Visibility>,
    pub namespace_filters: Vec<String>,
    pub excluded_source_kinds: Vec<String>,
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
            excluded_source_kinds: request.excluded_source_kinds.clone(),
            byte_budget: request.byte_budget,
            token_budget: request.token_budget,
        }
    }
}
