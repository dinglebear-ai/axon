//! `axon reset` service — intentional clean-slate destruction of local stores
//! for the pipeline-unification empty-DB cutover.
//!
//! This is NOT migration. Reset enumerates the configured stores (SQLite jobs
//! DB + ledger/graph/memory tables, the Qdrant collection, and the artifact
//! root), and — only under an explicit `--yes` — wipes them and recreates fresh
//! schema, writing a receipt of what was reset.
//!
//! The DEFAULT is a dry-run: it prints the exact plan (stores, paths,
//! collections, row/point/file counts) and mutates nothing. See
//! `docs/pipeline-unification/delivery/cutover-contract.md` ("Required Reset
//! Tooling", reset result shape).

mod artifacts;
mod qdrant;
mod sqlite;

pub use qdrant::QdrantInventory;
pub use sqlite::SqliteInventory;

use axon_api::reset::{
    RESET_ALL_STORES, RESET_STORE_ARTIFACTS, RESET_STORE_GRAPH, RESET_STORE_JOBS,
    RESET_STORE_LEDGER, RESET_STORE_MEMORY, RESET_STORE_VECTORS, ResetCreated, ResetDeleted,
    ResetResult, ResetStorePlan,
};
use axon_core::config::Config;
use axon_core::logging::{log_info, log_warn};
use std::error::Error;
use uuid::Uuid;

/// The SQLite-backed stores that all live in the single unified DB. Selecting
/// any of them means the DB is wiped + re-migrated (the composed migration set
/// recreates every store's tables), so they share one destructive action.
const SQLITE_STORES: &[&str] = &[
    RESET_STORE_JOBS,
    RESET_STORE_LEDGER,
    RESET_STORE_GRAPH,
    RESET_STORE_MEMORY,
];

/// Resolve the selected stores from config, defaulting to all. Unknown store
/// names are rejected with a clear error listing the valid set.
fn resolve_stores(cfg: &Config) -> Result<Vec<String>, Box<dyn Error>> {
    if cfg.reset_stores.is_empty() {
        return Ok(RESET_ALL_STORES.iter().map(|s| s.to_string()).collect());
    }
    let mut out = Vec::new();
    for requested in &cfg.reset_stores {
        let store = requested.trim();
        if !RESET_ALL_STORES.contains(&store) {
            return Err(format!(
                "unknown reset store {store:?}; valid stores: {}",
                RESET_ALL_STORES.join(", ")
            )
            .into());
        }
        if !out.iter().any(|s: &String| s == store) {
            out.push(store.to_string());
        }
    }
    // Keep canonical order regardless of the order the user passed them in.
    Ok(RESET_ALL_STORES
        .iter()
        .filter(|s| out.iter().any(|o| o == *s))
        .map(|s| s.to_string())
        .collect())
}

fn wants(stores: &[String], store: &str) -> bool {
    stores.iter().any(|s| s == store)
}

fn wants_any_sqlite(stores: &[String]) -> bool {
    SQLITE_STORES.iter().any(|s| wants(stores, s))
}

/// Whether reset should actually mutate. Dry-run is the default: destruction
/// requires `--yes` AND the absence of an explicit `--dry-run` pin.
fn is_dry_run(cfg: &Config) -> bool {
    cfg.reset_dry_run || !cfg.yes
}

/// Run `axon reset`. In dry-run (default) it inventories every selected store
/// and returns the plan without mutating. Under `--yes` it wipes + recreates
/// the selected stores and writes a receipt artifact.
pub async fn reset(cfg: &Config) -> Result<ResetResult, Box<dyn Error>> {
    let stores = resolve_stores(cfg)?;
    let dry_run = is_dry_run(cfg);
    let reset_id = format!("reset_{}", Uuid::new_v4().simple());

    log_info(&format!(
        "command=reset id={reset_id} dry_run={dry_run} stores={}",
        stores.join(",")
    ));

    let sqlite_inv = sqlite::inventory(&cfg.sqlite_path).await?;
    let qdrant_inv = if wants(&stores, RESET_STORE_VECTORS) {
        Some(qdrant::inventory(cfg).await)
    } else {
        None
    };
    let artifact_root = artifacts::artifact_root();
    let artifact_files = if wants(&stores, RESET_STORE_ARTIFACTS) {
        Some(artifacts::count_files(&artifact_root))
    } else {
        None
    };

    let mut warnings = Vec::new();
    let plan = build_plan(
        cfg,
        &stores,
        &sqlite_inv,
        qdrant_inv.as_ref(),
        &artifact_root,
        artifact_files,
    );

    if dry_run {
        return Ok(ResetResult {
            reset_id,
            stores,
            dry_run: true,
            plan,
            deleted: ResetDeleted::default(),
            created: ResetCreated::default(),
            receipt_path: None,
            warnings,
        });
    }

    let (deleted, created) = execute(
        cfg,
        &stores,
        &sqlite_inv,
        qdrant_inv.as_ref(),
        &artifact_root,
        &mut warnings,
    )
    .await?;

    let receipt = write_receipt(&reset_id, &stores, &plan, &deleted, &created, &warnings).await;
    let receipt_path = match receipt {
        Ok(path) => Some(path),
        Err(e) => {
            warnings.push(format!("failed to write reset receipt: {e}"));
            None
        }
    };

    Ok(ResetResult {
        reset_id,
        stores,
        dry_run: false,
        plan,
        deleted,
        created,
        receipt_path,
        warnings,
    })
}

