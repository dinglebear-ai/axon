use super::*;

/// Perform the destructive actions for the selected stores. Any single SQLite
/// store selection wipes+re-migrates the whole unified DB exactly once.
pub(super) async fn execute(
    cfg: &Config,
    stores: &[String],
    sqlite_inv: &SqliteInventory,
    qdrant_inv: Option<&QdrantInventory>,
    artifact_root: &std::path::Path,
    warnings: &mut Vec<String>,
) -> Result<(ResetDeleted, ResetCreated), Box<dyn Error>> {
    let mut deleted = ResetDeleted::default();
    let mut created = ResetCreated::default();

    if wants_any_sqlite(stores) {
        deleted.sqlite_tables = sqlite_inv.table_count;
        let version = sqlite::wipe_and_remigrate(&cfg.sqlite_path).await?;
        created.sqlite_schema_version = version;
        log_info(&format!(
            "reset sqlite wiped + re-migrated path={} schema_version={version}",
            cfg.sqlite_path.display()
        ));
    }

    if wants(stores, RESET_STORE_VECTORS) {
        reset_qdrant(cfg, qdrant_inv, &mut deleted, &mut created, warnings).await?;
    }

    if wants(stores, RESET_STORE_ARTIFACTS) {
        match artifacts::purge_files(artifact_root) {
            Ok(removed) => {
                deleted.artifact_files = removed;
                log_info(&format!(
                    "reset artifacts purged files={removed} root={}",
                    artifact_root.display()
                ));
            }
            Err(e) => warnings.push(format!("failed to purge artifacts: {e}")),
        }
    }

    Ok((deleted, created))
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
    axon_vector::ops::tei::qdrant_store::clear_collection_mode_cache(&cfg.collection);
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
    let bytes = serde_json::to_vec_pretty(&receipt)?;
    let path = axon_core::artifacts::atomic_write_under(&root, &relative, &bytes).await?;
    Ok(path.display().to_string())
}
