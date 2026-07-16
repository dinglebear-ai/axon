use super::*;
#[cfg(test)]
use axon_core::redact::{DefaultRedactor, RedactionContext, Redactor};

pub(super) async fn execute_resumable(
    cfg: &Config,
    stores: &[String],
    sqlite_inv: &SqliteInventory,
    qdrant_inv: Option<&QdrantInventory>,
    artifact_root: &std::path::Path,
    warnings: &mut Vec<String>,
    plan: &ResetPlan,
    existing: Option<ResetReceipt>,
) -> Result<(ResetReceipt, String), Box<dyn Error>> {
    let mut receipt = existing.unwrap_or_else(|| ResetReceipt {
        plan_id: plan.plan_id.clone(),
        reset_id: plan.reset_id.clone(),
        state: ResetExecutionState::Executing,
        chunks: planned_chunks(stores),
        deleted: ResetDeleted::default(),
        created: ResetCreated::default(),
        audit_events: vec![
            "reset.plan".to_string(),
            "reset.confirm".to_string(),
            "reset.execute".to_string(),
        ],
    });
    if receipt.plan_id != plan.plan_id {
        return Err("reset.receipt_plan_mismatch: refusing to resume another plan".into());
    }
    if receipt.state != ResetExecutionState::Completed {
        receipt.state = ResetExecutionState::Executing;
        if receipt
            .chunks
            .iter()
            .any(|chunk| chunk.status == "completed")
        {
            receipt.audit_events.push("reset.resume".to_string());
        }
    }
    plan_store::save_receipt(cfg, &receipt).await?;

    for index in 0..receipt.chunks.len() {
        if receipt.chunks[index].status == "completed" {
            continue;
        }
        let store = receipt.chunks[index].store.clone();
        let outcome = execute_physical_chunk(
            cfg,
            &store,
            sqlite_inv,
            qdrant_inv,
            artifact_root,
            warnings,
            &mut receipt.deleted,
            &mut receipt.created,
        )
        .await
        .map_err(|error| error.to_string());
        match outcome {
            Ok((count, checkpoint)) => {
                receipt.chunks[index].status = "completed".to_string();
                receipt.chunks[index].item_count = count;
                receipt.chunks[index].checkpoint = checkpoint;
                receipt
                    .audit_events
                    .push(format!("reset.chunk.complete:{store}"));
                plan_store::save_receipt(cfg, &receipt).await?;
            }
            Err(error) => {
                receipt.chunks[index].status = "failed".to_string();
                receipt.chunks[index].checkpoint = error.clone();
                receipt.state = ResetExecutionState::Failed;
                receipt
                    .audit_events
                    .push(format!("reset.chunk.failed:{store}"));
                let _ = plan_store::save_receipt(cfg, &receipt).await;
                return Err(error.into());
            }
        }
    }
    receipt.state = if warnings.is_empty() {
        ResetExecutionState::Completed
    } else {
        ResetExecutionState::CompletedDegraded
    };
    receipt.audit_events.push("reset.complete".to_string());
    let path = plan_store::save_receipt(cfg, &receipt).await?;
    Ok((receipt, path))
}

fn planned_chunks(stores: &[String]) -> Vec<ResetChunkReceipt> {
    let mut chunks = Vec::new();
    if wants_any_sqlite(stores) {
        chunks.push(pending_chunk("chunk_0000", "sqlite"));
    }
    if wants(stores, RESET_STORE_VECTORS) {
        chunks.push(pending_chunk("chunk_0001", RESET_STORE_VECTORS));
    }
    if wants(stores, RESET_STORE_ARTIFACTS) {
        chunks.push(pending_chunk("chunk_0002", RESET_STORE_ARTIFACTS));
    }
    chunks
}

fn pending_chunk(id: &str, store: &str) -> ResetChunkReceipt {
    ResetChunkReceipt {
        chunk_id: id.to_string(),
        store: store.to_string(),
        status: "pending".to_string(),
        item_count: 0,
        checkpoint: "not_started".to_string(),
    }
}