fn build_plan(
    cfg: &Config,
    stores: &[String],
    sqlite_inv: &SqliteInventory,
    qdrant_inv: Option<&QdrantInventory>,
    artifact_root: &std::path::Path,
    artifact_files: Option<usize>,
) -> Vec<ResetStorePlan> {
    let mut plan = Vec::new();
    let sqlite_path = cfg.sqlite_path.display().to_string();
    let action = if is_dry_run(cfg) { "would" } else { "did" };

    for store in stores {
        if SQLITE_STORES.contains(&store.as_str()) {
            plan.push(ResetStorePlan {
                store: store.clone(),
                location: sqlite_path.clone(),
                non_empty: sqlite_inv.non_empty(),
                item_count: Some(sqlite_inv.content_rows),
                detail: format!(
                    "{action} wipe + re-migrate the unified SQLite DB ({} tables)",
                    sqlite_inv.table_count
                ),
            });
        } else if store == RESET_STORE_VECTORS {
            let inv = qdrant_inv.cloned().unwrap_or_default();
            let detail = if inv.unreachable {
                "Qdrant unreachable — collection could not be inventoried".to_string()
            } else if inv.exists {
                format!(
                    "{action} drop + recreate collection '{}' ({} points, min payload schema v{})",
                    cfg.collection,
                    inv.points,
                    inv.min_schema_version.unwrap_or(0)
                )
            } else {
                format!(
                    "collection '{}' does not exist — nothing to drop",
                    cfg.collection
                )
            };
            plan.push(ResetStorePlan {
                store: store.clone(),
                location: format!("{}#{}", cfg.qdrant_url, cfg.collection),
                non_empty: inv.non_empty(),
                item_count: (!inv.unreachable).then_some(inv.points),
                detail,
            });
        } else if store == RESET_STORE_ARTIFACTS {
            let files = artifact_files.unwrap_or(0);
            plan.push(ResetStorePlan {
                store: store.clone(),
                location: artifact_root.display().to_string(),
                non_empty: files > 0,
                item_count: Some(files as u64),
                detail: format!("{action} delete {files} artifact file(s) under the artifact root"),
            });
        }
    }
    plan
}

/// Perform the destructive actions for the selected stores. Any single SQLite
/// store selection wipes+re-migrates the whole unified DB exactly once.
async fn execute(
    cfg: &Config,
    stores: &[String],
    _sqlite_inv: &SqliteInventory,
    qdrant_inv: Option<&QdrantInventory>,
    artifact_root: &std::path::Path,
    warnings: &mut Vec<String>,
) -> Result<(ResetDeleted, ResetCreated), Box<dyn Error>> {
    let mut deleted = ResetDeleted::default();
    let mut created = ResetCreated::default();

    if wants_any_sqlite(stores) {
        deleted.sqlite_tables = _sqlite_inv.table_count;
        let version = sqlite::wipe_and_remigrate(&cfg.sqlite_path).await?;
        created.sqlite_schema_version = version;
        log_info(&format!(
            "reset sqlite wiped + re-migrated path={} schema_version={version}",
            cfg.sqlite_path.display()
        ));
    }

    if wants(stores, RESET_STORE_VECTORS) {
        let inv = qdrant_inv.cloned().unwrap_or_default();
        if inv.unreachable {
            warnings.push(format!(
                "Qdrant unreachable — collection '{}' not reset",
                cfg.collection
            ));
        } else {
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
                        "TEI embedding dimension unavailable — collection '{}' dropped but not \
                         recreated; it will be created lazily on the first embed",
                        cfg.collection
                    ));
                    log_warn(&format!(
                        "reset qdrant dropped but not recreated (no TEI dim) collection='{}'",
                        cfg.collection
                    ));
                }
            }
        }
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

/// Write the reset receipt as a JSON artifact under the artifact root. Returns
/// its filesystem path. The receipt is the durable record of a destructive
/// reset required by the cutover contract.
async fn write_receipt(
    reset_id: &str,
    stores: &[String],
    plan: &[ResetStorePlan],
    deleted: &ResetDeleted,
    created: &ResetCreated,
    warnings: &[String],
) -> Result<String, Box<dyn Error>> {
    let root = artifacts::artifact_root();
    let relative = format!("reset/{reset_id}.json");
    let receipt = serde_json::json!({
        "reset_id": reset_id,
        "written_at_utc": chrono::Utc::now().to_rfc3339(),
        "stores": stores,
        "dry_run": false,
        "plan": plan,
        "deleted": deleted,
        "created": created,
        "warnings": warnings,
    });
    let bytes = serde_json::to_vec_pretty(&receipt)?;
    let path = axon_core::artifacts::atomic_write_under(&root, &relative, &bytes).await?;
    Ok(path.display().to_string())
}

#[cfg(test)]
#[path = "reset_tests.rs"]
mod tests;
