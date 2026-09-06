//! SQLite-backed ledger store.

mod cleanup;
mod document;
mod generation;
mod lease;
mod manifest;
mod source;
mod util;

use std::str::FromStr;

#[cfg(test)]
pub(crate) mod snapshot_test_hook {
    use std::sync::{Arc, Mutex, OnceLock};

    use axon_api::source::SourceId;
    use tokio::sync::Notify;

    pub(crate) struct Hook {
        pub source_id: String,
        pub entered: Arc<Notify>,
        pub resume: Arc<Notify>,
    }

    static HOOK: OnceLock<Mutex<Option<Hook>>> = OnceLock::new();

    pub(crate) fn install(source_id: &SourceId) -> (Arc<Notify>, Arc<Notify>) {
        let entered = Arc::new(Notify::new());
        let resume = Arc::new(Notify::new());
        *HOOK
            .get_or_init(|| Mutex::new(None))
            .lock()
            .expect("hook lock") = Some(Hook {
            source_id: source_id.0.clone(),
            entered: Arc::clone(&entered),
            resume: Arc::clone(&resume),
        });
        (entered, resume)
    }

    pub(crate) async fn pause_once_after_read(source_id: &SourceId) {
        let hook = {
            let mut guard = HOOK
                .get_or_init(|| Mutex::new(None))
                .lock()
                .expect("hook lock");
            if guard
                .as_ref()
                .is_some_and(|hook| hook.source_id == source_id.0)
            {
                guard.take()
            } else {
                None
            }
        };
        if let Some(hook) = hook {
            hook.entered.notify_one();
            hook.resume.notified().await;
        }
    }
}

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
    /// `jobs.source_id` can FK to `sources(source_id)`; the ledger's contract
    /// tables are created by the composed cross-crate migration runner
    /// (`axon-jobs/src/migrations.rs`), which applies THIS crate's
    /// `migration_set()` against the shared pool, so this constructor does NOT
    /// run migrations. Use [`SqliteLedgerStore::connect`] only for a standalone,
    /// ledger-only database (tests, tooling).
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

    #[cfg(test)]
    pub(crate) fn pool_for_tests(&self) -> &SqlitePool {
        &self.pool
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
        // Single idempotent upsert, so re-running it is safe. This is the write
        // that still failed with `(code: 5) database is locked` after the job
        // store was made retry-aware — `busy_timeout` can expire under
        // sustained multi-process write pressure, and the ledger upsert runs on
        // every source acquisition.
        retry_ledger_write("ledger upsert_source", || {
            source::upsert_source(self, source.clone())
        })
        .await
    }

    async fn get_source(&self, source_id: SourceId) -> Result<Option<SourceSummary>> {
        source::get_source(self, source_id).await
    }

    async fn get_source_detail(&self, source_id: SourceId) -> Result<Option<LedgerSourceDetail>> {
        source::get_source_detail(self, source_id).await
    }

    async fn list_sources(&self, request: SourceListRequest) -> Result<Page<SourceSummary>> {
        source::list_sources(self, request).await
    }

    async fn put_manifest(&self, manifest: SourceManifest) -> Result<()> {
        self.put_manifest_ref(&manifest).await
    }

    async fn put_manifest_ref(&self, manifest: &SourceManifest) -> Result<()> {
        retry_ledger_write("ledger put manifest", || {
            manifest::put_manifest(self, manifest)
        })
        .await
    }

    async fn get_manifest(
        &self,
        source_id: SourceId,
        generation: SourceGenerationId,
    ) -> Result<Option<SourceManifest>> {
        manifest::read_manifest(self, &source_id, &generation).await
    }

    async fn get_manifest_metadata(
        &self,
        source_id: SourceId,
        generation: SourceGenerationId,
    ) -> Result<Option<MetadataMap>> {
        manifest::read_manifest_metadata(self, &source_id, &generation).await
    }

    async fn get_manifest_items(
        &self,
        source_id: SourceId,
        generation: SourceGenerationId,
        item_keys: Vec<SourceItemKey>,
    ) -> Result<Vec<ManifestItem>> {
        manifest::read_manifest_items(self, &source_id, &generation, item_keys).await
    }

    async fn get_manifest_items_with_metadata_key(
        &self,
        source_id: SourceId,
        generation: SourceGenerationId,
        item_keys: Vec<SourceItemKey>,
        metadata_key: String,
    ) -> Result<Vec<ManifestItem>> {
        manifest::read_manifest_items_with_metadata_key(
            self,
            &source_id,
            &generation,
            item_keys,
            &metadata_key,
        )
        .await
    }

    async fn diff_manifest(&self, manifest: SourceManifest) -> Result<SourceManifestDiff> {
        self.diff_manifest_ref(&manifest).await
    }

    async fn diff_manifest_ref(&self, manifest: &SourceManifest) -> Result<SourceManifestDiff> {
        manifest::diff_manifest(self, manifest).await
    }

    async fn create_generation(&self, source_id: SourceId) -> Result<SourceGeneration> {
        retry_ledger_write("ledger create generation", || {
            generation::create_generation(self, source_id.clone())
        })
        .await
    }

    async fn committed_generation(
        &self,
        source_id: SourceId,
    ) -> Result<Option<SourceGenerationId>> {
        generation::committed_generation(self, &source_id).await
    }

    async fn complete_generation(&self, generation: SourceGeneration) -> Result<SourceGeneration> {
        retry_ledger_write("ledger complete generation", || {
            generation::complete_generation(self, generation.clone())
        })
        .await
    }

    async fn fail_generation(&self, generation: SourceGeneration) -> Result<SourceGeneration> {
        retry_ledger_write("ledger fail generation", || {
            generation::fail_generation(self, generation.clone())
        })
        .await
    }

    async fn publish_generation(
        &self,
        request: PublishGenerationRequest,
    ) -> Result<SourceGeneration> {
        retry_ledger_write("ledger publish generation", || {
            generation::publish_generation(self, request.clone())
        })
        .await
    }

    async fn update_document_status(&self, status: DocumentStatus) -> Result<()> {
        retry_ledger_write("ledger document status", || {
            document::update_document_status(self, status.clone())
        })
        .await
    }

    async fn update_document_statuses(&self, statuses: Vec<DocumentStatus>) -> Result<()> {
        document::update_document_statuses(self, statuses).await
    }

    async fn publish_document_statuses(
        &self,
        source_id: SourceId,
        generation: SourceGenerationId,
        updated_at: Timestamp,
    ) -> Result<u64> {
        retry_ledger_write("ledger publish document statuses", || {
            document::publish_document_statuses(
                self,
                source_id.clone(),
                generation.clone(),
                updated_at.clone(),
            )
        })
        .await
    }

    async fn record_cleanup_debt(&self, debt: CleanupDebt) -> Result<()> {
        retry_ledger_write("ledger record cleanup debt", || {
            cleanup::record_cleanup_debt(self, debt.clone())
        })
        .await
    }

    async fn list_pending_cleanup_debt(&self, source_id: SourceId) -> Result<Vec<CleanupDebt>> {
        cleanup::list_pending_cleanup_debt(self, &source_id).await
    }

    async fn list_pending_cleanup_debt_after(
        &self,
        after: Option<CleanupDebtId>,
        limit: usize,
    ) -> Result<Vec<CleanupDebt>> {
        cleanup::list_pending_cleanup_debt_after(self, after.as_ref(), limit).await
    }

    async fn list_adapter_release_debt(&self, limit: usize) -> Result<Vec<CleanupDebt>> {
        cleanup::list_adapter_release_debt(self, limit).await
    }

    async fn resolve_cleanup_debt(&self, debt_id: CleanupDebtId) -> Result<()> {
        retry_ledger_write("ledger resolve cleanup debt", || {
            cleanup::resolve_cleanup_debt(self, &debt_id)
        })
        .await
    }

    async fn delete_generation(
        &self,
        source_id: SourceId,
        generation: SourceGenerationId,
    ) -> Result<u64> {
        retry_ledger_write("ledger delete generation", || {
            cleanup::delete_generation(self, &source_id, &generation)
        })
        .await
    }

    async fn acquire_lease(&self, request: LeaseRequest) -> Result<Option<LeaseGuard>> {
        retry_ledger_write("ledger acquire lease", || {
            lease::acquire_lease(self, request.clone())
        })
        .await
    }

    async fn release_lease(&self, lease_id: LeaseId, owner_id: String) -> Result<()> {
        retry_ledger_write("ledger release lease", || {
            lease::release_lease(self, lease_id.clone(), owner_id.clone())
        })
        .await
    }

    async fn heartbeat_lease(
        &self,
        lease_id: LeaseId,
        owner_id: String,
        ttl_seconds: u64,
    ) -> Result<Option<LeaseGuard>> {
        retry_ledger_write("ledger heartbeat lease", || {
            lease::heartbeat_lease(self, lease_id.clone(), owner_id.clone(), ttl_seconds)
        })
        .await
    }

    async fn reset(&self) -> Result<()> {
        retry_ledger_write("ledger reset", || clear_ledger(&self.pool)).await
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
                "source_listing".to_string(),
            ],
            limits: MetadataMap::new(),
        }
        .into())
    }
}

/// Retry every `LedgerStore` mutation at the public store boundary. Ledger
/// operations are atomic transactions or idempotent writes, so a retried busy
/// snapshot starts from a new SQLite read view without duplicating effects.
pub(crate) async fn retry_ledger_write<T, F, Fut>(what: &str, op: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    axon_core::sqlite::retry_on(
        what,
        |e: &ApiError| axon_core::sqlite::message_is_retryable_busy(&e.to_string()),
        op,
    )
    .await
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
