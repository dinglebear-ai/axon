//! Request/result types for document preparation.

use axon_api::source::{
    GraphCandidate, PreparedDocument, SourceDocument, SourceError, SourceGenerationId,
    SourceParseFacts, SourceWarning,
};

use crate::profile::ChunkingProfile;

#[derive(Debug, Clone, PartialEq)]
pub struct PrepareSourceDocumentRequest {
    pub document: SourceDocument,
    pub generation: SourceGenerationId,
    pub profile: Option<ChunkingProfile>,
    pub parse_facts: Vec<SourceParseFacts>,
    pub graph_candidates: Vec<GraphCandidate>,
    pub warnings: Vec<SourceWarning>,
    pub errors: Vec<SourceError>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PrepareSourceDocumentResult {
    pub document: PreparedDocument,
}
