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

pub(crate) mod artifacts;
mod execution;
mod plan_store;
mod planning;
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
#[cfg(test)]
use execution::write_receipt;
use std::error::Error;
use std::path::PathBuf;
use uuid::Uuid;

use planning::{build_plan, estimate, inventory_checksum};

#[derive(Debug, Clone, Copy, Default)]
pub struct ResetAuthz {
    pub is_admin: bool,
}

impl ResetAuthz {
    pub fn admin() -> Self {
        Self { is_admin: true }
    }

    pub fn anonymous() -> Self {
        Self { is_admin: false }
    }
}

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
    // SQLite is one physical store. Selecting any logical SQLite owner wipes
    // and re-migrates the whole DB, so expand the reviewed plan to every
    // affected logical owner instead of hiding collateral deletion.
    let expands_sqlite = out
        .iter()
        .any(|store| SQLITE_STORES.contains(&store.as_str()));
    Ok(RESET_ALL_STORES
        .iter()
        .filter(|store| {
            out.iter().any(|selected| selected == *store)
                || (expands_sqlite && SQLITE_STORES.contains(store))
        })
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
    /// Human-readable blocker messages surfaced on `ResetPlan`/`ResetResult`.
    blockers: Vec<String>,
}

/// Run `axon reset`. In dry-run (default) it inventories every selected store
/// and returns the plan without mutating. Under `--yes` it wipes + recreates
/// the selected stores and writes a receipt artifact.
pub async fn reset(cfg: &Config) -> Result<ResetResult, Box<dyn Error>> {
    reset_with_authz(cfg, &ResetAuthz::anonymous()).await
}

pub async fn reset_with_authz(
    cfg: &Config,
    authz: &ResetAuthz,
) -> Result<ResetResult, Box<dyn Error>> {
    let stores = resolve_stores(cfg)?;
    let dry_run = is_dry_run(cfg);
    if dry_run && cfg.reset_plan_id.is_some() {
        return Err("reset.plan_id_without_execute: --plan-id requires --yes".into());
    }
    if !dry_run && !cfg.yes {
        return Err("reset.confirmation_required: pass --yes after reviewing a reset plan".into());
    }
    if !dry_run && !authz.is_admin {
        return Err("reset.admin_required: destructive reset requires axon:admin".into());
    }

    if !dry_run {
        let plan_id = cfg
            .reset_plan_id
            .as_deref()
            .ok_or("reset.plan_required: execute with --plan-id from a reviewed dry-run")?;
        return execute_saved_plan(cfg, plan_id, authz).await;
    }

    let reset_id = format!("reset_{}", Uuid::new_v4().simple());
    let plan_id = format!("reset_plan_{}", Uuid::new_v4().simple());

    log_info(&format!(
        "command=reset id={reset_id} dry_run={dry_run} stores={}",
        stores.join(",")
    ));

    let prepared = prepare_reset(cfg, stores, reset_id, plan_id, dry_run).await?;
    let result = planned_reset_result(prepared);
    plan_store::save_plan(cfg, &result).await?;
    Ok(result)
}

async fn execute_saved_plan(
    cfg: &Config,
    plan_id: &str,
    authz: &ResetAuthz,
) -> Result<ResetResult, Box<dyn Error>> {
    if !authz.is_admin {
        return Err("reset.admin_required: destructive reset requires axon:admin".into());
    }
    let saved = plan_store::load_plan(cfg, plan_id).await?;
    if saved.plan_id != plan_id {
        return Err("reset.plan_id_mismatch: stored plan identity does not match request".into());
    }
    if let Some(receipt) = plan_store::load_receipt(cfg, &saved.reset_id).await? {
        if matches!(
            receipt.state,
            ResetExecutionState::Completed | ResetExecutionState::CompletedDegraded
        ) {
            let path = plan_store::receipt_path(cfg, &saved.reset_id)?;
            return Ok(result_from_receipt(saved, receipt, path));
        }
    }
    let expires = chrono::DateTime::parse_from_rfc3339(&saved.plan_expires_at_utc)?;
    if expires < chrono::Utc::now() {
        return Err("reset.plan_expired: create and review a new reset plan".into());
    }
    if !saved.blockers.is_empty() {
        return Err(format!("reset.plan_blocked: {}", saved.blockers.join("; ")).into());
    }
    if !cfg.reset_stores.is_empty() && resolve_stores(cfg)? != saved.stores {
        return Err("reset.plan_scope_changed: --stores differs from reviewed plan".into());
    }
    let prepared = prepare_reset(cfg, saved.stores, saved.reset_id, saved.plan_id, false).await?;
    if prepared.config_snapshot_id != saved.config_snapshot_id {
        return Err("reset.config_changed: create and review a new reset plan".into());
    }
    if prepared.inventory_checksum != saved.inventory_checksum {
        return Err("reset.inventory_changed: create and review a new reset plan".into());
    }
    execute_prepared_reset(cfg, prepared).await
}

