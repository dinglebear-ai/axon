use super::*;
use sha2::{Digest, Sha256};

pub(super) fn estimate(plan: &[ResetStorePlan], sqlite_inv: &SqliteInventory) -> ResetEstimate {
    let mut estimate = ResetEstimate::default();
    let has_sqlite = plan
        .iter()
        .any(|row| SQLITE_STORES.contains(&row.store.as_str()));
    if has_sqlite {
        estimate.sqlite_rows = sqlite_inv.content_rows;
        estimate.sqlite_tables = sqlite_inv.table_count as u64;
    }
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
            _ => {}
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
    full_hash(&value.to_string())
}

pub(super) fn physical_chunk_checksum(plan: &[ResetStorePlan], chunk: &str) -> String {
    let rows: Vec<&ResetStorePlan> = plan
        .iter()
        .filter(|row| match chunk {
            "sqlite" => SQLITE_STORES.contains(&row.store.as_str()),
            RESET_STORE_VECTORS => row.store == RESET_STORE_VECTORS,
            RESET_STORE_ARTIFACTS => row.store == RESET_STORE_ARTIFACTS,
            _ => false,
        })
        .collect();
    full_hash(&serde_json::to_string(&rows).expect("reset plan rows serialize"))
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
    full_hash(input).chars().take(16).collect()
}

fn full_hash(input: &str) -> String {
    let digest = Sha256::digest(input.as_bytes());
    format!("{digest:x}")
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
            let tables = sqlite_inv
                .tables
                .iter()
                .filter(|(_, table)| {
                    table.store.as_deref() == Some(store.as_str())
                        || (store == RESET_STORE_JOBS && table.store.is_none())
                })
                .map(|(name, table)| format!("{name}={}", table.rows))
                .collect::<Vec<_>>()
                .join(",");
            plan.push(ResetStorePlan {
                store: store.clone(),
                location: sqlite_path.clone(),
                non_empty: rows > 0,
                item_count: Some(rows),
                detail: format!(
                    "would wipe + re-migrate the unified SQLite DB ({} tables; schema_version={}; \
                     schema_checksum={}; owned_tables=[{}])",
                    sqlite_inv.table_count,
                    sqlite_inv.schema_identity.version,
                    sqlite_inv.schema_identity.checksum,
                    tables,
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
