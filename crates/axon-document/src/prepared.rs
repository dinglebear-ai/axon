//! Request/result types for document preparation.

use axon_api::source::{PreparedDocument, SourceDocument, SourceGenerationId};

use crate::profile::ChunkingProfile;

#[derive(Debug, Clone, PartialEq)]
pub struct PrepareSourceDocumentRequest {
    pub document: SourceDocument,
    pub generation: SourceGenerationId,
    pub profile: Option<ChunkingProfile>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PrepareSourceDocumentResult {
    pub document: PreparedDocument,
}
