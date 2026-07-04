//! Ledger store boundary.

mod fake;
mod util;

use async_trait::async_trait;
use axon_api::source::*;
pub use fake::FakeLedgerStore;

pub type Result<T> = std::result::Result<T, ApiError>;

#[async_trait]
pub trait LedgerStore: Send + Sync {
    async fn upsert_source(&self, source: SourceSummary) -> Result<()>;
    async fn get_source(&self, source_id: SourceId) -> Result<Option<SourceSummary>>;
    async fn put_manifest(&self, manifest: SourceManifest) -> Result<()>;
    /// Read the stored manifest for a specific `(source_id, generation)`.
    ///
    /// Returns `None` when no manifest was written for that generation. Used by
    /// the source orchestrator to build the baseline source graph from the
    /// real per-document manifest items after indexing.
    async fn get_manifest(
        &self,
        source_id: SourceId,
        generation: SourceGenerationId,
    ) -> Result<Option<SourceManifest>>;
    async fn diff_manifest(&self, manifest: SourceManifest) -> Result<SourceManifestDiff>;
    async fn create_generation(&self, source_id: SourceId) -> Result<SourceGeneration>;
    async fn committed_generation(&self, source_id: SourceId)
    -> Result<Option<SourceGenerationId>>;
    async fn complete_generation(&self, generation: SourceGeneration) -> Result<SourceGeneration>;
    async fn fail_generation(&self, generation: SourceGeneration) -> Result<SourceGeneration>;
    async fn publish_generation(
        &self,
        request: PublishGenerationRequest,
    ) -> Result<SourceGeneration>;
    async fn update_document_status(&self, status: DocumentStatus) -> Result<()>;
    async fn record_cleanup_debt(&self, debt: CleanupDebt) -> Result<()>;
    async fn acquire_lease(&self, request: LeaseRequest) -> Result<Option<LeaseGuard>>;
    async fn heartbeat_lease(
        &self,
        lease_id: LeaseId,
        owner_id: String,
        ttl_seconds: u64,
    ) -> Result<Option<LeaseGuard>>;
    async fn release_lease(&self, lease_id: LeaseId, owner_id: String) -> Result<()>;
    async fn reset(&self) -> Result<()>;
    async fn capabilities(&self) -> Result<LedgerStoreCapability>;
}
