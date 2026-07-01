use crate::{CleanupDebtItem, ManifestItem, SourceLedgerStore};
use anyhow::Context;
use sqlx::{QueryBuilder, Row, Sqlite};
use std::collections::BTreeSet;

struct CommitState {
    committed_generation: i64,
    max_generation: i64,
    lease_owner: Option<String>,
}

async fn prune_removed_items_in_tx(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    source_id: &str,
    live_item_keys: &BTreeSet<String>,
) -> anyhow::Result<()> {
    if live_item_keys.is_empty() {
        sqlx::query(
            "DELETE FROM axon_source_manifest_items
             WHERE source_id = ? AND pending = 0",
        )
        .bind(source_id)
        .execute(tx.as_mut())
        .await
        .context("failed to prune removed source manifest items")?;
        return Ok(());
    }
    let mut query: QueryBuilder<'_, Sqlite> = QueryBuilder::new(
        "DELETE FROM axon_source_manifest_items
         WHERE source_id = ",
    );
    query
        .push_bind(source_id)
        .push(" AND pending = 0 AND item_key NOT IN (");
    let mut separated = query.separated(", ");
    for key in live_item_keys {
        separated.push_bind(key);
    }
    separated.push_unseparated(")");
    query
        .build()
        .execute(tx.as_mut())
        .await
        .context("failed to prune removed source manifest items")?;
    Ok(())
}

async fn prune_replaced_items_in_tx(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    source_id: &str,
    generation: i64,
    changed_manifest: &[ManifestItem],
) -> anyhow::Result<()> {
    if changed_manifest.is_empty() {
        return Ok(());
    }
    let mut query: QueryBuilder<'_, Sqlite> = QueryBuilder::new(
        "DELETE FROM axon_source_manifest_items
         WHERE source_id = ",
    );
    query
        .push_bind(source_id)
        .push(" AND pending = 0 AND indexed_generation < ")
        .push_bind(generation)
        .push(" AND item_key IN (");
    let mut separated = query.separated(", ");
    for item in changed_manifest {
        separated.push_bind(&item.item_key);
    }
    separated.push_unseparated(")");
    query
        .build()
        .execute(tx.as_mut())
        .await
        .context("failed to prune replaced source manifest items")?;
    Ok(())
}

async fn commit_pending_generation_items_in_tx(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    source_id: &str,
    generation: i64,
    now_ms: i64,
) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE axon_source_manifest_items
         SET pending = 0, updated_at_ms = ?
         WHERE source_id = ? AND indexed_generation = ?",
    )
    .bind(now_ms)
    .bind(source_id)
    .bind(generation)
    .execute(tx.as_mut())
    .await
    .context("failed to commit source manifest items")?;
    Ok(())
}

impl SourceLedgerStore {
    pub async fn commit_generation(&self, source_id: &str, generation: i64) -> anyhow::Result<()> {
        self.commit_generation_inner(source_id, generation, None)
            .await
    }

    pub async fn commit_generation_for_owner(
        &self,
        source_id: &str,
        generation: i64,
        owner: &str,
    ) -> anyhow::Result<()> {
        self.commit_generation_inner(source_id, generation, Some(owner))
            .await
    }

    pub async fn commit_generation_payload_for_owner(
        &self,
        source_id: &str,
        generation: i64,
        owner: &str,
        manifest: &[ManifestItem],
        cleanup_debt: &[CleanupDebtItem],
    ) -> anyhow::Result<()> {
        let now_ms = super::now_ms();
        let mut tx = self
            .pool
            .begin()
            .await
            .context("failed to begin source generation payload commit")?;
        for item in manifest {
            insert_pending_manifest_item(&mut tx, source_id, generation, item, now_ms).await?;
        }
        for debt in cleanup_debt {
            insert_cleanup_debt(&mut tx, source_id, debt, now_ms).await?;
        }
        self.commit_generation_in_tx(&mut tx, source_id, generation, Some(owner), now_ms)
            .await?;
        tx.commit()
            .await
            .context("failed to commit source generation payload transaction")?;
        Ok(())
    }

