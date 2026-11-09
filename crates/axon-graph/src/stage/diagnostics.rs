//! Bounded durable completeness diagnostics for private generation graphs.
use super::*;
use axon_api::source::{LifecycleStatus, Severity, SourceWarning};

const REJECTED: &str = "graph.candidates_rejected";

impl GraphStage {
    /// Preserve one aggregate warning, never a vector of rejected payloads.
    pub async fn record_rejected_candidates(&self, count: u64) -> Result<()> {
        if count == 0 {
            return Ok(());
        }
        let _guard = self.gate.lock().await;
        if self.sealed.load(std::sync::atomic::Ordering::Acquire) {
            return Err(graph_storage_error(
                "cannot record rejected candidates after graph stage seal",
            ));
        }
        let count = i64::try_from(count)
            .map_err(|_| graph_storage_error("graph rejection count exceeds supported range"))?;
        let mut tx = self.store.pool().begin().await.map_err(storage::error)?;
        let updated = sqlx::query("UPDATE stage_stats SET rejected_candidates=rejected_candidates+? WHERE singleton=1 AND rejected_candidates<=9223372036854775807-?")
            .bind(count).bind(count).execute(&mut *tx).await.map_err(storage::error)?;
        if updated.rows_affected() != 1 {
            return Err(graph_storage_error(
                "graph rejection count overflow or missing stage statistics",
            ));
        }
        let rejected: i64 =
            sqlx::query_scalar("SELECT rejected_candidates FROM stage_stats WHERE singleton=1")
                .fetch_one(&mut *tx)
                .await
                .map_err(storage::error)?;
        let json: String =
            sqlx::query_scalar("SELECT summary_json FROM stage_stats WHERE singleton=1")
                .fetch_one(&mut *tx)
                .await
                .map_err(storage::error)?;
        let mut summary: GraphWriteResult = serde_json::from_str(&json)
            .map_err(|_| graph_storage_error("graph stage summary corrupt"))?;
        summary.warnings.retain(|warning| warning.code != REJECTED);
        summary.warnings.push(SourceWarning {
            code: REJECTED.to_string(), severity: Severity::Warning,
            message: format!("Graphing rejected {rejected} invalid parser candidate(s) before writing source {} generation {}; valid graph facts remain published but extraction is incomplete. Inspect correlated candidate validation warnings, repair the producer, then refresh this source. Repeating unchanged input will not repair the dropped facts.", self.source.0, self.generation.0),
            source_item_key: None, retryable: false,
        });
        summary.header.status = LifecycleStatus::CompletedDegraded;
        sqlx::query("UPDATE stage_stats SET summary_json=? WHERE singleton=1")
            .bind(
                serde_json::to_string(&summary).map_err(|_| {
                    graph_storage_error("graph rejection summary serialization failed")
                })?,
            )
            .execute(&mut *tx)
            .await
            .map_err(storage::error)?;
        tx.commit().await.map_err(storage::error)?;
        Ok(())
    }
}

impl GraphStage {
    pub async fn activation_summary(
        pool: &SqlitePool,
        source: &SourceId,
        generation: &SourceGenerationId,
    ) -> Result<Option<GraphWriteResult>> {
        let receipt: Option<String> = sqlx::query_scalar("SELECT summary_json FROM graph_stage_receipts WHERE source_id=? AND generation_id=? ORDER BY activated_at DESC LIMIT 1").bind(&source.0).bind(&generation.0).fetch_optional(pool).await.map_err(storage::error)?;
        receipt
            .map(|json| {
                serde_json::from_str(&json).map_err(|_| {
                    graph_storage_error(
                        "graph activation receipt corrupt; inspect generation commit state",
                    )
                })
            })
            .transpose()
    }
}
