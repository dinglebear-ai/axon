use super::*;
use sha2::{Digest, Sha256};

pub(super) fn estimate(plan: &[ResetStorePlan]) -> ResetEstimate {
    let mut estimate = ResetEstimate::default();
    for row in plan {
        let count = row.item_count.unwrap_or(0);
        match row.store.as_str() {
            RESET_STORE_VECTORS => {
                estimate.qdrant_points = estimate.qdrant_points.saturating_add(count);
                estimate.qdrant_collections += u64::from(count > 0);
            }
            RESET_STORE_ARTIFACTS => {
                estimate.artifact_files = estimate.artifact_files.saturating_add(count);
            }
            _ => {
                estimate.sqlite_rows = estimate.sqlite_rows.saturating_add(count);
                estimate.sqlite_tables += u64::from(row.non_empty || count > 0);
            }
        }
    }
    estimate
}

pub(super) fn inventory_checksum(
    stores: &[String],
    plan: &[ResetStorePlan],
    estimates: &ResetEstimate,
) -> String {
    let value = serde_json::json!({
        "stores": stores,
        "plan": plan,
        "estimates": estimates,
    });
    short_hash(&value.to_string())
}

pub(super) fn config_snapshot_id(cfg: &Config) -> String {
    short_hash(&format!(
        "sqlite={};qdrant={};collection={}",
        cfg.sqlite_path.display(),
        cfg.qdrant_url,
        cfg.collection
    ))
}

fn short_hash(input: &str) -> String {
    let digest = Sha256::digest(input.as_bytes());
    format!("{digest:x}").chars().take(16).collect()
}

pub(super) fn build_plan(
    cfg: &Config,
    stores: &[String],
    sqlite_inv: &SqliteInventory,
    qdrant_inv: Option<&QdrantInventory>,
    artifact_root: &std::path::Path,
    artifact_files: Option<usize>,
) -> Vec<ResetStorePlan> {
    let mut plan = Vec::new();
    let sqlite_path = cfg.sqlite_path.display().to_string();

    for store in stores {
        if SQLITE_STORES.contains(&store.as_str()) {
            let rows = sqlite_inv.rows_by_store.get(store).copied().unwrap_or(0);
            plan.push(ResetStorePlan {
                store: store.clone(),
                location: sqlite_path.clone(),
                non_empty: rows > 0,
                item_count: Some(rows),
                detail: format!(
                    "would wipe + re-migrate the unified SQLite DB ({} tables)",
                    sqlite_inv.table_count
                ),
            });
        } else if store == RESET_STORE_VECTORS {
            plan.push(vector_plan(cfg, store, qdrant_inv));
        } else if store == RESET_STORE_ARTIFACTS {
            let files = artifact_files.unwrap_or(0);
            plan.push(ResetStorePlan {
                store: store.clone(),
                location: artifact_root.display().to_string(),
                non_empty: files > 0,
                item_count: Some(files as u64),
                detail: format!("would delete {files} artifact file(s) under the artifact root"),
            });
        }
    }
    plan
}

fn vector_plan(cfg: &Config, store: &str, qdrant_inv: Option<&QdrantInventory>) -> ResetStorePlan {
    let inv = qdrant_inv.cloned().unwrap_or_default();
    let detail = if inv.unreachable {
        "Qdrant unreachable — collection could not be inventoried".to_string()
    } else if inv.exists {
        let contracts = if inv.payload_contract_versions.is_empty() {
            "<none>".to_string()
        } else {
            inv.payload_contract_versions.join(",")
        };
        format!(
            "would drop + recreate collection '{}' ({} points, payload contracts: {})",
            cfg.collection, inv.points, contracts
        )
    } else {
        format!(
            "collection '{}' does not exist — nothing to drop",
            cfg.collection
        )
    };
    ResetStorePlan {
        store: store.to_string(),
        location: format!("{}#{}", cfg.qdrant_url, cfg.collection),
        non_empty: inv.non_empty(),
        item_count: (!inv.unreachable).then_some(inv.points),
        detail,
    }
}
