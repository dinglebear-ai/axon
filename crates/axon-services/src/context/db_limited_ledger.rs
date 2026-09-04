use std::sync::Arc;

use async_trait::async_trait;
use axon_api::source::*;
use axon_ledger::store::{LedgerStore, Result};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// Source-pipeline ledger admission gate. The unified SQLite pool is shared
/// with job heartbeats, provider scheduling, and other control-plane work, so
/// data-plane ledger stages must not consume every connection under concurrent
/// crawls. One permit represents one potentially connection-owning ledger
/// operation.
pub(super) struct DbLimitedLedgerStore {
    inner: Arc<dyn LedgerStore>,
    slots: Arc<Semaphore>,
}

impl DbLimitedLedgerStore {
    pub(super) fn new(inner: Arc<dyn LedgerStore>, slots: Arc<Semaphore>) -> Self {
        Self { inner, slots }
    }

    async fn permit(&self) -> Result<OwnedSemaphorePermit> {
        Arc::clone(&self.slots).acquire_owned().await.map_err(|_| {
            ApiError::new(
                "source.db_stage.admission_closed",
                ErrorStage::Storage,
                "source database-stage admission gate is closed",
            )
        })
    }
}

#[async_trait]
impl LedgerStore for DbLimitedLedgerStore {
    async fn upsert_source(&self, source: SourceSummary) -> Result<()> {
        let _permit = self.permit().await?;
        self.inner.upsert_source(source).await
    }

    async fn get_source(&self, source_id: SourceId) -> Result<Option<SourceSummary>> {
        let _permit = self.permit().await?;
        self.inner.get_source(source_id).await
    }

    async fn list_sources(&self, request: SourceListRequest) -> Result<Page<SourceSummary>> {
        let _permit = self.permit().await?;
        self.inner.list_sources(request).await
    }

    async fn put_manifest(&self, manifest: SourceManifest) -> Result<()> {
        let _permit = self.permit().await?;
        self.inner.put_manifest(manifest).await
    }

    async fn put_manifest_ref(&self, manifest: &SourceManifest) -> Result<()> {
        let _permit = self.permit().await?;
        self.inner.put_manifest_ref(manifest).await
    }

    async fn get_manifest(
        &self,
        source_id: SourceId,
        generation: SourceGenerationId,
    ) -> Result<Option<SourceManifest>> {
        let _permit = self.permit().await?;
        self.inner.get_manifest(source_id, generation).await
    }

    async fn get_manifest_metadata(
        &self,
        source_id: SourceId,
        generation: SourceGenerationId,
    ) -> Result<Option<MetadataMap>> {
        let _permit = self.permit().await?;
        self.inner
            .get_manifest_metadata(source_id, generation)
            .await
    }

    async fn get_manifest_items(
        &self,
        source_id: SourceId,
        generation: SourceGenerationId,
        item_keys: Vec<SourceItemKey>,
    ) -> Result<Vec<ManifestItem>> {
        let _permit = self.permit().await?;
        self.inner
            .get_manifest_items(source_id, generation, item_keys)
            .await
    }

    async fn get_manifest_items_with_metadata_key(
        &self,
        source_id: SourceId,
        generation: SourceGenerationId,
        item_keys: Vec<SourceItemKey>,
        metadata_key: String,
    ) -> Result<Vec<ManifestItem>> {
        let _permit = self.permit().await?;
        self.inner
            .get_manifest_items_with_metadata_key(source_id, generation, item_keys, metadata_key)
            .await
    }

    async fn diff_manifest(&self, manifest: SourceManifest) -> Result<SourceManifestDiff> {
        let _permit = self.permit().await?;
        self.inner.diff_manifest(manifest).await
    }

    async fn diff_manifest_ref(&self, manifest: &SourceManifest) -> Result<SourceManifestDiff> {
        let _permit = self.permit().await?;
        self.inner.diff_manifest_ref(manifest).await
    }

