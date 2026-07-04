//! Query DTOs for the retrieval boundary fake.

use axon_api::source::{ChunkId, DocumentId, SourceGenerationId, SourceId, Visibility};

use crate::citation::Citation;
use crate::context::ContextBundle;
use crate::plan::RetrievalPlan;

pub const MODULE_NAME: &str = "query";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RetrievalRequest {
    pub(crate) query: String,
    pub(crate) collection: String,
    pub(crate) limit: u32,
    pub(crate) source_id: Option<SourceId>,
    pub(crate) generation: Option<SourceGenerationId>,
    pub(crate) namespace_filters: Vec<String>,
    pub(crate) byte_budget: u64,
    pub(crate) token_budget: u32,
}

impl RetrievalRequest {
    pub(crate) fn plan(&self, allowed_visibility: Vec<Visibility>) -> RetrievalPlan {
        RetrievalPlan::from_request(self, allowed_visibility)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RetrievalMatch {
    pub(crate) chunk_id: ChunkId,
    pub(crate) document_id: DocumentId,
    pub(crate) source_id: SourceId,
    pub(crate) score: f64,
    pub(crate) canonical_uri: String,
    pub(crate) text: String,
    pub(crate) citation: Citation,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RetrievalResult {
    pub(crate) plan: RetrievalPlan,
    pub(crate) matches: Vec<RetrievalMatch>,
    pub(crate) context: ContextBundle,
    pub(crate) citations: Vec<Citation>,
}
