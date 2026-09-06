//! Contract-shaped `DocumentPreparer` / `ChunkRouter` traits.
//!
//! `crate::preparer::DocumentPreparer` and `crate::chunk_router::ChunkRouter`
//! are existing concrete structs whose bare names collide with the trait
//! names the contract wants — Rust forbids a struct and a trait with the
//! same identifier in one module, so both traits are defined here instead,
//! in a separate module, and implemented on the existing structs.
//!
//! Inside each `impl boundary::Trait for ConcreteStruct` block, calling
//! `self.method(...)` with the *original* argument shape resolves to the
//! pre-existing inherent method (inherent methods always shadow same-named
//! trait methods for direct dot-call resolution in Rust). That is the
//! load-bearing trick that lets every existing caller — including the
//! concurrent memory workflow's `DocumentPreparer::prepare(
//! PrepareSourceDocumentRequest)` calls — keep compiling untouched while the
//! trait-shaped methods become reachable through `&dyn Trait` / generic-bound
//! dispatch while requiring the same lineage-bearing request as production.
//!
//! Contract: `docs/pipeline-unification/foundation/types/trait-contract.md`.

use async_trait::async_trait;
use axon_api::source::{
    ApiError, ChunkProfile, ChunkProfileCapability, DocumentPreparerCapability, ErrorStage,
    HealthStatus, MetadataMap, PreparedDocument, SourceDocument,
};

use crate::PrepareSourceDocumentRequest;
use crate::profile::ChunkingProfile;

pub type Result<T> = std::result::Result<T, ApiError>;

/// Contract-shaped document preparer boundary.
///
/// Preparation requires a [`PrepareSourceDocumentRequest`] so source-generation
/// lineage cannot be omitted or synthesized by trait-object callers.
#[async_trait]
pub trait DocumentPreparer: Send + Sync {
    async fn prepare(&self, request: PrepareSourceDocumentRequest) -> Result<PreparedDocument>;
    async fn prepare_many(
        &self,
        requests: Vec<PrepareSourceDocumentRequest>,
    ) -> Result<Vec<PreparedDocument>>;
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
    async fn prepare(&self, request: PrepareSourceDocumentRequest) -> Result<PreparedDocument> {
        let result = self
            .prepare(request)
            .map_err(|err| ApiError::new("document.prepare.failed", ErrorStage::Preparing, err))?;
        Ok(result.document)
    }

    async fn prepare_many(
        &self,
        requests: Vec<PrepareSourceDocumentRequest>,
    ) -> Result<Vec<PreparedDocument>> {
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
