//! `LedgerPrune` cleanup-debt production for generations that have aged past
//! the retention window.
//!
//! Retention: keep the newly committed generation plus its immediate
//! predecessor (`LEDGER_GENERATION_RETENTION_COMMITTED`, currently 2) always.
//! An older generation is skipped for one more publish cycle while it still
//! has other unresolved (non-`LedgerPrune`) cleanup debt — vector/graph/
//! memory — referencing it, so ledger rows a pending delete still needs to
//! reason about (e.g. re-deriving its scope on retry) are not pulled out from
//! under it. See the `LEDGER_GENERATION_RETENTION_COMMITTED` doc comment in
//! `crate::lib` for the contract citation.
//!
//! Uses the durable per-source sequence so deleting an intermediate generation
//! cannot strand older debt-protected rows outside future retention walks.

use axon_api::source::*;

use crate::LEDGER_GENERATION_RETENTION_COMMITTED;
use crate::cleanup_debt::ledger_prune_debt;
use crate::migration::sqlite_error;
use crate::store::Result;

pub(super) async fn ledger_prune_cleanup_debt_in_tx(
    tx: &mut sqlx::SqliteConnection,
    source_id: &SourceId,
    previous_generation: Option<&SourceGenerationId>,
) -> Result<Vec<CleanupDebt>> {
    let Some(previous_generation) = previous_generation else {
        return Ok(Vec::new());
    };

    let previous_exists: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM source_generations
         WHERE source_id = ?1 AND generation = ?2 AND published_at IS NOT NULL",
    )
    .bind(&source_id.0)
    .bind(&previous_generation.0)
    .fetch_optional(&mut *tx)
    .await
    .map_err(sqlite_error)?;
    let Some(_) = previous_exists else {
        return Ok(Vec::new());
    };
    let retained_before_previous = LEDGER_GENERATION_RETENTION_COMMITTED.saturating_sub(1) as i64;
    let candidates: Vec<String> = sqlx::query_scalar(
        "SELECT generation FROM source_generations
         WHERE source_id = ?1 AND published_at IS NOT NULL
         ORDER BY sequence DESC
         LIMIT -1 OFFSET ?2",
    )
    .bind(&source_id.0)
    .bind(retained_before_previous)
    .fetch_all(&mut *tx)
    .await
    .map_err(sqlite_error)?;
    let mut cleanup_debt = Vec::new();
    for candidate in candidates.into_iter().map(SourceGenerationId::new) {
        if !has_unresolved_non_ledger_debt_in_tx(tx, source_id, &candidate).await? {
            cleanup_debt.push(ledger_prune_debt(source_id, &candidate));
        }
    }
    Ok(cleanup_debt)
}

/// Whether `generation` still has unresolved cleanup debt of any kind other
/// than `LedgerPrune` itself (vector/graph/memory) — the "...plus
/// active_cleanup_debt" half of the retention policy.
async fn has_unresolved_non_ledger_debt_in_tx(
    tx: &mut sqlx::SqliteConnection,
    source_id: &SourceId,
    generation: &SourceGenerationId,
) -> Result<bool> {
    let exists: Option<i64> = sqlx::query_scalar(
        r#"
        SELECT 1
        FROM cleanup_debt
        WHERE source_id = ?1
          AND generation_key = ?2
          AND completed_at IS NULL
          AND kind != 'ledger_prune'
        LIMIT 1
        "#,
    )
    .bind(&source_id.0)
    .bind(&generation.0)
    .fetch_optional(&mut *tx)
    .await
    .map_err(sqlite_error)?;
    Ok(exists.is_some())
}
