//! SQLite-backed ledger store.

use async_trait::async_trait;
use axon_api::source::*;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{Row, SqlitePool};

use crate::migration::{migrate_ledger, sqlite_error};
use crate::store::{LedgerStore, Result};

#[derive(Debug, Clone)]
pub struct SqliteLedgerStore {
    pool: SqlitePool,
}

impl SqliteLedgerStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn in_memory() -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .map_err(sqlite_error)?;
        migrate_ledger(&pool).await?;
        Ok(Self::new(pool))
    }
}

#[async_trait]
impl LedgerStore for SqliteLedgerStore {
    async fn upsert_source(&self, source: SourceSummary) -> Result<()> {
        let source_id = source.source_id.0.clone();
        let summary_json = serde_json::to_string(&source).map_err(json_error)?;
        sqlx::query(
            r#"
            INSERT INTO axon_ledger_sources (
                source_id,
                summary_json,
                created_at,
                updated_at
            ) VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(source_id) DO UPDATE SET
                summary_json = excluded.summary_json,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(source_id)
        .bind(summary_json)
        .bind(source.created_at.0)
        .bind(source.updated_at.0)
        .execute(&self.pool)
        .await
        .map_err(sqlite_error)?;

        Ok(())
    }

    async fn get_source(&self, source_id: SourceId) -> Result<Option<SourceSummary>> {
        let row = sqlx::query(
            r#"
            SELECT summary_json
            FROM axon_ledger_sources
            WHERE source_id = ?1
            "#,
        )
        .bind(source_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlite_error)?;

        row.map(|row| {
            let summary_json: String = row.get("summary_json");
            serde_json::from_str(&summary_json).map_err(json_error)
        })
        .transpose()
    }

    async fn put_manifest(&self, _manifest: SourceManifest) -> Result<()> {
        Err(unimplemented_error("put_manifest"))
    }

    async fn diff_manifest(&self, _manifest: SourceManifest) -> Result<SourceManifestDiff> {
        Err(unimplemented_error("diff_manifest"))
    }

    async fn create_generation(&self, _source_id: SourceId) -> Result<SourceGeneration> {
        Err(unimplemented_error("create_generation"))
    }

    async fn publish_generation(&self, _generation: SourceGeneration) -> Result<()> {
        Err(unimplemented_error("publish_generation"))
    }

    async fn update_document_status(&self, _status: DocumentStatus) -> Result<()> {
        Err(unimplemented_error("update_document_status"))
    }

    async fn record_cleanup_debt(&self, _debt: CleanupDebt) -> Result<()> {
        Err(unimplemented_error("record_cleanup_debt"))
    }

    async fn reset(&self) -> Result<()> {
        sqlx::query("DELETE FROM axon_ledger_sources")
            .execute(&self.pool)
            .await
            .map_err(sqlite_error)?;
        Ok(())
    }

    async fn capabilities(&self) -> Result<LedgerStoreCapability> {
        Ok(CapabilityBase {
            name: "sqlite-ledger".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            owner_crate: "axon-ledger".to_string(),
            health: HealthStatus::Healthy,
            features: vec!["source_summary".to_string()],
            limits: MetadataMap::new(),
        }
        .into())
    }
}

fn json_error(error: serde_json::Error) -> ApiError {
    ApiError::new(
        "source.ledger.json",
        ErrorStage::Upserting,
        format!("ledger JSON operation failed: {error}"),
    )
}

fn unimplemented_error(operation: &str) -> ApiError {
    ApiError::new(
        "source.ledger.sqlite_unimplemented",
        ErrorStage::Validation,
        format!("SQLite ledger operation is not implemented yet: {operation}"),
    )
}
