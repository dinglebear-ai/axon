//! SQLite-backed ledger store.

mod cleanup;
mod document;
mod generation;
mod lease;
mod manifest;
mod source;
mod util;

use std::str::FromStr;

use async_trait::async_trait;
use axon_api::source::*;
use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

use crate::migration::{clear_ledger, migrate_ledger, sqlite_error};
use crate::store::{LedgerStore, Result};

#[derive(Debug, Clone)]
pub struct SqliteLedgerStore {
    pub(crate) pool: SqlitePool,
}

impl SqliteLedgerStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Bind the ledger to an already-open, already-migrated SQLite pool — the
    /// shared runtime pool that also backs `JobStore`. Per the storage contract
    /// (docs/pipeline-unification), the runtime uses ONE database so
    /// `jobs.source_id` can FK to `sources(source_id)`; the ledger's tables are
    /// created by that shared pool's migration runner (see
    /// `axon-jobs/src/migrations/0017_source_ledger.sql`), so this constructor
    /// does NOT run migrations. Use [`SqliteLedgerStore::connect`] only for a
    /// standalone, ledger-only database (tests, tooling).
    pub fn from_pool(pool: SqlitePool) -> Self {
        Self::new(pool)
    }

    pub async fn connect(path: &str) -> Result<Self> {
        let options = SqliteConnectOptions::from_str(path)
            .map_err(sqlite_error)?
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(sqlite_max_connections(path))
            .connect_with(options)
            .await
            .map_err(sqlite_error)?;
        migrate_ledger(&pool).await?;
        Ok(Self::new(pool))
    }

    pub async fn in_memory() -> Result<Self> {
        Self::connect("sqlite::memory:").await
    }
}

fn sqlite_max_connections(path: &str) -> u32 {
    if path == "sqlite::memory:" || path.contains("mode=memory") {
        1
    } else {
        5
    }
}

#[async_trait]
impl LedgerStore for SqliteLedgerStore {
    async fn upsert_source(&self, source: SourceSummary) -> Result<()> {
        source::upsert_source(self, source).await
    }

    async fn get_source(&self, source_id: SourceId) -> Result<Option<SourceSummary>> {
        source::get_source(self, source_id).await
    }

    async fn put_manifest(&self, manifest: SourceManifest) -> Result<()> {
        manifest::put_manifest(self, manifest).await
    }

    async fn diff_manifest(&self, manifest: SourceManifest) -> Result<SourceManifestDiff> {
        manifest::diff_manifest(self, manifest).await
    }

    async fn create_generation(&self, source_id: SourceId) -> Result<SourceGeneration> {
        generation::create_generation(self, source_id).await
    }

    async fn committed_generation(
        &self,
        source_id: SourceId,
    ) -> Result<Option<SourceGenerationId>> {
        generation::committed_generation(self, &source_id).await
    }

    async fn complete_generation(&self, generation: SourceGeneration) -> Result<SourceGeneration> {
        generation::complete_generation(self, generation).await
    }

    async fn fail_generation(&self, generation: SourceGeneration) -> Result<SourceGeneration> {
        generation::fail_generation(self, generation).await
    }

    async fn publish_generation(
        &self,
        request: PublishGenerationRequest,
    ) -> Result<SourceGeneration> {
        generation::publish_generation(self, request).await
    }

    async fn update_document_status(&self, status: DocumentStatus) -> Result<()> {
        document::update_document_status(self, status).await
    }

    async fn record_cleanup_debt(&self, debt: CleanupDebt) -> Result<()> {
        cleanup::record_cleanup_debt(self, debt).await
    }

    async fn acquire_lease(&self, request: LeaseRequest) -> Result<Option<LeaseGuard>> {
        lease::acquire_lease(self, request).await
    }

    async fn release_lease(&self, lease_id: LeaseId, owner_id: String) -> Result<()> {
        lease::release_lease(self, lease_id, owner_id).await
    }

    async fn heartbeat_lease(
        &self,
        lease_id: LeaseId,
        owner_id: String,
        ttl_seconds: u64,
    ) -> Result<Option<LeaseGuard>> {
        lease::heartbeat_lease(self, lease_id, owner_id, ttl_seconds).await
    }

    async fn reset(&self) -> Result<()> {
        clear_ledger(&self.pool).await
    }

    async fn capabilities(&self) -> Result<LedgerStoreCapability> {
        Ok(CapabilityBase {
            name: "sqlite-ledger".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            owner_crate: "axon-ledger".to_string(),
            health: HealthStatus::Healthy,
            features: vec![
                "source_summary".to_string(),
                "manifest_diff".to_string(),
                "generation_publish".to_string(),
                "document_status".to_string(),
                "cleanup_debt".to_string(),
                "leases".to_string(),
            ],
            limits: MetadataMap::new(),
        }
        .into())
    }
}

impl SqliteLedgerStore {
    pub async fn document_status(
        &self,
        document_id: &DocumentId,
    ) -> Result<Option<DocumentStatus>> {
        document::document_status(self, document_id).await
    }

    pub async fn cleanup_debt_count(&self) -> Result<usize> {
        cleanup::cleanup_debt_count(self).await
    }

    pub async fn cleanup_debt(&self, debt_id: &CleanupDebtId) -> Result<Option<CleanupDebt>> {
        cleanup::cleanup_debt(self, debt_id).await
    }

    pub async fn foreign_keys_enabled(&self) -> Result<bool> {
        source::foreign_keys_enabled(self).await
    }
}
