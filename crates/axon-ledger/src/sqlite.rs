//! SQLite-backed ledger store.

use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;

use async_trait::async_trait;
use axon_api::source::*;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};

use crate::migration::{clear_ledger, migrate_ledger, sqlite_error};
use crate::store::{LedgerStore, Result};

#[derive(Debug, Clone)]
pub struct SqliteLedgerStore {
    pool: SqlitePool,
}

impl SqliteLedgerStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn connect(path: &str) -> Result<Self> {
        let options = SqliteConnectOptions::from_str(path)
            .map_err(sqlite_error)?
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
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

#[async_trait]
impl LedgerStore for SqliteLedgerStore {
    async fn upsert_source(&self, source: SourceSummary) -> Result<()> {
        let source_id = source.source_id.0.clone();
        let summary_json = serde_json::to_string(&source).map_err(json_error)?;
        sqlx::query(
            r#"
            INSERT INTO sources (
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
            FROM sources
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

    async fn put_manifest(&self, manifest: SourceManifest) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(sqlite_error)?;
        ensure_generation_for_manifest_in_tx(&mut tx, &manifest).await?;
        let manifest_json = serde_json::to_string(&manifest).map_err(json_error)?;
        sqlx::query(
            r#"
            INSERT INTO source_manifests (
                source_id,
                generation,
                manifest_json,
                created_at
            ) VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(source_id, generation) DO UPDATE SET
                manifest_json = excluded.manifest_json,
                created_at = excluded.created_at
            "#,
        )
        .bind(&manifest.source_id.0)
        .bind(&manifest.generation.0)
        .bind(manifest_json)
        .bind(&manifest.created_at.0)
        .execute(&mut *tx)
        .await
        .map_err(sqlite_error)?;

        sqlx::query(
            r#"
            DELETE FROM source_items
            WHERE source_id = ?1 AND generation = ?2
            "#,
        )
        .bind(&manifest.source_id.0)
        .bind(&manifest.generation.0)
        .execute(&mut *tx)
        .await
        .map_err(sqlite_error)?;

        for item in &manifest.items {
            let item_json = serde_json::to_string(item).map_err(json_error)?;
            sqlx::query(
                r#"
                INSERT INTO source_items (
                    source_id,
                    source_item_key,
                    generation,
                    item_canonical_uri,
                    content_hash,
                    version,
                    mtime,
                    item_json
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                "#,
            )
            .bind(&item.source_id.0)
            .bind(&item.source_item_key.0)
            .bind(&manifest.generation.0)
            .bind(&item.canonical_uri)
            .bind(item.content_hash.as_deref())
            .bind(item.version.as_deref())
            .bind(item.mtime.as_ref().map(|value| value.0.as_str()))
            .bind(item_json)
            .execute(&mut *tx)
            .await
            .map_err(sqlite_error)?;
        }

        tx.commit().await.map_err(sqlite_error)?;
        Ok(())
    }

    async fn diff_manifest(&self, manifest: SourceManifest) -> Result<SourceManifestDiff> {
        let previous_generation = self.committed_generation(&manifest.source_id).await?;
        let previous = match &previous_generation {
            Some(generation) => {
                let manifest = self
                    .manifest(&manifest.source_id, generation)
                    .await?
                    .ok_or_else(|| {
                        ApiError::new(
                            "source.ledger.committed_manifest_missing",
                            ErrorStage::Diffing,
                            format!("committed manifest {} is missing", generation.0),
                        )
                        .with_source_id(manifest.source_id.0.clone())
                    })?;
                keyed_manifest_items(manifest.items)
            }
            None => BTreeMap::new(),
        };
        let SourceManifest {
            source_id,
            generation,
            items,
            ..
        } = manifest;
        let next = keyed_manifest_items(items);

        let mut added = Vec::new();
        let mut modified = Vec::new();
        let mut unchanged = Vec::new();
        for (key, item) in &next {
            match previous.get(key) {
                None => added.push(item.clone()),
                Some(old) if manifest_item_changed(old, item) => modified.push(item.clone()),
                Some(_) => unchanged.push(item.clone()),
            }
        }

        let next_keys = next.keys().cloned().collect::<BTreeSet<_>>();
        let removed = previous
            .into_iter()
            .filter_map(|(key, item)| (!next_keys.contains(&key)).then_some(item))
            .collect::<Vec<_>>();

        Ok(SourceManifestDiff {
            header: stage_header(PipelinePhase::Diffing),
            source_id,
            previous_generation,
            next_generation: generation,
            counts: DiffCounts {
                added: added.len() as u64,
                modified: modified.len() as u64,
                removed: removed.len() as u64,
                unchanged: unchanged.len() as u64,
                skipped: 0,
                failed: 0,
            },
            added,
            modified,
            removed,
            unchanged,
            skipped: Vec::new(),
            failed: Vec::new(),
        })
    }

    async fn create_generation(&self, source_id: SourceId) -> Result<SourceGeneration> {
        let previous_generation = self.committed_generation(&source_id).await?;
        let next_sequence: i64 = sqlx::query_scalar(
            r#"
            SELECT COALESCE(MAX(sequence), 0) + 1
            FROM source_generations
            WHERE source_id = ?1
            "#,
        )
        .bind(&source_id.0)
        .fetch_one(&self.pool)
        .await
        .map_err(sqlite_error)?;
        let generation = SourceGeneration {
            source_id: source_id.clone(),
            generation: SourceGenerationId::new(format!("gen_{next_sequence}")),
            status: LifecycleStatus::Running,
            publish_state: PublishState::Writing,
            created_at: timestamp(),
            published_at: None,
            item_counts: ItemCounts {
                added: 0,
                modified: 0,
                removed: 0,
                unchanged: 0,
                failed: 0,
            },
            document_counts: DocumentCounts {
                discovered: 0,
                prepared: 0,
                embedded: 0,
                published: 0,
                failed: 0,
            },
            cleanup_debt: Vec::new(),
            previous_generation,
        };
        self.upsert_generation(&generation, next_sequence).await?;
        Ok(generation)
    }

    async fn publish_generation(&self, generation: SourceGeneration) -> Result<()> {
        if !matches!(
            generation.status,
            LifecycleStatus::Completed | LifecycleStatus::CompletedDegraded
        ) {
            return Err(ApiError::new(
                "source.ledger.generation_not_publishable",
                ErrorStage::Publishing,
                format!(
                    "generation {} has non-publishable status {:?}",
                    generation.generation.0, generation.status
                ),
            )
            .with_source_id(generation.source_id.0));
        }

        let mut tx = self.pool.begin().await.map_err(sqlite_error)?;
        let manifest_exists: Option<i64> = sqlx::query_scalar(
            r#"
            SELECT 1
            FROM source_manifests
            WHERE source_id = ?1 AND generation = ?2
            "#,
        )
        .bind(&generation.source_id.0)
        .bind(&generation.generation.0)
        .fetch_optional(&mut *tx)
        .await
        .map_err(sqlite_error)?;
        if manifest_exists.is_none() {
            return Err(ApiError::new(
                "source.ledger.manifest_missing",
                ErrorStage::Publishing,
                format!(
                    "generation {} cannot publish without a manifest",
                    generation.generation.0
                ),
            )
            .with_source_id(generation.source_id.0));
        }

        let previous = current_committed_generation_in_tx(&mut tx, &generation.source_id).await?;
        if previous != generation.previous_generation {
            return Err(ApiError::new(
                "source.ledger.generation_baseline_changed",
                ErrorStage::Publishing,
                format!(
                    "generation {} was based on {:?}, but committed generation is {:?}",
                    generation.generation.0, generation.previous_generation, previous
                ),
            )
            .with_source_id(generation.source_id.0));
        }

        let mut committed_generation = generation.clone();
        committed_generation.publish_state = PublishState::Committed;
        committed_generation.published_at = Some(timestamp());
        upsert_generation_in_tx(&mut tx, &committed_generation, None).await?;
        record_removed_item_cleanup_debt_in_tx(&mut tx, &committed_generation, previous.as_ref())
            .await?;

        let result = sqlx::query(
            r#"
            UPDATE sources
            SET committed_generation = ?1,
                updated_at = ?2
            WHERE source_id = ?3
              AND (
                (committed_generation IS NULL AND ?4 IS NULL)
                OR committed_generation = ?4
              )
            "#,
        )
        .bind(&committed_generation.generation.0)
        .bind(timestamp().0)
        .bind(&committed_generation.source_id.0)
        .bind(previous.as_ref().map(|value| value.0.as_str()))
        .execute(&mut *tx)
        .await
        .map_err(sqlite_error)?;
        if result.rows_affected() != 1 {
            return Err(ApiError::new(
                "source.ledger.generation_baseline_changed",
                ErrorStage::Publishing,
                format!(
                    "source {} committed generation changed during publish",
                    committed_generation.source_id.0
                ),
            )
            .with_source_id(committed_generation.source_id.0));
        }
        tx.commit().await.map_err(sqlite_error)?;
        Ok(())
    }

    async fn update_document_status(&self, status: DocumentStatus) -> Result<()> {
        let status_json = serde_json::to_string(&status).map_err(json_error)?;
        sqlx::query(
            r#"
            INSERT INTO document_status (
                document_id,
                source_id,
                source_item_key,
                generation,
                status,
                status_json,
                updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(document_id) DO UPDATE SET
                source_id = excluded.source_id,
                source_item_key = excluded.source_item_key,
                generation = excluded.generation,
                status = excluded.status,
                status_json = excluded.status_json,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&status.document_id.0)
        .bind(&status.source_id.0)
        .bind(&status.source_item_key.0)
        .bind(&status.generation.0)
        .bind(format!("{:?}", status.status))
        .bind(status_json)
        .bind(&status.updated_at.0)
        .execute(&self.pool)
        .await
        .map_err(sqlite_error)?;
        Ok(())
    }

    async fn record_cleanup_debt(&self, debt: CleanupDebt) -> Result<()> {
        let debt_json = serde_json::to_string(&debt).map_err(json_error)?;
        let selector_hash = cleanup_selector_hash(&debt.selector)?;
        let generation_key = debt
            .generation
            .as_ref()
            .map(|value| value.0.as_str())
            .unwrap_or("");
        sqlx::query(
            r#"
            INSERT INTO cleanup_debt (
                debt_id,
                job_id,
                source_id,
                generation,
                generation_key,
                kind,
                selector_hash,
                status,
                debt_json,
                attempts,
                created_at,
                next_retry_at,
                completed_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            ON CONFLICT(source_id, generation_key, kind, selector_hash) DO UPDATE SET
                debt_id = CASE
                    WHEN cleanup_debt.completed_at IS NULL THEN excluded.debt_id
                    ELSE cleanup_debt.debt_id
                END,
                job_id = CASE
                    WHEN cleanup_debt.completed_at IS NULL THEN excluded.job_id
                    ELSE cleanup_debt.job_id
                END,
                status = CASE
                    WHEN cleanup_debt.completed_at IS NULL THEN excluded.status
                    ELSE cleanup_debt.status
                END,
                debt_json = CASE
                    WHEN cleanup_debt.completed_at IS NULL THEN excluded.debt_json
                    ELSE cleanup_debt.debt_json
                END,
                attempts = MAX(cleanup_debt.attempts, excluded.attempts),
                next_retry_at = CASE
                    WHEN cleanup_debt.completed_at IS NULL THEN excluded.next_retry_at
                    ELSE cleanup_debt.next_retry_at
                END,
                completed_at = COALESCE(cleanup_debt.completed_at, excluded.completed_at)
            "#,
        )
        .bind(&debt.debt_id.0)
        .bind(debt.job_id.0.to_string())
        .bind(&debt.source_id.0)
        .bind(debt.generation.as_ref().map(|value| value.0.as_str()))
        .bind(generation_key)
        .bind(format!("{:?}", debt.kind))
        .bind(selector_hash)
        .bind(format!("{:?}", debt.status))
        .bind(debt_json)
        .bind(i64::from(debt.attempts))
        .bind(&debt.created_at.0)
        .bind(debt.next_retry_at.as_ref().map(|value| value.0.as_str()))
        .bind(debt.completed_at.as_ref().map(|value| value.0.as_str()))
        .execute(&self.pool)
        .await
        .map_err(sqlite_error)?;
        Ok(())
    }

    async fn acquire_lease(&self, request: LeaseRequest) -> Result<Option<LeaseGuard>> {
        let mut tx = self.pool.begin().await.map_err(sqlite_error)?;
        let existing = sqlx::query(
            r#"
            SELECT lease_id, owner_id, expires_at
            FROM leases
            WHERE lease_key = ?1
            "#,
        )
        .bind(&request.lease_key)
        .fetch_optional(&mut *tx)
        .await
        .map_err(sqlite_error)?;

        if let Some(row) = existing {
            let expires_at: String = row.get("expires_at");
            let owner_id: String = row.get("owner_id");
            let lease_id: String = row.get("lease_id");
            if expires_at > request.acquired_at.0 {
                if owner_id != request.owner_id {
                    tx.rollback().await.map_err(sqlite_error)?;
                    return Ok(None);
                }

                let guard = LeaseGuard {
                    lease_id: LeaseId::new(lease_id),
                    lease_key: request.lease_key,
                    owner_id: request.owner_id,
                    expires_at: add_seconds(&request.acquired_at, request.ttl_seconds),
                    heartbeat_at: request.acquired_at.clone(),
                    acquired_at: request.acquired_at,
                    job_id: request.job_id,
                    metadata: request.metadata,
                };
                let lease_json = serde_json::to_string(&guard).map_err(json_error)?;
                sqlx::query(
                    r#"
                    UPDATE leases
                    SET expires_at = ?1,
                        heartbeat_at = ?2,
                        job_id = ?3,
                        lease_json = ?4
                    WHERE lease_id = ?5
                    "#,
                )
                .bind(&guard.expires_at.0)
                .bind(&guard.heartbeat_at.0)
                .bind(guard.job_id.map(|value| value.0.to_string()))
                .bind(lease_json)
                .bind(&guard.lease_id.0)
                .execute(&mut *tx)
                .await
                .map_err(sqlite_error)?;
                tx.commit().await.map_err(sqlite_error)?;
                return Ok(Some(guard));
            }
            sqlx::query("DELETE FROM leases WHERE lease_id = ?1")
                .bind(lease_id)
                .execute(&mut *tx)
                .await
                .map_err(sqlite_error)?;
        }

        let guard = LeaseGuard {
            lease_id: LeaseId::new(format!("lease_{}", uuid::Uuid::new_v4())),
            lease_key: request.lease_key,
            owner_id: request.owner_id,
            expires_at: add_seconds(&request.acquired_at, request.ttl_seconds),
            heartbeat_at: request.acquired_at.clone(),
            acquired_at: request.acquired_at,
            job_id: request.job_id,
            metadata: request.metadata,
        };
        let lease_json = serde_json::to_string(&guard).map_err(json_error)?;
        sqlx::query(
            r#"
            INSERT OR IGNORE INTO leases (
                lease_id,
                lease_key,
                owner_id,
                acquired_at,
                expires_at,
                heartbeat_at,
                job_id,
                lease_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
        )
        .bind(&guard.lease_id.0)
        .bind(&guard.lease_key)
        .bind(&guard.owner_id)
        .bind(&guard.acquired_at.0)
        .bind(&guard.expires_at.0)
        .bind(&guard.heartbeat_at.0)
        .bind(guard.job_id.map(|value| value.0.to_string()))
        .bind(lease_json)
        .execute(&mut *tx)
        .await
        .map_err(sqlite_error)?;
        let inserted = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)
            FROM leases
            WHERE lease_id = ?1
            "#,
        )
        .bind(&guard.lease_id.0)
        .fetch_one(&mut *tx)
        .await
        .map_err(sqlite_error)?
            == 1;
        if !inserted {
            tx.rollback().await.map_err(sqlite_error)?;
            return Ok(None);
        }
        tx.commit().await.map_err(sqlite_error)?;
        Ok(Some(guard))
    }

    async fn release_lease(&self, lease_id: LeaseId) -> Result<()> {
        sqlx::query("DELETE FROM leases WHERE lease_id = ?1")
            .bind(&lease_id.0)
            .execute(&self.pool)
            .await
            .map_err(sqlite_error)?;
        Ok(())
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
    async fn committed_generation(
        &self,
        source_id: &SourceId,
    ) -> Result<Option<SourceGenerationId>> {
        let committed_generation: Option<String> = sqlx::query_scalar(
            r#"
            SELECT committed_generation
            FROM sources
            WHERE source_id = ?1
            "#,
        )
        .bind(&source_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlite_error)?
        .flatten();
        Ok(committed_generation.map(SourceGenerationId::new))
    }

    async fn manifest(
        &self,
        source_id: &SourceId,
        generation: &SourceGenerationId,
    ) -> Result<Option<SourceManifest>> {
        let row = sqlx::query(
            r#"
            SELECT manifest_json
            FROM source_manifests
            WHERE source_id = ?1 AND generation = ?2
            "#,
        )
        .bind(&source_id.0)
        .bind(&generation.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlite_error)?;

        row.map(|row| {
            let manifest_json: String = row.get("manifest_json");
            serde_json::from_str(&manifest_json).map_err(json_error)
        })
        .transpose()
    }

    async fn upsert_generation(&self, generation: &SourceGeneration, sequence: i64) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(sqlite_error)?;
        upsert_generation_in_tx(&mut tx, generation, Some(sequence)).await?;
        tx.commit().await.map_err(sqlite_error)?;
        Ok(())
    }

    pub async fn document_status(
        &self,
        document_id: &DocumentId,
    ) -> Result<Option<DocumentStatus>> {
        let row = sqlx::query(
            r#"
            SELECT status_json
            FROM document_status
            WHERE document_id = ?1
            "#,
        )
        .bind(&document_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlite_error)?;

        row.map(|row| {
            let status_json: String = row.get("status_json");
            serde_json::from_str(&status_json).map_err(json_error)
        })
        .transpose()
    }

    pub async fn cleanup_debt_count(&self) -> Result<usize> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cleanup_debt")
            .fetch_one(&self.pool)
            .await
            .map_err(sqlite_error)?;
        Ok(count as usize)
    }

    pub async fn cleanup_debt(&self, debt_id: &CleanupDebtId) -> Result<Option<CleanupDebt>> {
        let row = sqlx::query(
            r#"
            SELECT debt_json
            FROM cleanup_debt
            WHERE debt_id = ?1
            "#,
        )
        .bind(&debt_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlite_error)?;

        row.map(|row| {
            let debt_json: String = row.get("debt_json");
            serde_json::from_str(&debt_json).map_err(json_error)
        })
        .transpose()
    }

    pub async fn foreign_keys_enabled(&self) -> Result<bool> {
        let enabled: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
            .fetch_one(&self.pool)
            .await
            .map_err(sqlite_error)?;
        Ok(enabled == 1)
    }
}

async fn upsert_generation_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    generation: &SourceGeneration,
    sequence: Option<i64>,
) -> Result<()> {
    let sequence = match sequence {
        Some(sequence) => sequence,
        None => sqlx::query_scalar(
            r#"
            SELECT sequence
            FROM source_generations
            WHERE source_id = ?1 AND generation = ?2
            "#,
        )
        .bind(&generation.source_id.0)
        .bind(&generation.generation.0)
        .fetch_one(&mut **tx)
        .await
        .map_err(sqlite_error)?,
    };
    let generation_json = serde_json::to_string(generation).map_err(json_error)?;
    sqlx::query(
        r#"
        INSERT INTO source_generations (
            source_id,
            generation,
            sequence,
            status,
            publish_state,
            generation_json,
            created_at,
            published_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        ON CONFLICT(source_id, generation) DO UPDATE SET
            sequence = COALESCE(excluded.sequence, source_generations.sequence),
            status = excluded.status,
            publish_state = excluded.publish_state,
            generation_json = excluded.generation_json,
            published_at = excluded.published_at
        "#,
    )
    .bind(&generation.source_id.0)
    .bind(&generation.generation.0)
    .bind(sequence)
    .bind(format!("{:?}", generation.status))
    .bind(format!("{:?}", generation.publish_state))
    .bind(generation_json)
    .bind(&generation.created_at.0)
    .bind(
        generation
            .published_at
            .as_ref()
            .map(|value| value.0.as_str()),
    )
    .execute(&mut **tx)
    .await
    .map_err(sqlite_error)?;
    Ok(())
}

async fn ensure_generation_for_manifest_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    manifest: &SourceManifest,
) -> Result<()> {
    let exists: Option<i64> = sqlx::query_scalar(
        r#"
        SELECT 1
        FROM source_generations
        WHERE source_id = ?1 AND generation = ?2
        "#,
    )
    .bind(&manifest.source_id.0)
    .bind(&manifest.generation.0)
    .fetch_optional(&mut **tx)
    .await
    .map_err(sqlite_error)?;
    if exists.is_some() {
        return Ok(());
    }

    let sequence: i64 = sqlx::query_scalar(
        r#"
        SELECT COALESCE(MAX(sequence), 0) + 1
        FROM source_generations
        WHERE source_id = ?1
        "#,
    )
    .bind(&manifest.source_id.0)
    .fetch_one(&mut **tx)
    .await
    .map_err(sqlite_error)?;
    let generation = SourceGeneration {
        source_id: manifest.source_id.clone(),
        generation: manifest.generation.clone(),
        status: LifecycleStatus::Running,
        publish_state: PublishState::Writing,
        created_at: manifest.created_at.clone(),
        published_at: None,
        item_counts: ItemCounts {
            added: 0,
            modified: 0,
            removed: 0,
            unchanged: manifest.items.len() as u64,
            failed: 0,
        },
        document_counts: DocumentCounts {
            discovered: manifest.items.len() as u64,
            prepared: 0,
            embedded: 0,
            published: 0,
            failed: 0,
        },
        cleanup_debt: Vec::new(),
        previous_generation: current_committed_generation_in_tx(tx, &manifest.source_id).await?,
    };
    upsert_generation_in_tx(tx, &generation, Some(sequence)).await
}

async fn current_committed_generation_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    source_id: &SourceId,
) -> Result<Option<SourceGenerationId>> {
    let committed_generation: Option<String> = sqlx::query_scalar(
        r#"
        SELECT committed_generation
        FROM sources
        WHERE source_id = ?1
        "#,
    )
    .bind(&source_id.0)
    .fetch_optional(&mut **tx)
    .await
    .map_err(sqlite_error)?
    .flatten();
    Ok(committed_generation.map(SourceGenerationId::new))
}

async fn record_removed_item_cleanup_debt_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    generation: &SourceGeneration,
    previous_generation: Option<&SourceGenerationId>,
) -> Result<()> {
    let Some(previous_generation) = previous_generation else {
        return Ok(());
    };
    let previous_items =
        manifest_items_in_tx(tx, &generation.source_id, previous_generation).await?;
    let next_items =
        manifest_items_in_tx(tx, &generation.source_id, &generation.generation).await?;
    let next_keys = next_items
        .iter()
        .map(|item| item.source_item_key.clone())
        .collect::<BTreeSet<_>>();

    for item in previous_items {
        if next_keys.contains(&item.source_item_key) {
            continue;
        }
        let debt = CleanupDebt {
            debt_id: CleanupDebtId::new(format!(
                "debt_{}",
                uuid::Uuid::new_v5(
                    &uuid::Uuid::NAMESPACE_URL,
                    format!(
                        "{}:{}:{}",
                        generation.source_id.0, previous_generation.0, item.source_item_key.0
                    )
                    .as_bytes(),
                )
            )),
            job_id: JobId::new(uuid::Uuid::from_u128(0)),
            source_id: generation.source_id.clone(),
            generation: Some(previous_generation.clone()),
            kind: CleanupDebtKind::VectorDelete,
            selector: CleanupSelector::SourceItem {
                source_id: generation.source_id.clone(),
                source_item_key: item.source_item_key,
                generation: previous_generation.clone(),
            },
            status: LifecycleStatus::Pending,
            created_at: timestamp(),
            attempts: 0,
            last_error: None,
            next_retry_at: None,
            completed_at: None,
        };
        insert_cleanup_debt_in_tx(tx, debt).await?;
    }
    Ok(())
}

async fn manifest_items_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    source_id: &SourceId,
    generation: &SourceGenerationId,
) -> Result<Vec<ManifestItem>> {
    let rows = sqlx::query(
        r#"
        SELECT item_json
        FROM source_items
        WHERE source_id = ?1 AND generation = ?2
        "#,
    )
    .bind(&source_id.0)
    .bind(&generation.0)
    .fetch_all(&mut **tx)
    .await
    .map_err(sqlite_error)?;

    rows.into_iter()
        .map(|row| {
            let item_json: String = row.get("item_json");
            serde_json::from_str(&item_json).map_err(json_error)
        })
        .collect()
}

