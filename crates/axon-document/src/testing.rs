//! Test doubles for crates that need document preparation without runtime IO.

use std::sync::Mutex;

use async_trait::async_trait;
use axon_api::source::{
    ApiError, CapabilityBase, ChunkProfile, ChunkProfileCapability, DocumentPreparerCapability,
    ErrorStage, HealthStatus, MetadataMap, PreparedDocument, SourceDocument,
};

use crate::boundary;
use crate::prepared::{PrepareSourceDocumentRequest, PrepareSourceDocumentResult};
use crate::preparer::DocumentPreparer;

#[derive(Debug, Clone)]
pub struct RecordingPreparer {
    inner: DocumentPreparer,
    requests: Vec<PrepareSourceDocumentRequest>,
}

impl RecordingPreparer {
    pub fn new(inner: DocumentPreparer) -> Self {
        Self {
            inner,
            requests: Vec::new(),
        }
    }

    pub fn prepare(
        &mut self,
        request: PrepareSourceDocumentRequest,
    ) -> Result<PrepareSourceDocumentResult, String> {
        self.requests.push(request.clone());
        self.inner.prepare(request)
    }

    pub fn requests(&self) -> &[PrepareSourceDocumentRequest] {
        &self.requests
    }
}

/// Deterministic success/failure/degraded mode for the boundary fakes below.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FakeDocumentMode {
    #[default]
    Success,
    Degraded,
    Failure,
}

/// `boundary::DocumentPreparer` fake — deterministic, records calls.
#[derive(Debug, Default)]
pub struct FakeDocumentPreparer {
    mode: FakeDocumentMode,
    calls: Mutex<Vec<SourceDocument>>,
}

impl FakeDocumentPreparer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_mode(mode: FakeDocumentMode) -> Self {
        Self {
            mode,
            calls: Mutex::new(Vec::new()),
        }
    }

    pub fn calls(&self) -> Vec<SourceDocument> {
        self.calls
            .lock()
            .expect("FakeDocumentPreparer call log mutex poisoned")
            .clone()
    }
}

#[async_trait]
impl boundary::DocumentPreparer for FakeDocumentPreparer {
    async fn prepare(&self, document: SourceDocument) -> boundary::Result<PreparedDocument> {
        self.calls
            .lock()
            .expect("FakeDocumentPreparer call log mutex poisoned")
            .push(document.clone());
        if self.mode == FakeDocumentMode::Failure {
            return Err(ApiError::new(
                "document.prepare.fake_failure",
                ErrorStage::Preparing,
                "FakeDocumentPreparer configured to fail",
            ));
        }
        let mut prepared = PreparedDocument {
            document_id: document.document_id,
            source_id: document.source_id,
            source_item_key: document.source_item_key.clone(),
            generation: axon_api::source::SourceGenerationId::new("fake-generation"),
            canonical_uri: document.canonical_uri,
            prepare_version: "fake-document-preparer".to_string(),
            chunking_profile: crate::profile::ChunkingProfile::PlainTextWindows
                .as_str()
                .to_string(),
            chunking_method: crate::profile::ChunkingProfile::PlainTextWindows
                .as_str()
                .to_string(),
            chunks: Vec::new(),
            metadata: document.metadata,
            cleanup_keys: Vec::new(),
            graph_refs: Vec::new(),
            parse_facts: Vec::new(),
            graph_candidates: Vec::new(),
            warnings: Vec::new(),
            errors: Vec::new(),
        };
        if self.mode == FakeDocumentMode::Degraded {
            prepared.warnings.push(axon_api::source::SourceWarning {
                code: "document.prepare.fake_degraded".to_string(),
                severity: axon_api::source::Severity::Warning,
                message: "FakeDocumentPreparer configured to degrade".to_string(),
                source_item_key: Some(document.source_item_key),
                retryable: false,
            });
        }
        Ok(prepared)
    }

    async fn prepare_many(
        &self,
        documents: Vec<SourceDocument>,
    ) -> boundary::Result<Vec<PreparedDocument>> {
        let mut prepared = Vec::with_capacity(documents.len());
        for document in documents {
            prepared.push(boundary::DocumentPreparer::prepare(self, document).await?);
        }
        Ok(prepared)
    }

    async fn capabilities(&self) -> boundary::Result<DocumentPreparerCapability> {
        Ok(DocumentPreparerCapability(CapabilityBase {
            name: "fake::DocumentPreparer".to_string(),
            version: "fake".to_string(),
            owner_crate: "axon-document".to_string(),
            health: match self.mode {
                FakeDocumentMode::Success => HealthStatus::Healthy,
                FakeDocumentMode::Degraded => HealthStatus::Degraded,
                FakeDocumentMode::Failure => HealthStatus::Unavailable,
            },
            features: vec!["prepare".to_string(), "prepare_many".to_string()],
            limits: MetadataMap::new(),
        }))
    }
}

/// `boundary::ChunkRouter` fake — deterministic profile-per-call mapping,
/// records calls.
#[derive(Debug, Default)]
pub struct FakeChunkRouter {
    mode: FakeDocumentMode,
    profile: Option<ChunkProfile>,
    calls: Mutex<Vec<SourceDocument>>,
}

impl FakeChunkRouter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_mode(mode: FakeDocumentMode) -> Self {
        Self {
            mode,
            profile: None,
            calls: Mutex::new(Vec::new()),
        }
    }

    /// Force every `route` call to return this profile (deterministic mapping).
    pub fn with_fixed_profile(mut self, profile: ChunkProfile) -> Self {
        self.profile = Some(profile);
        self
    }

    pub fn calls(&self) -> Vec<SourceDocument> {
        self.calls
            .lock()
            .expect("FakeChunkRouter call log mutex poisoned")
            .clone()
    }
}

impl boundary::ChunkRouter for FakeChunkRouter {
    fn route(&self, document: &SourceDocument) -> boundary::Result<ChunkProfile> {
        self.calls
            .lock()
            .expect("FakeChunkRouter call log mutex poisoned")
            .push(document.clone());
        if self.mode == FakeDocumentMode::Failure {
            return Err(ApiError::new(
                "document.chunk_route.fake_failure",
                ErrorStage::Preparing,
                "FakeChunkRouter configured to fail",
            ));
        }
        Ok(self
            .profile
            .clone()
            .unwrap_or(ChunkProfile::PlainTextWindows))
    }

    fn supported_profiles(&self) -> Vec<ChunkProfileCapability> {
        vec![ChunkProfileCapability(CapabilityBase {
            name: "fake::ChunkRouter".to_string(),
            version: "fake".to_string(),
            owner_crate: "axon-document".to_string(),
            health: match self.mode {
                FakeDocumentMode::Success => HealthStatus::Healthy,
                FakeDocumentMode::Degraded => HealthStatus::Degraded,
                FakeDocumentMode::Failure => HealthStatus::Unavailable,
            },
            features: vec!["route".to_string()],
            limits: MetadataMap::new(),
        })]
    }
}

#[cfg(test)]
#[path = "testing_tests.rs"]
mod tests;
