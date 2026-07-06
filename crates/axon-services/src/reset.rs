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
mod execution;
mod qdrant;
mod sqlite;

pub use qdrant::QdrantInventory;
pub use sqlite::SqliteInventory;

use axon_api::reset::{
    RESET_ALL_STORES, RESET_STORE_ARTIFACTS, RESET_STORE_CODE_INDEX, RESET_STORE_GRAPH,
    RESET_STORE_JOBS, RESET_STORE_LEDGER, RESET_STORE_MEMORY, RESET_STORE_VECTORS,
    RESET_STORE_WATCH, ResetChunkReceipt, ResetCreated, ResetDeleted, ResetEstimate,
    ResetExecutionState, ResetPlan, ResetReceipt, ResetResult, ResetStorePlan,
};
use axon_core::config::Config;
use axon_core::logging::{log_info, log_warn};
use execution::{execute, write_receipt};
use sha2::{Digest, Sha256};
use std::error::Error;
use std::path::PathBuf;
use uuid::Uuid;

/// The SQLite-backed stores that all live in the single unified DB. Selecting
/// any of them means the DB is wiped + re-migrated (the composed migration set
/// recreates every store's tables), so they share one destructive action.
const SQLITE_STORES: &[&str] = &[
    RESET_STORE_JOBS,
    RESET_STORE_LEDGER,
    RESET_STORE_CODE_INDEX,
    RESET_STORE_WATCH,
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

struct PreparedReset {
    stores: Vec<String>,
    reset_id: String,
    plan_id: String,
    sqlite_inv: SqliteInventory,
    qdrant_inv: Option<QdrantInventory>,
    artifact_root: PathBuf,
    artifact_files: Option<usize>,
    warnings: Vec<String>,
    plan: Vec<ResetStorePlan>,
    reset_plan: ResetPlan,
    estimates: ResetEstimate,
    inventory_checksum: String,
    config_snapshot_id: String,
    auth_snapshot_id: String,
    confirmation_text: String,
    plan_expires_at_utc: String,
    receipt_preview_path: Option<String>,
}

/// Run `axon reset`. In dry-run (default) it inventories every selected store
/// and returns the plan without mutating. Under `--yes` it wipes + recreates
/// the selected stores and writes a receipt artifact.
pub async fn reset(cfg: &Config) -> Result<ResetResult, Box<dyn Error>> {
    let stores = resolve_stores(cfg)?;
    let dry_run = is_dry_run(cfg);
    let reset_id = format!("reset_{}", Uuid::new_v4().simple());
    let plan_id = cfg
        .reset_plan_id
        .clone()
        .unwrap_or_else(|| format!("reset_plan_{}", Uuid::new_v4().simple()));

    log_info(&format!(
        "command=reset id={reset_id} dry_run={dry_run} stores={}",
        stores.join(",")
    ));

    let prepared = prepare_reset(cfg, stores, reset_id, plan_id, dry_run).await?;
    if dry_run {
        return Ok(planned_reset_result(prepared));
    }
    execute_prepared_reset(cfg, prepared).await
}

async fn prepare_reset(
    cfg: &Config,
    stores: Vec<String>,
    reset_id: String,
    plan_id: String,
    dry_run: bool,
) -> Result<PreparedReset, Box<dyn Error>> {
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

    let plan = build_plan(
        cfg,
        &stores,
        &sqlite_inv,
        qdrant_inv.as_ref(),
        &artifact_root,
        artifact_files,
    );
    let estimates = estimate(&plan);
    let inventory_checksum = compute_inventory_checksum(&stores, &plan, &estimates);
    let config_snapshot_id = format!("cfg_{}", short_hash(&cfg_snapshot_material(cfg)));
    let auth_snapshot_id = if dry_run {
        "auth_readonly_local_cli".to_string()
    } else {
        "auth_admin_local_cli".to_string()
    };
    let plan_expires_at_utc = (chrono::Utc::now() + chrono::Duration::minutes(15)).to_rfc3339();
    let confirmation_text = format!(
        "reset {plan_id} will destroy and recreate stores: {}",
        stores.join(",")
    );
    let receipt_preview_path = Some(
        artifacts::artifact_root()
            .join("reset")
            .join(format!("{reset_id}.json"))
            .display()
            .to_string(),
    );
    let reset_plan = ResetPlan {
        plan_id: plan_id.clone(),
        reset_id: reset_id.clone(),
        stores: stores.clone(),
        estimates: estimates.clone(),
        inventory_checksum: inventory_checksum.clone(),
        config_snapshot_id: config_snapshot_id.clone(),
        auth_snapshot_id: auth_snapshot_id.clone(),
        confirmation_text: confirmation_text.clone(),
        receipt_path: receipt_preview_path.clone(),
        expires_at_utc: plan_expires_at_utc.clone(),
        blockers: Vec::new(),
    };

    Ok(PreparedReset {
        stores,
        reset_id,
        plan_id,
        sqlite_inv,
        qdrant_inv,
        artifact_root,
        artifact_files,
        warnings: Vec::new(),
        plan,
        reset_plan,
        estimates,
        inventory_checksum,
        config_snapshot_id,
        auth_snapshot_id,
        confirmation_text,
        plan_expires_at_utc,
        receipt_preview_path,
    })
}

fn planned_reset_result(prepared: PreparedReset) -> ResetResult {
    ResetResult {
        plan_id: prepared.plan_id,
        reset_id: prepared.reset_id,
        stores: prepared.stores,
        dry_run: true,
        plan: prepared.plan,
        reset_plan: prepared.reset_plan,
        estimates: prepared.estimates,
        execution_state: ResetExecutionState::Planned,
        inventory_checksum: prepared.inventory_checksum,
        config_snapshot_id: prepared.config_snapshot_id,
        auth_snapshot_id: prepared.auth_snapshot_id,
        confirmation_text: prepared.confirmation_text,
        plan_expires_at_utc: prepared.plan_expires_at_utc,
        blockers: Vec::new(),
        chunks: Vec::new(),
        audit_events: vec!["reset.plan".to_string()],
        deleted: ResetDeleted::default(),
        created: ResetCreated::default(),
        receipt_path: prepared.receipt_preview_path,
        warnings: prepared.warnings,
    }
}

async fn execute_prepared_reset(
    cfg: &Config,
    mut prepared: PreparedReset,
) -> Result<ResetResult, Box<dyn Error>> {
    let mut audit_events = vec![
        "reset.plan".to_string(),
        "reset.confirm".to_string(),
        "reset.execute".to_string(),
    ];
    let before_execute = build_plan(
        cfg,
        &prepared.stores,
        &sqlite::inventory(&cfg.sqlite_path).await?,
        prepared.qdrant_inv.as_ref(),
        &prepared.artifact_root,
        prepared.artifact_files,
    );
    let before_checksum = compute_inventory_checksum(
        &prepared.stores,
        &before_execute,
        &estimate(&before_execute),
    );
    if before_checksum != prepared.inventory_checksum {
        return Err(format!(
            "reset.inventory_changed: plan {} inventory changed before execution",
            prepared.plan_id
        )
        .into());
    }

    let (deleted, created) = execute(
        cfg,
        &prepared.stores,
        &prepared.sqlite_inv,
        prepared.qdrant_inv.as_ref(),
        &prepared.artifact_root,
        &mut prepared.warnings,
    )
    .await?;
    let chunks = reset_chunks(&prepared.stores, &deleted, &created);
    audit_events.push("reset.complete".to_string());

    let receipt = ResetReceipt {
        plan_id: prepared.plan_id.clone(),
        reset_id: prepared.reset_id.clone(),
        state: ResetExecutionState::Completed,
        chunks: chunks.clone(),
        deleted: deleted.clone(),
        created: created.clone(),
        audit_events: audit_events.clone(),
    };
    let receipt = write_receipt(
        &prepared.reset_id,
        &prepared.stores,
        &prepared.plan,
        &prepared.reset_plan,
        &receipt,
        &prepared.warnings,
    )
    .await;
    let receipt_path = match receipt {
        Ok(path) => Some(path),
        Err(e) => {
            prepared
                .warnings
                .push(format!("failed to write reset receipt: {e}"));
            None
        }
    };

    Ok(ResetResult {
        plan_id: prepared.plan_id,
        reset_id: prepared.reset_id,
        stores: prepared.stores,
        dry_run: false,
        plan: prepared.plan,
        reset_plan: prepared.reset_plan,
        estimates: prepared.estimates,
        execution_state: ResetExecutionState::Completed,
        inventory_checksum: prepared.inventory_checksum,
        config_snapshot_id: prepared.config_snapshot_id,
        auth_snapshot_id: prepared.auth_snapshot_id,
        confirmation_text: prepared.confirmation_text,
        plan_expires_at_utc: prepared.plan_expires_at_utc,
        blockers: Vec::new(),
        chunks,
        audit_events,
        deleted,
        created,
        receipt_path,
        warnings: prepared.warnings,
    })
}

fn estimate(plan: &[ResetStorePlan]) -> ResetEstimate {
    let mut estimate = ResetEstimate::default();
    for row in plan {
        let count = row.item_count.unwrap_or(0);
        match row.store.as_str() {
            RESET_STORE_VECTORS => {
                estimate.qdrant_points = estimate.qdrant_points.saturating_add(count);
                if count > 0 {
                    estimate.qdrant_collections = estimate.qdrant_collections.saturating_add(1);
                }
            }
            RESET_STORE_ARTIFACTS => {
                estimate.artifact_files = estimate.artifact_files.saturating_add(count);
            }
            _ => {
                estimate.sqlite_rows = estimate.sqlite_rows.saturating_add(count);
                if row.non_empty || count > 0 {
                    estimate.sqlite_tables = estimate.sqlite_tables.saturating_add(1);
                }
            }
        }
    }
    estimate
}

fn compute_inventory_checksum(
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

fn cfg_snapshot_material(cfg: &Config) -> String {
    format!(
        "sqlite={};qdrant={};collection={};stores={}",
        cfg.sqlite_path.display(),
        cfg.qdrant_url,
        cfg.collection,
        cfg.reset_stores.join(",")
    )
}

fn short_hash(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let digest = hasher.finalize();
    format!("{digest:x}").chars().take(16).collect()
}

fn reset_chunks(
    stores: &[String],
    deleted: &ResetDeleted,
    created: &ResetCreated,
) -> Vec<ResetChunkReceipt> {
    stores
        .iter()
        .enumerate()
        .map(|(idx, store)| {
            let item_count = match store.as_str() {
                RESET_STORE_VECTORS => deleted.qdrant_collections.len() as u64,
                RESET_STORE_ARTIFACTS => deleted.artifact_files as u64,
                _ => deleted.sqlite_tables as u64,
            };
            let checkpoint = if store == RESET_STORE_VECTORS {
                format!("created={}", created.qdrant_collections.join(","))
            } else if SQLITE_STORES.contains(&store.as_str()) {
                format!("schema_version={}", created.sqlite_schema_version)
            } else {
                "complete".to_string()
            };
            ResetChunkReceipt {
                chunk_id: format!("chunk_{idx:04}"),
                store: store.clone(),
                status: "completed".to_string(),
                item_count,
                checkpoint,
            }
        })
        .collect()
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

#[cfg(test)]
#[path = "reset_tests.rs"]
mod tests;