    pub async fn commit_generation_delta_for_owner(
        &self,
        source_id: &str,
        generation: i64,
        owner: &str,
        changed_manifest: &[ManifestItem],
        live_item_keys: &BTreeSet<String>,
        cleanup_debt: &[CleanupDebtItem],
    ) -> anyhow::Result<()> {
        let now_ms = super::now_ms();
        let mut tx = self
            .pool
            .begin()
            .await
            .context("failed to begin source generation delta commit")?;
        for item in changed_manifest {
            insert_pending_manifest_item(&mut tx, source_id, generation, item, now_ms).await?;
        }
        for debt in cleanup_debt {
            insert_cleanup_debt(&mut tx, source_id, debt, now_ms).await?;
        }
        let state = self
            .validate_generation_commit_in_tx(&mut tx, source_id, generation, Some(owner))
            .await?;
        ensure_generation_has_manifest_state_in_tx(
            &mut tx,
            source_id,
            generation,
            state.committed_generation,
        )
        .await?;
        prune_removed_items_in_tx(&mut tx, source_id, live_item_keys).await?;
        prune_replaced_items_in_tx(&mut tx, source_id, generation, changed_manifest).await?;
        commit_pending_generation_items_in_tx(&mut tx, source_id, generation, now_ms).await?;
        self.commit_generation_metadata_in_tx(&mut tx, source_id, generation, Some(owner), now_ms)
            .await?;
        tx.commit()
            .await
            .context("failed to commit source generation delta transaction")?;
        Ok(())
    }

    pub async fn abort_generation_for_owner(
        &self,
        source_id: &str,
        generation: i64,
        owner: &str,
    ) -> anyhow::Result<()> {
        let now_ms = super::now_ms();
        let mut tx = self
            .pool
            .begin()
            .await
            .context("failed to begin source generation abort")?;
        sqlx::query(
            "DELETE FROM axon_source_manifest_items
             WHERE source_id = ? AND indexed_generation = ? AND pending = 1",
        )
        .bind(source_id)
        .bind(generation)
        .execute(&mut *tx)
        .await
        .context("failed to delete aborted source manifest items")?;
        let result = sqlx::query(
            "UPDATE axon_source_sources
             SET max_generation = committed_generation, updated_at_ms = ?
             WHERE source_id = ? AND lease_owner = ? AND max_generation = ?",
        )
        .bind(now_ms)
        .bind(source_id)
        .bind(owner)
        .bind(generation)
        .execute(&mut *tx)
        .await
        .context("failed to abort source generation")?;
        if result.rows_affected() != 1 {
            anyhow::bail!("source ledger lease for {source_id} was lost before abort");
        }
        tx.commit()
            .await
            .context("failed to commit source generation abort")?;
        Ok(())
    }

    async fn commit_generation_inner(
        &self,
        source_id: &str,
        generation: i64,
        owner: Option<&str>,
    ) -> anyhow::Result<()> {
        let now_ms = super::now_ms();
        let mut tx = self
            .pool
            .begin()
            .await
            .context("failed to begin source generation commit")?;
        self.commit_generation_in_tx(&mut tx, source_id, generation, owner, now_ms)
            .await?;
        tx.commit()
            .await
            .context("failed to commit source generation transaction")?;
        Ok(())
    }

