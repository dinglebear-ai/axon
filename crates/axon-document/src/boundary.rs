//! Async preparation and synchronous routing boundaries.
//!
//! The concrete preparer and this trait share the same lineage-bearing request
//! and prepared/skipped outcome. Async batch preparation preserves every outcome.
//! Contract: `docs/pipeline-unification/foundation/types/trait-contract.md`.

use async_trait::async_trait;
use axon_api::source::{
    ApiError, ChunkProfile, ChunkProfileCapability, DocumentPreparerCapability, ErrorStage,
    HealthStatus, MetadataMap, SourceDocument,
};

use crate::profile::ChunkingProfile;
use crate::{PrepareSourceDocumentRequest, PrepareSourceDocumentResult};

pub type Result<T> = std::result::Result<T, ApiError>;

/// Contract-shaped document preparer boundary.
///
/// Preparation requires a [`PrepareSourceDocumentRequest`] so source-generation
/// lineage cannot be omitted or synthesized by trait-object callers.
#[async_trait]
pub trait DocumentPreparer: Send + Sync {
    async fn prepare(
        &self,
        request: PrepareSourceDocumentRequest,
    ) -> Result<PrepareSourceDocumentResult>;
    async fn prepare_many(
        &self,
        requests: Vec<PrepareSourceDocumentRequest>,
    ) -> Result<Vec<PrepareSourceDocumentResult>>;
    async fn capabilities(&self) -> Result<DocumentPreparerCapability>;
}

/// Contract-shaped chunk routing boundary. Sync, per contract — no
/// `async_trait` needed.
pub trait ChunkRouter: Send + Sync {
    fn route(&self, document: &SourceDocument) -> Result<ChunkProfile>;
    fn supported_profiles(&self) -> Vec<ChunkProfileCapability>;
}

#[async_trait]
impl DocumentPreparer for crate::preparer::DocumentPreparer {
    async fn prepare(
        &self,
        request: PrepareSourceDocumentRequest,
    ) -> Result<PrepareSourceDocumentResult> {
        let result = self
            .prepare(request)
            .map_err(|err| ApiError::new("document.prepare.failed", ErrorStage::Preparing, err))?;
        Ok(result)
    }

    async fn prepare_many(
        &self,
        requests: Vec<PrepareSourceDocumentRequest>,
    ) -> Result<Vec<PrepareSourceDocumentResult>> {
        let mut prepared = Vec::with_capacity(requests.len());
        for request in requests {
            prepared.push(DocumentPreparer::prepare(self, request).await?);
        }
        Ok(prepared)
    }

    async fn capabilities(&self) -> Result<DocumentPreparerCapability> {
        Ok(DocumentPreparerCapability(
            axon_api::source::CapabilityBase {
                name: "axon-document::DocumentPreparer".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                owner_crate: "axon-document".to_string(),
                health: HealthStatus::Healthy,
                features: vec!["prepare".to_string(), "prepare_many".to_string()],
                limits: MetadataMap::new(),
            },
        ))
    }
}

impl ChunkRouter for crate::chunk_router::ChunkRouter {
    fn route(&self, document: &SourceDocument) -> Result<ChunkProfile> {
        // Inherent-shadow: resolves to `ChunkRouter::route(&self, &SourceDocument)
        // -> Result<ChunkingProfile, String>`, not this trait method.
        self.route(document)
            .map(ChunkProfile::from)
            .map_err(|err| ApiError::new("document.chunk_route.failed", ErrorStage::Preparing, err))
    }

    fn supported_profiles(&self) -> Vec<ChunkProfileCapability> {
        ALL_CHUNKING_PROFILES
            .iter()
            .map(|profile| {
                ChunkProfileCapability(axon_api::source::CapabilityBase {
                    name: profile.as_str().to_string(),
                    version: env!("CARGO_PKG_VERSION").to_string(),
                    owner_crate: "axon-document".to_string(),
                    health: HealthStatus::Healthy,
                    features: vec!["route".to_string()],
                    limits: MetadataMap::new(),
                })
            })
            .collect()
    }
}

const ALL_CHUNKING_PROFILES: [ChunkingProfile; 11] = [
    ChunkingProfile::CodeSymbol,
    ChunkingProfile::CodeManifest,
    ChunkingProfile::MarkdownSections,
    ChunkingProfile::HtmlArticle,
    ChunkingProfile::PlainTextWindows,
    ChunkingProfile::TranscriptSegments,
    ChunkingProfile::StructuredRecords,
    ChunkingProfile::ApiSchema,
    ChunkingProfile::ToolOutput,
    ChunkingProfile::SessionTurns,
    ChunkingProfile::AtomicMetadata,
];

#[cfg(test)]
#[path = "boundary_tests.rs"]
mod tests;
