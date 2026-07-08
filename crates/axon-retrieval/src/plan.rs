//! Retrieval planning DTOs for the boundary fake.

use axon_api::source::{SourceGenerationId, SourceId, Visibility};

use crate::query::RetrievalRequest;

pub const MODULE_NAME: &str = "plan";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RetrievalPlan {
    pub(crate) collection: String,
    pub(crate) limit: u32,
    pub(crate) source_id: Option<SourceId>,
    pub(crate) generation: Option<SourceGenerationId>,
    pub(crate) allowed_visibility: Vec<Visibility>,
    pub(crate) namespace_filters: Vec<String>,
    pub(crate) excluded_namespaces: Vec<String>,
    pub(crate) byte_budget: u64,
    pub(crate) token_budget: u32,
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
            excluded_namespaces: request.excluded_namespaces.clone(),
            byte_budget: request.byte_budget,
            token_budget: request.token_budget,
        }
    }
}