fn result_from_receipt(
    mut saved: ResetResult,
    receipt: ResetReceipt,
    path: PathBuf,
) -> ResetResult {
    saved.dry_run = false;
    saved.execution_state = receipt.state;
    saved.chunks = receipt.chunks;
    saved.audit_events = receipt.audit_events;
    saved.deleted = receipt.deleted;
    saved.created = receipt.created;
    saved.receipt_path = Some(path.display().to_string());
    saved
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

    let mut blockers = Vec::new();
    if qdrant_inv
        .as_ref()
        .is_some_and(|inventory| inventory.unreachable)
    {
        blockers.push(format!(
            "vectors store '{}' is unreachable and cannot be reset safely",
            cfg.collection
        ));
    }

    let plan = build_plan(
        cfg,
        &stores,
        &sqlite_inv,
        qdrant_inv.as_ref(),
        &artifact_root,
        artifact_files,
    );
    let estimates = estimate(&plan);
    let inventory_checksum = inventory_checksum(&stores, &plan, &estimates);
    let config_snapshot_id = format!("cfg_{}", planning::config_snapshot_id(cfg));
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
        plan_store::receipt_path(cfg, &reset_id)?
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
        blockers: blockers.clone(),
    };

    Ok(PreparedReset {
        stores,
        reset_id,
        plan_id,
        sqlite_inv,
        qdrant_inv,
        artifact_root,
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
        blockers,
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
        blockers: prepared.blockers,
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
    let current_qdrant = if wants(&prepared.stores, RESET_STORE_VECTORS) {
        Some(qdrant::inventory(cfg).await)
    } else {
        None
    };
    let current_artifacts = wants(&prepared.stores, RESET_STORE_ARTIFACTS)
        .then(|| artifacts::count_files(&prepared.artifact_root));
    let before_execute = build_plan(
        cfg,
        &prepared.stores,
        &sqlite::inventory(&cfg.sqlite_path).await?,
        current_qdrant.as_ref(),
        &prepared.artifact_root,
        current_artifacts,
    );
    let before_checksum = inventory_checksum(
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

    let existing_receipt = plan_store::load_receipt(cfg, &prepared.reset_id).await?;
    let (receipt, receipt_path) = execution::execute_resumable(
        cfg,
        &prepared.stores,
        &prepared.sqlite_inv,
        prepared.qdrant_inv.as_ref(),
        &prepared.artifact_root,
        &mut prepared.warnings,
        &prepared.reset_plan,
        existing_receipt,
    )
    .await?;
    let chunks = receipt.chunks.clone();
    let deleted = receipt.deleted.clone();
    let created = receipt.created.clone();
    let audit_events = receipt.audit_events.clone();
    let execution_state = receipt.state.clone();

    Ok(ResetResult {
        plan_id: prepared.plan_id,
        reset_id: prepared.reset_id,
        stores: prepared.stores,
        dry_run: false,
        plan: prepared.plan,
        reset_plan: prepared.reset_plan,
        estimates: prepared.estimates,
        execution_state,
        inventory_checksum: prepared.inventory_checksum,
        config_snapshot_id: prepared.config_snapshot_id,
        auth_snapshot_id: prepared.auth_snapshot_id,
        confirmation_text: prepared.confirmation_text,
        plan_expires_at_utc: prepared.plan_expires_at_utc,
        blockers: prepared.blockers,
        chunks,
        audit_events,
        deleted,
        created,
        receipt_path: Some(receipt_path),
        warnings: prepared.warnings,
    })
}

#[cfg(test)]
#[path = "reset_tests.rs"]
mod tests;