    async fn commit_generation_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, Sqlite>,
        source_id: &str,
        generation: i64,
        owner: Option<&str>,
        now_ms: i64,
    ) -> anyhow::Result<()> {
        let state = self
            .validate_generation_commit_in_tx(tx, source_id, generation, owner)
            .await?;
        ensure_generation_has_manifest_state_in_tx(
            tx,
            source_id,
            generation,
            state.committed_generation,
        )
        .await?;
        sqlx::query(
            "DELETE FROM axon_source_manifest_items
             WHERE source_id = ? AND pending = 0 AND indexed_generation < ?",
        )
        .bind(source_id)
        .bind(generation)
        .execute(tx.as_mut())
        .await
        .context("failed to prune old source manifest items")?;
        commit_pending_generation_items_in_tx(tx, source_id, generation, now_ms).await?;
        self.commit_generation_metadata_in_tx(tx, source_id, generation, owner, now_ms)
            .await
    }

    async fn validate_generation_commit_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, Sqlite>,
        source_id: &str,
        generation: i64,
        owner: Option<&str>,
    ) -> anyhow::Result<CommitState> {
        let row = sqlx::query(
            "SELECT committed_generation, max_generation, lease_owner
             FROM axon_source_sources
             WHERE source_id = ?",
        )
        .bind(source_id)
        .fetch_optional(tx.as_mut())
        .await
        .context("failed to read source generation state before commit")?
        .ok_or_else(|| anyhow::anyhow!("source ledger source {source_id} does not exist"))?;

        let state = CommitState {
            committed_generation: row.try_get("committed_generation")?,
            max_generation: row.try_get("max_generation")?,
            lease_owner: row.try_get("lease_owner")?,
        };

        if let Some(owner) = owner {
            if state.lease_owner.as_deref() != Some(owner) {
                anyhow::bail!("source ledger lease for {source_id} was lost before commit");
            }
        }

        let allows_implicit_generation = state.max_generation == state.committed_generation
            && generation == state.committed_generation.saturating_add(1);
        if state.max_generation != generation && !allows_implicit_generation {
            anyhow::bail!(
                "source ledger generation {generation} is stale for {source_id}; active generation is {}",
                state.max_generation
            );
        }

        Ok(state)
    }

    async fn commit_generation_metadata_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, Sqlite>,
        source_id: &str,
        generation: i64,
        owner: Option<&str>,
        now_ms: i64,
    ) -> anyhow::Result<()> {
        let query = sqlx::query(
            "UPDATE axon_source_sources
             SET committed_generation = MAX(committed_generation, ?),
                 max_generation = MAX(max_generation, ?),
                 last_success_at_ms = ?,
                 last_error = NULL,
                 backoff_until_ms = 0,
                 backoff_dependency = NULL,
                 updated_at_ms = ?
             WHERE source_id = ?
               AND (max_generation = ? OR (max_generation = committed_generation AND committed_generation = ?))
               AND (? IS NULL OR lease_owner = ?)",
        )
        .bind(generation)
        .bind(generation)
        .bind(now_ms)
        .bind(now_ms)
        .bind(source_id)
        .bind(generation)
        .bind(generation.saturating_sub(1))
        .bind(owner)
        .bind(owner);
        let result = query
            .execute(tx.as_mut())
            .await
            .context("failed to commit source generation")?;
        if result.rows_affected() != 1 {
            anyhow::bail!("source ledger lease for {source_id} was lost before commit");
        }
        Ok(())
    }
}

async fn ensure_generation_has_manifest_state_in_tx(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    source_id: &str,
    generation: i64,
    committed_generation: i64,
) -> anyhow::Result<()> {
    if committed_generation != 0 {
        return Ok(());
    }
    let pending_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM axon_source_manifest_items
         WHERE source_id = ? AND indexed_generation = ? AND pending = 1",
    )
    .bind(source_id)
    .bind(generation)
    .fetch_one(tx.as_mut())
    .await
    .context("failed to verify first source generation manifest state")?;
    if pending_count == 0 {
        anyhow::bail!(
            "source ledger first generation {generation} for {source_id} cannot commit without manifest state"
        );
    }
    Ok(())
}

async fn insert_pending_manifest_item(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    source_id: &str,
    generation: i64,
    item: &ManifestItem,
    now_ms: i64,
) -> anyhow::Result<()> {
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
    .bind(&item.item_key)
    .bind(&item.content_hash)
    .bind(item.size_bytes)
    .bind(generation)
    .bind(now_ms)
    .execute(&mut **tx)
    .await
    .context("failed to record source manifest item")?;
    Ok(())
}

async fn insert_cleanup_debt(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    source_id: &str,
    debt: &CleanupDebtItem,
    now_ms: i64,
) -> anyhow::Result<()> {
    super::validate_cleanup_debt_item(source_id, debt)?;
    sqlx::query(
        "INSERT INTO axon_source_cleanup_debt (
            source_id, generation, item_key, selector_json, retry_count, last_error, updated_at_ms
         ) VALUES (?, ?, ?, ?, 0, NULL, ?)
         ON CONFLICT(source_id, generation, item_key) DO UPDATE SET
            selector_json = excluded.selector_json,
            updated_at_ms = excluded.updated_at_ms",
    )
    .bind(source_id)
    .bind(debt.generation)
    .bind(&debt.item_key)
    .bind(&debt.selector_json)
    .bind(now_ms)
    .execute(&mut **tx)
    .await
    .context("failed to record source cleanup debt")?;
    Ok(())
}