async fn insert_cleanup_debt_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    debt: CleanupDebt,
) -> Result<()> {
    let debt_json = serde_json::to_string(&debt).map_err(json_error)?;
    let selector_hash = cleanup_selector_hash(&debt.selector)?;
    let generation_key = debt
        .generation
        .as_ref()
        .map(|value| value.0.as_str())
        .unwrap_or("");
    sqlx::query(
        r#"
        INSERT INTO cleanup_debt (
            debt_id,
            job_id,
            source_id,
            generation,
            generation_key,
            kind,
            selector_hash,
            status,
            debt_json,
            attempts,
            created_at,
            next_retry_at,
            completed_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        ON CONFLICT(source_id, generation_key, kind, selector_hash) DO NOTHING
        "#,
    )
    .bind(&debt.debt_id.0)
    .bind(debt.job_id.0.to_string())
    .bind(&debt.source_id.0)
    .bind(debt.generation.as_ref().map(|value| value.0.as_str()))
    .bind(generation_key)
    .bind(format!("{:?}", debt.kind))
    .bind(selector_hash)
    .bind(format!("{:?}", debt.status))
    .bind(debt_json)
    .bind(i64::from(debt.attempts))
    .bind(&debt.created_at.0)
    .bind(debt.next_retry_at.as_ref().map(|value| value.0.as_str()))
    .bind(debt.completed_at.as_ref().map(|value| value.0.as_str()))
    .execute(&mut **tx)
    .await
    .map_err(sqlite_error)?;
    Ok(())
}

fn cleanup_selector_hash(selector: &CleanupSelector) -> Result<String> {
    let selector_json = serde_json::to_vec(selector).map_err(json_error)?;
    Ok(uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, &selector_json).to_string())
}

