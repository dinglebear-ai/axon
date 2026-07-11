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
    /// Bulk-list registered sources (id, canonical URI, kind/adapter, status,
    /// counts, …), filtered and paginated per `request`. The `list_sources`
    /// entry in `docs/pipeline-unification/runtime/ledger-contract.md`'s
    /// Public Boundary — the only enumeration mechanism callers should use
    /// once a source is ledger-registered (see `axon-services::refresh`).
    async fn list_sources(&self, request: SourceListRequest) -> Result<Page<SourceSummary>>;
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
    /// List every not-yet-resolved cleanup-debt entry for a source, oldest
    /// first. Used by `axon-prune` to drain superseded-generation debt after a
    /// new generation is committed. A debt is "pending" while its `completed_at`
    /// timestamp is unset (status alone is advisory).
    async fn list_pending_cleanup_debt(&self, source_id: SourceId) -> Result<Vec<CleanupDebt>>;
    /// Mark one cleanup-debt entry resolved: set its status to `Completed` and
    /// stamp `completed_at`. Idempotent — resolving an already-resolved or
    /// unknown debt id is a no-op.
    async fn resolve_cleanup_debt(&self, debt_id: CleanupDebtId) -> Result<()>;
    /// Delete ledger rows (generation, manifest, items, document status) for
    /// one superseded generation of `source_id`. This is the `LedgerPrune`
    /// cleanup-debt boundary from `docs/pipeline-unification/runtime/
    /// ledger-contract.md` — it never touches the committed/current
    /// generation (callers must fence that, same as vector deletes).
    /// Idempotent: deleting an already-deleted or unknown generation is a
    /// no-op returning `0`. Returns the number of ledger rows removed.
    async fn delete_generation(
        &self,
        source_id: SourceId,
        generation: SourceGenerationId,
    ) -> Result<u64>;
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
