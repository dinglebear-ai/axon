//! Transport-neutral prune DTOs.
//!
//! Shared wire shapes for `axon-prune`. These mirror the pruning contract in
//! `docs/pipeline-unification/runtime/pruning-contract.md`:
//! `PruneRequest`, `PruneSelector`, `PrunePlan`, `PruneResult` (plus the
//! supporting `PruneStep`, `PruneEstimate`, `PruneCounts`, and
//! `PruneStepResult` shapes the plan/result reference). They are data
//! contracts only — planner/executor logic lives in `axon-prune`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::common::SourceWarning;
use super::enums::LifecycleStatus;
use super::ids::*;
use super::vector::VectorDeleteSelector;

/// A request to prune (destructively clean up) some slice of stored state.
///
/// Per the pruning contract, prune is never ad hoc deletion: every request
/// carries a `selector` (scope), a `dry_run` flag (default-safe), a
/// `require_confirmation` gate for destructive execution, and an audit
/// `reason`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PruneRequest {
    pub selector: PruneSelector,
    #[serde(default = "default_dry_run")]
    pub dry_run: bool,
    #[serde(default)]
    pub require_confirmation: bool,
    #[serde(default)]
    pub reason: String,
}

impl PruneRequest {
    /// A default-safe dry-run request for `selector`.
    pub fn dry_run(selector: PruneSelector, reason: impl Into<String>) -> Self {
        Self {
            selector,
            dry_run: true,
            require_confirmation: false,
            reason: reason.into(),
        }
    }

    /// An executing (non-dry-run) request for `selector`.
    pub fn execute(selector: PruneSelector, reason: impl Into<String>) -> Self {
        Self {
            selector,
            dry_run: false,
            require_confirmation: true,
            reason: reason.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PruneExecuteRequest {
    pub plan: PrunePlan,
    pub confirm: bool,
    pub reason: String,
}

/// The scope of a prune. Each variant resolves to a concrete plan by the
/// `axon-prune` planner. Variants mirror the pruning contract exactly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum PruneSelector {
    /// Prune an entire source (all generations).
    Source { source_id: SourceId },
    /// Prune one generation of a source (generation-fenced).
    Generation {
        source_id: SourceId,
        generation: SourceGenerationId,
    },
    /// Execute a recorded cleanup-debt entry.
    CleanupDebt { debt_id: CleanupDebtId },
    /// Prune a whole vector collection.
    Collection { collection: String },
    /// Prune a single artifact by id (never by arbitrary path).
    Artifact { artifact_id: ArtifactId },
    /// Prune graph nodes/edges (orphan cleanup).
    Graph {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        node_id: Option<GraphNodeId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        edge_id: Option<GraphEdgeId>,
    },
    /// Prune (forget) memory records.
    Memory {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        memory_id: Option<MemoryId>,
    },
    /// Job-retention cleanup older than N days.
    JobRetention { older_than_days: u32 },
    /// Cache cleanup older than N days.
    Cache { older_than_days: u32 },
}

/// A resolved, reviewable plan describing exactly what a prune would delete.
///
/// A plan is produced by dry-run resolution and never mutates state. It must be
/// serializable to JSON for review (safety rule: "prune plans must be
/// reviewable as JSON").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PrunePlan {
    pub job_id: JobId,
    pub selector: PruneSelector,
    pub destructive: bool,
    pub requires_admin: bool,
    pub estimated: PruneEstimate,
    pub steps: Vec<PruneStep>,
    pub warnings: Vec<SourceWarning>,
}

/// Estimated impact counts for a plan (what *would* be deleted).
#[derive(
    Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(deny_unknown_fields)]
pub struct PruneEstimate {
    pub vector_points: u64,
    pub artifacts: u64,
    pub graph_nodes: u64,
    pub graph_edges: u64,
    pub memory_records: u64,
    pub ledger_generations: u64,
    pub jobs: u64,
    pub cache_entries: u64,
}

/// One ordered step in a prune plan. Steps follow the cleanup-debt execution
/// order (vector → artifact → graph → memory → ledger → job/cache).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PruneStep {
    pub target: PruneTargetKind,
    pub description: String,
    pub estimated_deletes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vector_selector: Option<VectorDeleteSelector>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<SourceId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation: Option<SourceGenerationId>,
    /// Identity for a [`PruneTargetKind::Graph`] node-delete step (stable
    /// keys, matching [`axon_graph`]'s `GraphStore::delete_nodes`). Additive —
    /// existing `Vector`/`Ledger` steps never set this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph_stable_keys: Option<Vec<String>>,
    /// Identity for a [`PruneTargetKind::Graph`] edge-delete step, matching
    /// `GraphStore::delete_edges`. Additive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph_edge_ids: Option<Vec<GraphEdgeId>>,
    /// Identity for a [`PruneTargetKind::Memory`] forget step, matching
    /// `MemoryStore::forget`. Additive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_ids: Option<Vec<MemoryId>>,
}

/// The store boundary a prune step targets, in cleanup-debt execution order.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PruneTargetKind {
    Vector,
    Artifact,
    Graph,
    Memory,
    Ledger,
    JobRetention,
    Cache,
}

impl PruneTargetKind {
    /// The canonical cleanup-debt execution order (contract §"Debt execution
    /// order"). Ledger prune runs last so join metadata stays available.
    pub const EXECUTION_ORDER: [PruneTargetKind; 7] = [
        PruneTargetKind::Vector,
        PruneTargetKind::Artifact,
        PruneTargetKind::Graph,
        PruneTargetKind::Memory,
        PruneTargetKind::Ledger,
        PruneTargetKind::JobRetention,
        PruneTargetKind::Cache,
    ];

    /// Rank of this target in the canonical execution order (lower runs first).
    pub fn order_rank(self) -> usize {
        Self::EXECUTION_ORDER
            .iter()
            .position(|k| *k == self)
            .unwrap_or(usize::MAX)
    }
}

/// The outcome of executing a prune plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PruneResult {
    pub job_id: JobId,
    pub status: LifecycleStatus,
    pub steps: Vec<PruneStepResult>,
    pub deleted_counts: PruneCounts,
    pub cleanup_debt_remaining: u64,
}

/// Per-step execution outcome. Records what was actually deleted plus any
/// skipped reason, satisfying the receipt requirement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PruneStepResult {
    pub target: PruneTargetKind,
    pub status: LifecycleStatus,
    pub deleted: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skipped_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<SourceId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation: Option<SourceGenerationId>,
}

/// Actual deletion counts from an executed prune (the receipt's tallies).
#[derive(
    Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(deny_unknown_fields)]
pub struct PruneCounts {
    pub vector_points: u64,
    pub artifacts: u64,
    pub graph_nodes: u64,
    pub graph_edges: u64,
    pub memory_records: u64,
    pub ledger_generations: u64,
    pub jobs: u64,
    pub cache_entries: u64,
}

impl PruneCounts {
    /// Total items deleted across all boundaries.
    pub fn total(&self) -> u64 {
        self.vector_points
            + self.artifacts
            + self.graph_nodes
            + self.graph_edges
            + self.memory_records
            + self.ledger_generations
            + self.jobs
            + self.cache_entries
    }
}

fn default_dry_run() -> bool {
    true
}
