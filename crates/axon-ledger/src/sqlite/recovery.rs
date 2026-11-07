//! Lease-fenced reconciliation of source writes abandoned before publication.
use crate::cleanup_debt::generation_vector_delete_debt;
use crate::migration::sqlite_error;
use crate::sqlite::util::{json_error, timestamp_str_after};
use crate::sqlite::{SqliteLedgerStore, cleanup, generation};
use crate::store::Result;
use axon_api::source::*;
use axon_core::sqlite::ImmediateTx;

pub(super) async fn recover(
    store: &SqliteLedgerStore,
    current: SourceGeneration,
    lease: LeaseGuard,
    collection: String,
) -> Result<u64> {
    let mut tx = ImmediateTx::begin_with_gate(&store.pool, &store.write_gate)
        .await
        .map_err(sqlite_error)?;
    let acquired: Option<String>=sqlx::query_scalar("SELECT acquired_at FROM leases WHERE lease_id=?1 AND lease_key=?2 AND owner_id=?3 AND julianday(expires_at)>julianday('now')")
        .bind(&lease.lease_id.0).bind(format!("source:{}",current.source_id.0)).bind(&lease.owner_id)
        .fetch_optional(&mut *tx).await.map_err(sqlite_error)?;
    if acquired.is_none() || collection.trim().is_empty() {
        return Err(ApiError::new("source.ledger.recovery_lease_required",ErrorStage::Cleaning,
            "abandoned generation recovery requires this source's live lease and explicit vector collection; reacquire the source lease before retrying")
            .with_source_id(current.source_id.0));
    }
    let acquired = acquired.unwrap();
    let sequence:Option<i64>=sqlx::query_scalar("SELECT sequence FROM source_generations WHERE source_id=?1 AND generation=?2 AND published_at IS NULL AND publish_state='writing' AND status='running'")
        .bind(&current.source_id.0).bind(&current.generation.0).fetch_optional(&mut *tx).await.map_err(sqlite_error)?;
    let Some(sequence) = sequence else {
        return Err(ApiError::new("source.ledger.recovery_generation_changed",ErrorStage::Cleaning,
            "active recovery generation is no longer running and unpublished; inspect its state before retrying").with_source_id(current.source_id.0));
    };
    // A persisted writer manifest distinguishes Axon writes from imported
    // generation collisions and records the collection that must be cleaned.
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT g.generation_json, json_extract(m.manifest_json, '$.metadata.\"axon.vector_write_collection\"')
         FROM source_generations g JOIN source_manifests m
         ON m.source_id=g.source_id AND m.generation=g.generation
         WHERE g.source_id=?1 AND g.sequence<?2 AND g.published_at IS NULL
         AND g.publish_state='writing' AND g.status IN ('running','completed','failed')
         AND typeof(json_extract(m.manifest_json, '$.metadata.\"axon.vector_write_collection\"'))='text'
         AND length(trim(json_extract(m.manifest_json, '$.metadata.\"axon.vector_write_collection\"')))>0
         AND NOT EXISTS (SELECT 1 FROM cleanup_debt d WHERE d.source_id=g.source_id
             AND d.generation_key=g.generation AND d.kind='vector_delete')
         ORDER BY g.sequence LIMIT 64")
        .bind(&current.source_id.0).bind(sequence).fetch_all(&mut *tx).await.map_err(sqlite_error)?;
    let mut recovered = 0;
    for (row, write_collection) in rows {
        let mut old: SourceGeneration = serde_json::from_str(&row).map_err(json_error)?;
        // A generation created by this same lease holder can still be in use.
        if !timestamp_str_after(&acquired, &old.created_at.0)? {
            continue;
        }
        old.status = LifecycleStatus::Failed;
        let mut debt = generation_vector_delete_debt(&old.source_id, &old.generation);
        debt.vector_collection = Some(write_collection);
        debt.job_id = lease.job_id.unwrap_or(JobId::new(uuid::Uuid::nil()));
        old.cleanup_debt.push(debt.debt_id.clone());
        generation::upsert_generation_in_tx(&mut tx, &old, None).await?;
        cleanup::insert_cleanup_debt_once_in_tx(&mut tx, debt).await?;
        recovered += 1;
    }
    tx.commit().await.map_err(sqlite_error)?;
    Ok(recovered)
}
