use anyhow::Context;
use sqlx::Row;

use crate::{CleanupDebtItem, SourceLedgerStore};

impl SourceLedgerStore {
    pub async fn record_cleanup_debt(
        &self,
        source_id: &str,
        generation: i64,
        item_key: &str,
        selector_json: &str,
    ) -> anyhow::Result<()> {
        let now_ms = super::now_ms();
        sqlx::query(
            "INSERT INTO axon_source_cleanup_debt (
                source_id, generation, item_key, selector_json, retry_count, last_error, updated_at_ms
             ) VALUES (?, ?, ?, ?, 0, NULL, ?)
             ON CONFLICT(source_id, generation, item_key) DO UPDATE SET
                selector_json = excluded.selector_json,
                updated_at_ms = excluded.updated_at_ms",
        )
        .bind(source_id)
        .bind(generation)
        .bind(item_key)
        .bind(selector_json)
        .bind(now_ms)
        .execute(&self.pool)
        .await
        .context("failed to record source cleanup debt")?;
        Ok(())
    }

    pub async fn cleanup_debt_count(&self, source_id: &str) -> anyhow::Result<i64> {
        sqlx::query_scalar("SELECT COUNT(*) FROM axon_source_cleanup_debt WHERE source_id = ?")
            .bind(source_id)
            .fetch_one(&self.pool)
            .await
            .context("failed to count source cleanup debt")
    }

    pub async fn cleanup_debt_items(
        &self,
        source_id: &str,
    ) -> anyhow::Result<Vec<CleanupDebtItem>> {
        let rows = sqlx::query(
            "SELECT generation, item_key, selector_json
             FROM axon_source_cleanup_debt
             WHERE source_id = ?
             ORDER BY generation, item_key",
        )
        .bind(source_id)
        .fetch_all(&self.pool)
        .await
        .context("failed to list source cleanup debt")?;
        rows.into_iter()
            .map(|row| {
                Ok(CleanupDebtItem {
                    generation: row.try_get("generation")?,
                    item_key: row.try_get("item_key")?,
                    selector_json: row.try_get("selector_json")?,
                })
            })
            .collect::<Result<_, sqlx::Error>>()
            .context("failed to decode source cleanup debt")
    }

    pub async fn clear_cleanup_debt_item(
        &self,
        source_id: &str,
        generation: i64,
        item_key: &str,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "DELETE FROM axon_source_cleanup_debt
             WHERE source_id = ? AND generation = ? AND item_key = ?",
        )
        .bind(source_id)
        .bind(generation)
        .bind(item_key)
        .execute(&self.pool)
        .await
        .context("failed to clear source cleanup debt")?;
        Ok(())
    }
}
