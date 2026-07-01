use crate::{
    CleanupDebtItem, ManifestDiff, ManifestItem, RefreshPreflight, SourceIdentity, SourceKind,
    SourcePhase, SourceStatus, StaleManifestItem,
};
use anyhow::Context;
use sqlx::{Row, SqlitePool};
use std::collections::{BTreeMap, BTreeSet};

mod cleanup;
mod commit;

#[derive(Debug, Clone)]
pub struct SourceLedgerStore {
    pub(super) pool: SqlitePool,
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

    pub async fn release_lease(&self, source_id: &str, owner: &str) -> anyhow::Result<()> {
        let result = sqlx::query(
            "UPDATE axon_source_sources
             SET lease_owner = NULL, lease_expires_at_ms = 0, updated_at_ms = ?
             WHERE source_id = ? AND lease_owner = ?",
        )
        .bind(now_ms())
        .bind(source_id)
        .bind(owner)
        .execute(&self.pool)
        .await
        .context("failed to release source ledger lease")?;
        if result.rows_affected() != 1 {
            anyhow::bail!("source ledger lease for {source_id} is no longer owned by {owner}");
        }
        Ok(())
    }

    pub async fn extend_lease_for_owner(
        &self,
        source_id: &str,
        owner: &str,
        ttl_ms: i64,
    ) -> anyhow::Result<()> {
        let now_ms = now_ms();
        self.validate_owner_lease_active(source_id, owner, now_ms)
            .await?;
        let expires_at = now_ms.saturating_add(ttl_ms.max(0));
        let result = sqlx::query(
            "UPDATE axon_source_sources
             SET lease_expires_at_ms = ?, updated_at_ms = ?
             WHERE source_id = ? AND lease_owner = ? AND lease_expires_at_ms > ?",
        )
        .bind(expires_at)
        .bind(now_ms)
        .bind(source_id)
        .bind(owner)
        .bind(now_ms)
        .execute(&self.pool)
        .await
        .context("failed to extend source ledger lease")?;
        if result.rows_affected() != 1 {
            anyhow::bail!("source ledger lease for {source_id} is no longer owned by {owner}");
        }
        Ok(())
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
        self.begin_generation_inner(source, None).await
    }

    pub async fn begin_generation_for_owner(
        &self,
        source: &SourceIdentity,
        owner: &str,
    ) -> anyhow::Result<i64> {
        self.begin_generation_inner(source, Some(owner)).await
    }

    async fn begin_generation_inner(
        &self,
        source: &SourceIdentity,
        owner: Option<&str>,
    ) -> anyhow::Result<i64> {
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
        if let Some(owner) = owner {
            validate_owner_lease_active_in_tx(&mut tx, &source.source_id, owner, now_ms).await?;
        }
        let next = current.saturating_add(1);
        let result = sqlx::query(
            "UPDATE axon_source_sources
             SET max_generation = ?, updated_at_ms = ?
             WHERE source_id = ?
               AND (
                 (? IS NULL AND (lease_owner IS NULL OR lease_expires_at_ms <= ?))
                 OR (? IS NOT NULL AND lease_owner = ? AND lease_expires_at_ms > ?)
               )",
        )
        .bind(next)
        .bind(now_ms)
        .bind(&source.source_id)
        .bind(owner)
        .bind(now_ms)
        .bind(owner)
        .bind(owner)
        .bind(now_ms)
        .execute(&mut *tx)
        .await
        .context("failed to allocate source generation")?;
        if result.rows_affected() != 1 {
            anyhow::bail!(
                "source ledger lease for {} was lost before generation allocation",
                source.source_id
            );
        }
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
             ON CONFLICT(source_id, indexed_generation, item_key) DO UPDATE SET
                content_hash = excluded.content_hash,
                size_bytes = excluded.size_bytes,
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
            "SELECT item_key, content_hash, indexed_generation
             FROM axon_source_manifest_items
             WHERE source_id = ? AND pending = 0",
        )
        .bind(source_id)
        .fetch_all(&self.pool)
        .await
        .context("failed to read existing source manifest")?;

        let existing: BTreeMap<String, (String, i64)> = rows
            .into_iter()
            .map(|row| {
                Ok((
                    row.try_get::<String, _>("item_key")?,
                    (
                        row.try_get::<String, _>("content_hash")?,
                        row.try_get::<i64, _>("indexed_generation")?,
                    ),
                ))
            })
            .collect::<Result<_, sqlx::Error>>()?;
        let incoming_keys: BTreeSet<&str> =
            manifest.iter().map(|item| item.item_key.as_str()).collect();
        let mut diff = ManifestDiff::default();

