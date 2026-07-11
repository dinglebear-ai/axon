//! Query DTOs for the retrieval boundary fake.

use axon_api::source::{ChunkId, DocumentId, SourceGenerationId, SourceId, Visibility};

use crate::citation::Citation;
use crate::context::ContextBundle;
use crate::plan::RetrievalPlan;

pub const MODULE_NAME: &str = "query";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetrievalRequest {
    pub query: String,
    pub collection: String,
    pub limit: u32,
    pub source_id: Option<SourceId>,
    pub generation: Option<SourceGenerationId>,
    pub namespace_filters: Vec<String>,
    /// Namespaces to drop from results when `namespace_filters` is empty
    /// (unrestricted search). A caller that sets an explicit positive
    /// `namespace_filters` already governs which namespaces can appear, so
    /// this default exclusion only matters for the common unrestricted case
    /// — e.g. plain `query`/`ask` excluding `memory` by default so memory
    /// records don't leak into normal retrieval without intent.
    pub excluded_namespaces: Vec<String>,
    pub byte_budget: u64,
    pub token_budget: u32,
}

impl RetrievalRequest {
    pub(crate) fn plan(&self, allowed_visibility: Vec<Visibility>) -> RetrievalPlan {
        RetrievalPlan::from_request(self, allowed_visibility)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RetrievalMatch {
    pub chunk_id: ChunkId,
    pub document_id: DocumentId,
    pub source_id: SourceId,
    pub score: f64,
    pub canonical_uri: String,
    pub text: String,
    pub citation: Citation,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RetrievalResult {
    pub plan: RetrievalPlan,
    pub matches: Vec<RetrievalMatch>,
    pub context: ContextBundle,
    pub citations: Vec<Citation>,
}

/// Retrieval-domain query request for [`crate::boundary::RetrievalEngine::query`].
///
/// Distinct from `axon_api::mcp_schema::QueryRequest`, which is MCP/CLI
/// transport-shaped (carries file_path/symbol/line-number CLI-output fields)
/// and is barred from trait signatures by the pipeline-unification trait
/// contract's "traits do not accept CLI/MCP/REST transport structs" rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryRequest {
    pub query: String,
    pub collection: String,
    pub limit: u32,
    pub namespace_filters: Vec<String>,
}

/// Retrieval-domain query result for [`crate::boundary::RetrievalEngine::query`].
///
/// Distinct from `axon_api::mcp_schema::QueryResult`/`QueryHit` for the same
/// reason as [`QueryRequest`] — those are transport-shaped result rows, not a
/// retrieval-domain match set.
#[derive(Debug, Clone, PartialEq)]
pub struct QueryResult {
    pub matches: Vec<RetrievalMatch>,
    pub citations: Vec<Citation>,
}