#[allow(clippy::too_many_arguments)]
async fn execute_physical_chunk(
    cfg: &Config,
    store: &str,
    sqlite_inv: &SqliteInventory,
    qdrant_inv: Option<&QdrantInventory>,
    artifact_root: &std::path::Path,
    warnings: &mut Vec<String>,
    deleted: &mut ResetDeleted,
    created: &mut ResetCreated,
) -> Result<(u64, String), Box<dyn Error>> {
    match store {
        "sqlite" => {
            deleted.sqlite_tables = sqlite_inv.table_count;
            created.sqlite_schema_version = sqlite::wipe_and_remigrate(&cfg.sqlite_path).await?;
            Ok((
                sqlite_inv.content_rows,
                format!("schema_version={}", created.sqlite_schema_version),
            ))
        }
        RESET_STORE_VECTORS => {
            let before = qdrant_inv.cloned().unwrap_or_default();
            if before.unreachable {
                return Err("reset.vectors_unreachable: refusing partial clean slate".into());
            }
            reset_qdrant(cfg, qdrant_inv, deleted, created, warnings).await?;
            Ok((
                before.points,
                format!("created={}", created.qdrant_collections.join(",")),
            ))
        }
        RESET_STORE_ARTIFACTS => {
            let removed = artifacts::purge_files(artifact_root)?;
            deleted.artifact_files = removed;
            Ok((removed as u64, "complete".to_string()))
        }
        _ => Err(format!("reset.unknown_chunk_store: {store}").into()),
    }
}

async fn reset_qdrant(
    cfg: &Config,
    qdrant_inv: Option<&QdrantInventory>,
    deleted: &mut ResetDeleted,
    created: &mut ResetCreated,
    warnings: &mut Vec<String>,
) -> Result<(), Box<dyn Error>> {
    let inv = qdrant_inv.cloned().unwrap_or_default();
    if inv.unreachable {
        warnings.push(format!(
            "Qdrant unreachable — collection '{}' not reset",
            cfg.collection
        ));
        return Ok(());
    }

    let dropped = qdrant::drop_collection(cfg).await?;
    if dropped {
        deleted.qdrant_collections.push(cfg.collection.clone());
    }
    // No process-wide vector-mode cache to invalidate here: `axon-vectors`
    // (the replacement for the legacy `axon-vector` crate) resolves a
    // collection's vector mode per request rather than caching it in a
    // process-wide static, so there is nothing stale left behind by the
    // drop+recreate above.
    match qdrant::probe_tei_dim(cfg).await {
        Some(dim) => {
            qdrant::create_named_collection(cfg, dim).await?;
            created.qdrant_collections.push(cfg.collection.clone());
            log_info(&format!(
                "reset qdrant recreated collection='{}' dim={dim}",
                cfg.collection
            ));
        }
        None => {
            warnings.push(format!(
                "TEI embedding dimension unavailable — collection '{}' dropped but not recreated; \
                 it will be created lazily on the first embed",
                cfg.collection
            ));
            log_warn(&format!(
                "reset qdrant dropped but not recreated (no TEI dim) collection='{}'",
                cfg.collection
            ));
        }
    }
    Ok(())
}

/// Write the reset receipt as a JSON artifact under the artifact root. Returns
/// its filesystem path. The receipt is the durable record of a destructive
/// reset required by the cutover contract.
#[cfg(test)]
pub(super) async fn write_receipt(
    reset_id: &str,
    stores: &[String],
    plan: &[ResetStorePlan],
    reset_plan: &ResetPlan,
    receipt: &ResetReceipt,
    warnings: &[String],
) -> Result<String, Box<dyn Error>> {
    let root = artifacts::artifact_root();
    let relative = format!("reset/{reset_id}.json");
    let receipt = serde_json::json!({
        "reset_id": reset_id,
        "written_at_utc": chrono::Utc::now().to_rfc3339(),
        "stores": stores,
        "dry_run": false,
        "reset_plan": reset_plan,
        "plan": plan,
        "receipt": receipt,
        "deleted": receipt.deleted,
        "created": receipt.created,
        "warnings": warnings,
    });
    // Fail-closed redaction boundary: this receipt is a durable audit-trail
    // artifact returned to callers verbatim; a warning/plan entry carrying a
    // provider error body or path fragment must not persist a secret.
    let (receipt, _redaction_report) =
        DefaultRedactor::new().redact_json(receipt, &RedactionContext::artifact_metadata());
    let bytes = serde_json::to_vec_pretty(&receipt)?;
    let path = axon_core::artifacts::atomic_write_under(&root, &relative, &bytes).await?;
    Ok(path.display().to_string())
}
