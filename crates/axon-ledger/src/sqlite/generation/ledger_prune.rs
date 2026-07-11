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
//! Walks the generation chain backward via each row's own
//! `previous_generation` pointer (the only linkage — there is no forward
//! index), stopping as soon as a row is missing (already ledger-pruned, or
//! never existed): nothing older than a missing row can still be present.

use axon_api::source::*;

use crate::LEDGER_GENERATION_RETENTION_COMMITTED;
use crate::migration::sqlite_error;
use crate::sqlite::util::{json_error, timestamp};
use crate::store::Result;

pub(super) async fn ledger_prune_cleanup_debt_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    source_id: &SourceId,
    previous_generation: Option<&SourceGenerationId>,
) -> Result<Vec<CleanupDebt>> {
    let Some(previous_generation) = previous_generation else {
        return Ok(Vec::new());
    };

    // The just-published generation is retained slot 1; `previous_generation`
    // is retained slot 2. Step back the rest of the retention window to reach
    // the first prune candidate.
    let mut cursor = Some(previous_generation.clone());
    for _ in 0..LEDGER_GENERATION_RETENTION_COMMITTED.saturating_sub(1) {
        let Some(current) = cursor else {
            return Ok(Vec::new());
        };
        cursor = fetch_generation_in_tx(tx, source_id, &current)
            .await?
            .and_then(|generation| generation.previous_generation);
    }

    let mut cleanup_debt = Vec::new();
    while let Some(candidate) = cursor {
        let Some(candidate_generation) = fetch_generation_in_tx(tx, source_id, &candidate).await?
        else {
            break; // already pruned — nothing older left to consider
        };
        if !has_unresolved_non_ledger_debt_in_tx(tx, source_id, &candidate).await? {
            cleanup_debt.push(ledger_prune_debt(source_id, &candidate));
        }
        cursor = candidate_generation.previous_generation;
    }
    Ok(cleanup_debt)
}

/// The full generation row for `(source_id, generation)`, or `None` if it no
/// longer exists (already ledger-pruned, or never existed).
async fn fetch_generation_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    source_id: &SourceId,
    generation: &SourceGenerationId,
) -> Result<Option<SourceGeneration>> {
    let generation_json: Option<String> = sqlx::query_scalar(
        r#"
        SELECT generation_json
        FROM source_generations
        WHERE source_id = ?1 AND generation = ?2
        "#,
    )
    .bind(&source_id.0)
    .bind(&generation.0)
    .fetch_optional(&mut **tx)
    .await
    .map_err(sqlite_error)?;
    generation_json
        .map(|json| serde_json::from_str(&json).map_err(json_error))
        .transpose()
}

/// Whether `generation` still has unresolved cleanup debt of any kind other
/// than `LedgerPrune` itself (vector/graph/memory) — the "...plus
/// active_cleanup_debt" half of the retention policy.
async fn has_unresolved_non_ledger_debt_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
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
    .fetch_optional(&mut **tx)
    .await
    .map_err(sqlite_error)?;
    Ok(exists.is_some())
}

fn ledger_prune_debt(source_id: &SourceId, generation: &SourceGenerationId) -> CleanupDebt {
    CleanupDebt {
        debt_id: CleanupDebtId::new(format!(
            "debt_{}",
            uuid::Uuid::new_v5(
                &uuid::Uuid::NAMESPACE_URL,
                format!("ledger:{}:{}", source_id.0, generation.0).as_bytes(),
            )
        )),
        job_id: JobId::new(uuid::Uuid::from_u128(0)),
        source_id: source_id.clone(),
        generation: Some(generation.clone()),
        kind: CleanupDebtKind::LedgerPrune,
        selector: CleanupSelector::LedgerGenerations {
            source_id: source_id.clone(),
            up_to_generation: generation.clone(),
        },
        status: LifecycleStatus::Pending,
        created_at: timestamp(),
        attempts: 0,
        last_error: None,
        next_retry_at: None,
        completed_at: None,
    }
}