    async fn create_generation(&self, source_id: SourceId) -> Result<SourceGeneration> {
        let _permit = self.permit().await?;
        self.inner.create_generation(source_id).await
    }

    async fn committed_generation(
        &self,
        source_id: SourceId,
    ) -> Result<Option<SourceGenerationId>> {
        let _permit = self.permit().await?;
        self.inner.committed_generation(source_id).await
    }

    async fn complete_generation(&self, generation: SourceGeneration) -> Result<SourceGeneration> {
        let _permit = self.permit().await?;
        self.inner.complete_generation(generation).await
    }

    async fn fail_generation(&self, generation: SourceGeneration) -> Result<SourceGeneration> {
        let _permit = self.permit().await?;
        self.inner.fail_generation(generation).await
    }

    async fn publish_generation(
        &self,
        request: PublishGenerationRequest,
    ) -> Result<SourceGeneration> {
        let _permit = self.permit().await?;
        self.inner.publish_generation(request).await
    }

    async fn update_document_status(&self, status: DocumentStatus) -> Result<()> {
        let _permit = self.permit().await?;
        self.inner.update_document_status(status).await
    }

    async fn update_document_statuses(&self, statuses: Vec<DocumentStatus>) -> Result<()> {
        let _permit = self.permit().await?;
        self.inner.update_document_statuses(statuses).await
    }

    async fn publish_document_statuses(
        &self,
        source_id: SourceId,
        generation: SourceGenerationId,
        updated_at: Timestamp,
    ) -> Result<u64> {
        let _permit = self.permit().await?;
        self.inner
            .publish_document_statuses(source_id, generation, updated_at)
            .await
    }

    async fn record_cleanup_debt(&self, debt: CleanupDebt) -> Result<()> {
        let _permit = self.permit().await?;
        self.inner.record_cleanup_debt(debt).await
    }

    async fn list_pending_cleanup_debt(&self, source_id: SourceId) -> Result<Vec<CleanupDebt>> {
        let _permit = self.permit().await?;
        self.inner.list_pending_cleanup_debt(source_id).await
    }

    async fn list_adapter_release_debt(&self, limit: usize) -> Result<Vec<CleanupDebt>> {
        let _permit = self.permit().await?;
        self.inner.list_adapter_release_debt(limit).await
    }

    async fn list_pending_cleanup_debt_after(
        &self,
        after: Option<CleanupDebtId>,
        limit: usize,
    ) -> Result<Vec<CleanupDebt>> {
        let _permit = self.permit().await?;
        self.inner
            .list_pending_cleanup_debt_after(after, limit)
            .await
    }

    async fn resolve_cleanup_debt(&self, debt_id: CleanupDebtId) -> Result<()> {
        let _permit = self.permit().await?;
        self.inner.resolve_cleanup_debt(debt_id).await
    }

    async fn delete_generation(
        &self,
        source_id: SourceId,
        generation: SourceGenerationId,
    ) -> Result<u64> {
        let _permit = self.permit().await?;
        self.inner.delete_generation(source_id, generation).await
    }

    async fn acquire_lease(&self, request: LeaseRequest) -> Result<Option<LeaseGuard>> {
        let _permit = self.permit().await?;
        self.inner.acquire_lease(request).await
    }

    async fn heartbeat_lease(
        &self,
        lease_id: LeaseId,
        owner_id: String,
        ttl_seconds: u64,
    ) -> Result<Option<LeaseGuard>> {
        let _permit = self.permit().await?;
        self.inner
            .heartbeat_lease(lease_id, owner_id, ttl_seconds)
            .await
    }

    async fn release_lease(&self, lease_id: LeaseId, owner_id: String) -> Result<()> {
        let _permit = self.permit().await?;
        self.inner.release_lease(lease_id, owner_id).await
    }

    async fn reset(&self) -> Result<()> {
        let _permit = self.permit().await?;
        self.inner.reset().await
    }

    async fn capabilities(&self) -> Result<LedgerStoreCapability> {
        let _permit = self.permit().await?;
        self.inner.capabilities().await
    }
}