        for item in manifest {
            match existing.get(&item.item_key) {
                None => diff.added.push(item.clone()),
                Some((hash, indexed_generation)) if hash != &item.content_hash => {
                    diff.modified.push(item.clone());
                    diff.removed.push(StaleManifestItem {
                        item_key: item.item_key.clone(),
                        indexed_generation: *indexed_generation,
                    });
                }
                Some(_) => {}
            }
        }

        for (key, (_, indexed_generation)) in &existing {
            if !incoming_keys.contains(key.as_str()) {
                diff.removed.push(StaleManifestItem {
                    item_key: key.clone(),
                    indexed_generation: *indexed_generation,
                });
            }
        }
        diff.removed.sort();
        diff.removed.dedup();
        Ok(diff)
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
        let cleanup_debt_count = self.cleanup_debt_count(source_id).await?;
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

    pub async fn committed_generation_item_count(&self, source_id: &str) -> anyhow::Result<usize> {
        let status = self.source_status(source_id).await?;
        if status.committed_generation <= 0 {
            return Ok(0);
        }
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)
             FROM axon_source_manifest_items
             WHERE source_id = ? AND pending = 0",
        )
        .bind(source_id)
        .fetch_one(&self.pool)
        .await
        .context("failed to count committed source items")?;
        Ok(count.max(0) as usize)
    }

    async fn validate_owner_lease_active(
        &self,
        source_id: &str,
        owner: &str,
        now_ms: i64,
    ) -> anyhow::Result<()> {
        let mut tx = self
            .pool
            .begin()
            .await
            .context("failed to begin source ledger lease validation")?;
        validate_owner_lease_active_in_tx(&mut tx, source_id, owner, now_ms).await?;
        tx.commit()
            .await
            .context("failed to commit source ledger lease validation")?;
        Ok(())
    }
}

pub(crate) async fn validate_owner_lease_active_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    source_id: &str,
    owner: &str,
    now_ms: i64,
) -> anyhow::Result<()> {
    let row = sqlx::query(
        "SELECT lease_owner, lease_expires_at_ms
         FROM axon_source_sources
         WHERE source_id = ?",
    )
    .bind(source_id)
    .fetch_optional(tx.as_mut())
    .await
    .context("failed to read source ledger lease state")?
    .ok_or_else(|| anyhow::anyhow!("source ledger source {source_id} does not exist"))?;
    let lease_owner: Option<String> = row.try_get("lease_owner")?;
    if lease_owner.as_deref() != Some(owner) {
        anyhow::bail!("source ledger lease for {source_id} was lost");
    }
    let lease_expires_at_ms: i64 = row.try_get("lease_expires_at_ms")?;
    if lease_expires_at_ms <= now_ms {
        anyhow::bail!("source ledger lease for {source_id} owned by {owner} expired");
    }
    Ok(())
}

pub(crate) fn validate_cleanup_debt_item(
    source_id: &str,
    debt: &CleanupDebtItem,
) -> anyhow::Result<()> {
    let selector: serde_json::Value = serde_json::from_str(&debt.selector_json)
        .context("failed to parse source cleanup selector json")?;
    let selector_source_id = selector
        .get("source_id")
        .and_then(|value| value.as_str())
        .ok_or_else(|| anyhow::anyhow!("cleanup selector missing source_id"))?;
    if selector_source_id != source_id {
        anyhow::bail!("cleanup selector source_id does not match debt source_id");
    }
    let selector_generation = selector
        .get("source_generation")
        .and_then(|value| value.as_i64())
        .ok_or_else(|| anyhow::anyhow!("cleanup selector missing source_generation"))?;
    if selector_generation != debt.generation {
        anyhow::bail!("cleanup selector source_generation does not match debt generation");
    }
    let selector_item_key = selector
        .get("item_key")
        .or_else(|| selector.get("source_item_key"))
        .and_then(|value| value.as_str())
        .ok_or_else(|| anyhow::anyhow!("cleanup selector missing item_key"))?;
    if selector_item_key != debt.item_key {
        anyhow::bail!("cleanup selector item_key does not match debt item_key");
    }
    Ok(())
}

pub(super) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
