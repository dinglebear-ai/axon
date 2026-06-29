use crate::{
    ManifestDiff, ManifestItem, RefreshPreflight, SourceIdentity, SourceKind, SourcePhase,
    SourceStatus,
};
use anyhow::Context;
use sqlx::{Row, SqlitePool};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone)]
pub struct SourceLedgerStore {
    pool: SqlitePool,
}

impl SourceLedgerStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn ensure_source(&self, source: &SourceIdentity) -> anyhow::Result<()> {
        let now_ms = now_ms();
        sqlx::query(
            "INSERT INTO axon_source_sources (
                source_id, source_kind, collection, index_version, updated_at_ms
             ) VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(source_id) DO UPDATE SET
                source_kind = excluded.source_kind,
                collection = excluded.collection,
                index_version = excluded.index_version,
                updated_at_ms = excluded.updated_at_ms",
        )
        .bind(&source.source_id)
        .bind(source.source_kind.as_str())
        .bind(&source.collection)
        .bind(source.index_version)
        .bind(now_ms)
        .execute(&self.pool)
        .await
        .context("failed to ensure source ledger source")?;
        Ok(())
    }

    pub async fn acquire_lease(
        &self,
        source: &SourceIdentity,
        owner: &str,
        ttl_ms: i64,
    ) -> anyhow::Result<bool> {
        self.ensure_source(source).await?;
        let now_ms = now_ms();
        let expires_at = now_ms.saturating_add(ttl_ms.max(0));
        let result = sqlx::query(
            "UPDATE axon_source_sources
             SET lease_owner = ?, lease_expires_at_ms = ?, updated_at_ms = ?
             WHERE source_id = ?
               AND (lease_owner IS NULL OR lease_expires_at_ms <= ? OR lease_owner = ?)",
        )
        .bind(owner)
        .bind(expires_at)
        .bind(now_ms)
        .bind(&source.source_id)
        .bind(now_ms)
        .bind(owner)
        .execute(&self.pool)
        .await
        .context("failed to acquire source ledger lease")?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn preflight_refresh(
        &self,
        source_id: &str,
        now_ms: i64,
    ) -> anyhow::Result<RefreshPreflight> {
        let row = sqlx::query(
            "SELECT backoff_until_ms, backoff_dependency, last_error
             FROM axon_source_sources
             WHERE source_id = ?",
        )
        .bind(source_id)
        .fetch_optional(&self.pool)
        .await
        .context("failed to read source refresh preflight")?;

        let Some(row) = row else {
            return Ok(RefreshPreflight::Ready);
        };
        let until_ms: i64 = row.try_get("backoff_until_ms")?;
        if until_ms > now_ms {
            return Ok(RefreshPreflight::BackingOff {
                until_ms,
                dependency: row
                    .try_get::<Option<String>, _>("backoff_dependency")?
                    .unwrap_or_else(|| "unknown".to_string()),
                message: row
                    .try_get::<Option<String>, _>("last_error")?
                    .unwrap_or_else(|| "refresh is backing off".to_string()),
            });
        }
        Ok(RefreshPreflight::Ready)
    }

    pub async fn begin_generation(&self, source: &SourceIdentity) -> anyhow::Result<i64> {
        self.ensure_source(source).await?;
        let now_ms = now_ms();
        let mut tx = self
            .pool
            .begin()
            .await
            .context("failed to begin source generation transaction")?;
        let current: i64 = sqlx::query_scalar(
            "SELECT max_generation FROM axon_source_sources WHERE source_id = ?",
        )
        .bind(&source.source_id)
        .fetch_one(&mut *tx)
        .await
        .context("failed to read current source generation")?;
        let next = current.saturating_add(1);
        sqlx::query(
            "UPDATE axon_source_sources
             SET max_generation = ?, updated_at_ms = ?
             WHERE source_id = ?",
        )
        .bind(next)
        .bind(now_ms)
        .bind(&source.source_id)
        .execute(&mut *tx)
        .await
        .context("failed to allocate source generation")?;
        tx.commit()
            .await
            .context("failed to commit source generation allocation")?;
        Ok(next)
    }

    pub async fn record_manifest_item(
        &self,
        source_id: &str,
        generation: i64,
        item: ManifestItem,
    ) -> anyhow::Result<()> {
        let now_ms = now_ms();
        sqlx::query(
            "INSERT INTO axon_source_manifest_items (
                source_id, item_key, content_hash, size_bytes, indexed_generation, pending, updated_at_ms
             ) VALUES (?, ?, ?, ?, ?, 1, ?)
             ON CONFLICT(source_id, item_key) DO UPDATE SET
                content_hash = excluded.content_hash,
                size_bytes = excluded.size_bytes,
                indexed_generation = excluded.indexed_generation,
                pending = excluded.pending,
                updated_at_ms = excluded.updated_at_ms",
        )
        .bind(source_id)
        .bind(item.item_key)
        .bind(item.content_hash)
        .bind(item.size_bytes)
        .bind(generation)
        .bind(now_ms)
        .execute(&self.pool)
        .await
        .context("failed to record source manifest item")?;
        Ok(())
    }

    pub async fn diff_manifest(
        &self,
        source_id: &str,
        manifest: &[ManifestItem],
    ) -> anyhow::Result<ManifestDiff> {
        let rows = sqlx::query(
            "SELECT item_key, content_hash
             FROM axon_source_manifest_items
             WHERE source_id = ? AND pending = 0",
        )
        .bind(source_id)
        .fetch_all(&self.pool)
        .await
        .context("failed to read existing source manifest")?;

        let existing: BTreeMap<String, String> = rows
            .into_iter()
            .map(|row| {
                Ok((
                    row.try_get::<String, _>("item_key")?,
                    row.try_get::<String, _>("content_hash")?,
                ))
            })
            .collect::<Result<_, sqlx::Error>>()?;
        let incoming_keys: BTreeSet<&str> =
            manifest.iter().map(|item| item.item_key.as_str()).collect();
        let mut diff = ManifestDiff::default();

        for item in manifest {
            match existing.get(&item.item_key) {
                None => diff.added.push(item.clone()),
                Some(hash) if hash != &item.content_hash => {
                    diff.modified.push(item.clone());
                    diff.removed.push(item.item_key.clone());
                }
                Some(_) => {}
            }
        }

        for key in existing.keys() {
            if !incoming_keys.contains(key.as_str()) {
                diff.removed.push(key.clone());
            }
        }
        diff.removed.sort();
        diff.removed.dedup();
        Ok(diff)
    }

    pub async fn commit_generation(&self, source_id: &str, generation: i64) -> anyhow::Result<()> {
        let now_ms = now_ms();
        let mut tx = self
            .pool
            .begin()
            .await
            .context("failed to begin source generation commit")?;
        sqlx::query(
            "DELETE FROM axon_source_manifest_items
             WHERE source_id = ? AND pending = 0 AND indexed_generation < ?",
        )
        .bind(source_id)
        .bind(generation)
        .execute(&mut *tx)
        .await
        .context("failed to prune old source manifest items")?;
        sqlx::query(
            "UPDATE axon_source_manifest_items
             SET pending = 0, indexed_generation = ?, updated_at_ms = ?
             WHERE source_id = ? AND indexed_generation = ?",
        )
        .bind(generation)
        .bind(now_ms)
        .bind(source_id)
        .bind(generation)
        .execute(&mut *tx)
        .await
        .context("failed to commit source manifest items")?;
        sqlx::query(
            "UPDATE axon_source_sources
             SET committed_generation = MAX(committed_generation, ?),
                 max_generation = MAX(max_generation, ?),
                 last_success_at_ms = ?,
                 last_error = NULL,
                 backoff_until_ms = 0,
                 backoff_dependency = NULL,
                 updated_at_ms = ?
             WHERE source_id = ?",
        )
        .bind(generation)
        .bind(generation)
        .bind(now_ms)
        .bind(now_ms)
        .bind(source_id)
        .execute(&mut *tx)
        .await
        .context("failed to commit source generation")?;
        tx.commit()
            .await
            .context("failed to commit source generation transaction")?;
        Ok(())
    }

    pub async fn source_status(&self, source_id: &str) -> anyhow::Result<SourceStatus> {
        let row = sqlx::query(
            "SELECT source_kind, committed_generation, max_generation, backoff_until_ms,
                    last_error, updated_at_ms
             FROM axon_source_sources
             WHERE source_id = ?",
        )
        .bind(source_id)
        .fetch_one(&self.pool)
        .await
        .context("failed to read source status")?;
        let cleanup_debt_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM axon_source_cleanup_debt WHERE source_id = ?")
                .bind(source_id)
                .fetch_one(&self.pool)
                .await
                .context("failed to count source cleanup debt")?;
        let source_kind_text: String = row.try_get("source_kind")?;
        let backoff_until_ms: i64 = row.try_get("backoff_until_ms")?;
        let committed_generation: i64 = row.try_get("committed_generation")?;
        let max_generation: i64 = row.try_get("max_generation")?;
        Ok(SourceStatus {
            source_id: source_id.to_string(),
            source_kind: SourceKind::try_from(source_kind_text.as_str())?,
            phase: if backoff_until_ms > now_ms() {
                SourcePhase::BackingOff
            } else {
                SourcePhase::Idle
            },
            committed_generation,
            active_generation: (max_generation > committed_generation).then_some(max_generation),
            backoff_until_ms: (backoff_until_ms > 0).then_some(backoff_until_ms),
            last_error: row.try_get("last_error")?,
            cleanup_debt_count,
            updated_at_ms: row.try_get("updated_at_ms")?,
        })
    }

    pub async fn set_backoff(
        &self,
        source_id: &str,
        until_ms: i64,
        dependency: &str,
        message: &str,
    ) -> anyhow::Result<()> {
        let now_ms = now_ms();
        sqlx::query(
            "UPDATE axon_source_sources
             SET backoff_until_ms = ?, backoff_dependency = ?, last_error = ?,
                 last_checked_at_ms = ?, updated_at_ms = ?
             WHERE source_id = ?",
        )
        .bind(until_ms)
        .bind(dependency)
        .bind(message)
        .bind(now_ms)
        .bind(now_ms)
        .bind(source_id)
        .execute(&self.pool)
        .await
        .context("failed to set source backoff")?;
        Ok(())
    }

    pub async fn max_generation(&self, source_id: &str) -> anyhow::Result<i64> {
        sqlx::query_scalar("SELECT max_generation FROM axon_source_sources WHERE source_id = ?")
            .bind(source_id)
            .fetch_one(&self.pool)
            .await
            .context("failed to read source max generation")
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