fn keyed_manifest_items(items: Vec<ManifestItem>) -> BTreeMap<SourceItemKey, ManifestItem> {
    items
        .into_iter()
        .map(|item| (item.source_item_key.clone(), item))
        .collect()
}

fn manifest_item_changed(old: &ManifestItem, next: &ManifestItem) -> bool {
    old.content_hash != next.content_hash || old.version != next.version || old.mtime != next.mtime
}

fn stage_header(phase: PipelinePhase) -> StageResultHeader {
    StageResultHeader {
        job_id: JobId::new(uuid::Uuid::from_u128(0)),
        stage_id: StageId::new(uuid::Uuid::from_u128(0)),
        phase,
        status: LifecycleStatus::Completed,
        started_at: timestamp(),
        completed_at: Some(timestamp()),
        counts: StageCounts {
            items_total: None,
            items_done: 0,
            documents_total: None,
            documents_done: 0,
            chunks_total: None,
            chunks_done: 0,
            bytes_total: None,
            bytes_done: 0,
        },
        warnings: Vec::new(),
        error: None,
    }
}

fn timestamp() -> Timestamp {
    Timestamp("2026-07-01T00:00:00Z".to_string())
}

fn add_seconds(timestamp: &Timestamp, seconds: u64) -> Timestamp {
    let parsed = chrono::DateTime::parse_from_rfc3339(&timestamp.0)
        .map(|value| value.with_timezone(&chrono::Utc));
    match parsed {
        Ok(value) => Timestamp((value + chrono::Duration::seconds(seconds as i64)).to_rfc3339()),
        Err(_) => timestamp.clone(),
    }
}

fn json_error(error: serde_json::Error) -> ApiError {
    ApiError::new(
        "source.ledger.json",
        ErrorStage::Upserting,
        format!("ledger JSON operation failed: {error}"),
    )
}
