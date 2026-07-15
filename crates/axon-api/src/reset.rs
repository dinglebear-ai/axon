//! Transport-neutral DTO for the `axon reset` clean-slate cutover command.
//!
//! Lives in `axon-api` (not `axon-services`) per the workspace architecture
//! rule: the contract type is owned by the layer transports consume directly.
//! `axon-services::reset` returns this; CLI/MCP/REST all format it.
//!
//! `reset` is intentional local-state destruction for the pipeline-unification
//! empty-DB cutover — NOT migration. Its default is a dry-run plan that mutates
//! nothing; actual destruction requires an explicit `--yes`. See
//! `docs/pipeline-unification/delivery/cutover-contract.md` ("Required Reset
//! Tooling", reset result shape).

use serde::{Deserialize, Serialize};

/// Legacy target Qdrant payload schema version from the pre-unification vector
/// payload. Kept for older docs/API references only; the unified pipeline's
/// live compatibility fence is [`TARGET_PAYLOAD_CONTRACT_VERSION`].
pub const TARGET_PAYLOAD_SCHEMA_VERSION: u32 = 8;

/// Target vector payload contract version the unified pipeline writes and
/// expects after the clean-break cutover.
///
/// Current points carry this string in `payload_contract_version`; retired
/// pre-unification points either lack it or carry only the old integer
/// `payload_schema_version`.
pub const TARGET_PAYLOAD_CONTRACT_VERSION: &str = "2026-07-01";

/// Logical stores a reset can target. String-typed at the wire boundary so the
/// registry can grow without a breaking enum change across transports.
pub const RESET_STORE_JOBS: &str = "jobs";
pub const RESET_STORE_LEDGER: &str = "ledger";
pub const RESET_STORE_CODE_INDEX: &str = "code_index";
pub const RESET_STORE_WATCH: &str = "watch";
pub const RESET_STORE_GRAPH: &str = "graph";
pub const RESET_STORE_MEMORY: &str = "memory";
pub const RESET_STORE_VECTORS: &str = "vectors";
pub const RESET_STORE_ARTIFACTS: &str = "artifacts";

/// Every store selectable by `--stores`, in canonical order.
pub const RESET_ALL_STORES: &[&str] = &[
    RESET_STORE_JOBS,
    RESET_STORE_LEDGER,
    RESET_STORE_CODE_INDEX,
    RESET_STORE_WATCH,
    RESET_STORE_GRAPH,
    RESET_STORE_MEMORY,
    RESET_STORE_VECTORS,
    RESET_STORE_ARTIFACTS,
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ResetPlanId(pub String);

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ResetEstimate {
    pub sqlite_rows: u64,
    pub sqlite_tables: u64,
    pub qdrant_points: u64,
    pub qdrant_collections: u64,
    pub artifact_files: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub enum ResetExecutionState {
    Planned,
    Executing,
    Completed,
    CompletedDegraded,
    Blocked,
    Failed,
}

/// Counts of what a reset deleted (or would delete, when `dry_run`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ResetDeleted {
    /// SQLite tables present before reset (all live in the single unified DB).
    pub sqlite_tables: usize,
    /// Qdrant collections dropped/recreated (usually just the configured one).
    pub qdrant_collections: Vec<String>,
    /// Artifact files removed under the artifact root.
    pub artifact_files: usize,
}

/// Counts of what a reset recreated at the current fresh schema.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ResetCreated {
    /// Highest applied SQLite migration version after re-migration (0 when the
    /// jobs store was not part of this reset).
    pub sqlite_schema_version: i64,
    /// Qdrant collections created fresh (named dense + bm42 sparse).
    pub qdrant_collections: Vec<String>,
}

/// Per-store inventory row rendered in the dry-run plan and receipt. This is the
/// "exact stores, paths, collections, row counts, artifact counts" the cutover
/// contract requires the dry-run to print.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ResetStorePlan {
    /// Logical store name (`jobs`/`ledger`/`graph`/`memory`/`vectors`/`artifacts`).
    pub store: String,
    /// Concrete backing location: a SQLite path, a Qdrant collection, or an
    /// artifact directory.
    pub location: String,
    /// True when the store currently holds data that a reset would destroy.
    pub non_empty: bool,
    /// Rows/points/files this store currently holds (best-effort; `None` when
    /// the backing service was unreachable during planning).
    pub item_count: Option<u64>,
    /// One-line human-readable note (e.g. "would drop + recreate collection").
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ResetChunkReceipt {
    pub chunk_id: String,
    pub store: String,
    pub status: String,
    pub item_count: u64,
    pub checkpoint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ResetPlan {
    pub plan_id: String,
    pub reset_id: String,
    pub stores: Vec<String>,
    pub estimates: ResetEstimate,
    pub inventory_checksum: String,
    pub config_snapshot_id: String,
    pub auth_snapshot_id: String,
    pub confirmation_text: String,
    pub receipt_path: Option<String>,
    pub expires_at_utc: String,
    pub blockers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ResetReceipt {
    pub plan_id: String,
    pub reset_id: String,
    pub state: ResetExecutionState,
    pub chunks: Vec<ResetChunkReceipt>,
    pub deleted: ResetDeleted,
    pub created: ResetCreated,
    pub audit_events: Vec<String>,
}

/// Result of `axon reset`. Mirrors the cutover-contract reset result shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ResetResult {
    /// Stable reusable plan id (`reset_plan_...`) bound to selected stores,
    /// estimates, auth/config snapshots, inventory checksum, and TTL.
    pub plan_id: String,
    /// Stable id for this reset invocation (`reset_...`).
    pub reset_id: String,
    /// Stores selected for this reset, in canonical order.
    pub stores: Vec<String>,
    /// True when nothing was mutated — counts and plan reflect what *would*
    /// happen. This is the default.
    pub dry_run: bool,
    /// Per-store inventory + intended action.
    pub plan: Vec<ResetStorePlan>,
    pub reset_plan: ResetPlan,
    pub estimates: ResetEstimate,
    pub execution_state: ResetExecutionState,
    pub inventory_checksum: String,
    pub config_snapshot_id: String,
    pub auth_snapshot_id: String,
    pub confirmation_text: String,
    pub plan_expires_at_utc: String,
    pub blockers: Vec<String>,
    pub chunks: Vec<ResetChunkReceipt>,
    pub audit_events: Vec<String>,
    /// What was deleted (all zero/empty when `dry_run`).
    pub deleted: ResetDeleted,
    /// What was recreated at fresh schema (all zero/empty when `dry_run`).
    pub created: ResetCreated,
    /// Filesystem path of the written reset receipt artifact (`None` in
    /// dry-run — no receipt is written when nothing is destroyed).
    pub receipt_path: Option<String>,
    /// Non-fatal warnings (unreachable service, partial delete, etc.).
    pub warnings: Vec<String>,
}

impl ResetResult {
    /// Total item count across all planned stores (points + rows + files),
    /// summing only stores with a known count.
    #[must_use]
    pub fn total_planned_items(&self) -> u64 {
        self.plan.iter().filter_map(|p| p.item_count).sum()
    }

    /// True when every selected store is already empty — a reset would be a
    /// no-op destruction. Doctor/preflight use the same emptiness signal.
    #[must_use]
    pub fn all_empty(&self) -> bool {
        self.plan.iter().all(|p| !p.non_empty)
    }
}
