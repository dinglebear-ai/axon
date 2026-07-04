//! SQLite migration helpers for the ledger store.

use crate::store::Result;
use axon_api::migration::{MigrationSet, SqlMigration};
use axon_api::source::{ApiError, ErrorStage};
use sqlx::{Executor, SqlitePool};

/// Namespace under which the composed cross-crate runner tracks ledger
/// migrations (see [`axon_api::migration`]).
pub const MIGRATION_NAMESPACE: &str = "ledger";

/// Ordered ledger migration set, exposed for the composed cross-crate runner in
/// `axon-jobs`. The ledger owns the seven contract tables (`sources`,
/// `source_generations`, `source_manifests`, `source_items`, `document_status`,
/// `cleanup_debt`, `leases`) per the schema contract, so this is the SOLE
/// creator of them in the unified pool.
pub const MIGRATIONS: &[SqlMigration] = &[SqlMigration {
    version: 1,
    name: "0001_ledger_lifecycle",
    sql: include_str!("migrations/0001_ledger_lifecycle.sql"),
}];

/// The ledger's [`MigrationSet`] for composition into the unified runner.
pub fn migration_set() -> MigrationSet {
    MigrationSet::new(MIGRATION_NAMESPACE, MIGRATIONS)
}

pub async fn migrate_ledger(pool: &SqlitePool) -> Result<()> {
    sqlx::migrate!("src/migrations")
        .run(pool)
        .await
        .map_err(|error| {
            ApiError::new(
                "source.ledger.migration",
                ErrorStage::Upserting,
                format!("ledger SQLite migration failed: {error}"),
            )
        })?;
    Ok(())
}

pub(crate) async fn clear_ledger(pool: &SqlitePool) -> Result<()> {
    let mut tx = pool.begin().await.map_err(sqlite_error)?;
    for statement in [
        "DELETE FROM source_items",
        "DELETE FROM document_status",
        "DELETE FROM cleanup_debt",
        "DELETE FROM leases",
        "DELETE FROM source_manifests",
        "DELETE FROM source_generations",
        "DELETE FROM sources",
    ] {
        tx.execute(statement).await.map_err(sqlite_error)?;
    }
    tx.commit().await.map_err(sqlite_error)?;
    Ok(())
}

pub(crate) fn sqlite_error(error: sqlx::Error) -> ApiError {
    ApiError::new(
        "source.ledger.sqlite",
        ErrorStage::Upserting,
        format!("ledger SQLite operation failed: {error}"),
    )
}
