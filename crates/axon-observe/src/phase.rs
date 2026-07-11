//! `PipelinePhase` registry: applies-to scope and human-readable meaning for
//! every canonical phase, per the "Phase Registry" table in
//! `docs/pipeline-unification/runtime/observability-contract.md` and the
//! "Required Phase Enum Values" list in
//! `docs/pipeline-unification/schemas/event-schema.md`.
//!
//! `PipelinePhase` itself is owned by `axon_api::source` (the canonical
//! registry lives in `foundation/types/enum-contract.md`) — this module does
//! **not** redefine it. It only adds descriptive metadata and small predicate
//! helpers so callers stop hand-rolling phase label strings.

pub const MODULE_NAME: &str = "phase";

use axon_api::source::PipelinePhase;

/// Applies-to scope and human-readable meaning for one canonical phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhaseDescriptor {
    pub phase: PipelinePhase,
    /// Job families this phase applies to (contract's "Applies To" column).
    pub applies_to: &'static str,
    /// Short human-readable description (contract's "Meaning" column).
    pub meaning: &'static str,
}

/// Canonical phase registry, in the exact order required by
/// `schemas/event-schema.md` ("Required Phase Enum Values"). `degraded` and
/// `failed` are statuses/severities, not phases, and are intentionally absent.
pub const PHASE_REGISTRY: &[PhaseDescriptor] = &[
    PhaseDescriptor {
        phase: PipelinePhase::Queued,
        applies_to: "all async jobs",
        meaning: "job accepted, not running",
    },
    PhaseDescriptor {
        phase: PipelinePhase::Requested,
        applies_to: "transport boundaries",
        meaning: "caller request accepted before planning",
    },
    PhaseDescriptor {
        phase: PipelinePhase::Resolving,
        applies_to: "source/watch/map",
        meaning: "source identity and adapter resolution",
    },
    PhaseDescriptor {
        phase: PipelinePhase::Routing,
        applies_to: "source/watch/map",
        meaning: "adapter/scope/provider selection",
    },
    PhaseDescriptor {
        phase: PipelinePhase::Authorizing,
        applies_to: "all protected ops",
        meaning: "auth, credentials, execution policy",
    },
    PhaseDescriptor {
        phase: PipelinePhase::Planning,
        applies_to: "source/prune/research",
        meaning: "execution plan built",
    },
    PhaseDescriptor {
        phase: PipelinePhase::Leasing,
        applies_to: "source/watch/jobs",
        meaning: "lease acquisition",
    },
    PhaseDescriptor {
        phase: PipelinePhase::Discovering,
        applies_to: "source/map/watch",
        meaning: "manifest/item discovery",
    },
    PhaseDescriptor {
        phase: PipelinePhase::Diffing,
        applies_to: "mutable sources",
        meaning: "manifest diff",
    },
    PhaseDescriptor {
        phase: PipelinePhase::Fetching,
        applies_to: "source/research/summarize",
        meaning: "network/local/package fetch",
    },
    PhaseDescriptor {
        phase: PipelinePhase::Rendering,
        applies_to: "web/screenshot/brand/endpoints",
        meaning: "browser/render path",
    },
    PhaseDescriptor {
        phase: PipelinePhase::Enriching,
        applies_to: "source/research/memory",
        meaning: "optional LLM/metadata/source enrichment",
    },
    PhaseDescriptor {
        phase: PipelinePhase::Normalizing,
        applies_to: "source",
        meaning: "SourceDocument construction",
    },
    PhaseDescriptor {
        phase: PipelinePhase::Parsing,
        applies_to: "source/extract",
        meaning: "parser facts and graph candidates",
    },
    PhaseDescriptor {
        phase: PipelinePhase::Graphing,
        applies_to: "source/memory/sessions",
        meaning: "graph writes",
    },
    PhaseDescriptor {
        phase: PipelinePhase::Preparing,
        applies_to: "source",
        meaning: "chunking/preparation",
    },
    PhaseDescriptor {
        phase: PipelinePhase::Batching,
        applies_to: "source/memory/query",
        meaning: "batching provider inputs",
    },
    PhaseDescriptor {
        phase: PipelinePhase::Embedding,
        applies_to: "source/memory",
        meaning: "embedding batches",
    },
    PhaseDescriptor {
        phase: PipelinePhase::Vectorizing,
        applies_to: "source/memory",
        meaning: "vector point construction before write",
    },
    PhaseDescriptor {
        phase: PipelinePhase::Upserting,
        applies_to: "source/memory",
        meaning: "vector writes",
    },
    PhaseDescriptor {
        phase: PipelinePhase::Retrieving,
        applies_to: "query/retrieve/ask",
        meaning: "vector/document retrieval",
    },
    PhaseDescriptor {
        phase: PipelinePhase::Synthesizing,
        applies_to: "ask/research/summarize/chat",
        meaning: "LLM generation",
    },
    PhaseDescriptor {
        phase: PipelinePhase::Evaluating,
        applies_to: "evaluate",
        meaning: "judge/baseline evaluation",
    },
    PhaseDescriptor {
        phase: PipelinePhase::Publishing,
        applies_to: "source",
        meaning: "generation publish",
    },
    PhaseDescriptor {
        phase: PipelinePhase::Cleaning,
        applies_to: "source/prune",
        meaning: "cleanup debt execution",
    },
    PhaseDescriptor {
        phase: PipelinePhase::Complete,
        applies_to: "all",
        meaning: "terminal success",
    },
    PhaseDescriptor {
        phase: PipelinePhase::Canceled,
        applies_to: "all",
        meaning: "terminal cancellation",
    },
];

/// Look up the registry entry for `phase`. Every `PipelinePhase` variant has
/// exactly one entry — `None` would indicate the registry has drifted from the
/// canonical enum.
pub fn describe(phase: PipelinePhase) -> Option<&'static PhaseDescriptor> {
    PHASE_REGISTRY.iter().find(|entry| entry.phase == phase)
}

/// The stable `snake_case` wire label for `phase` (matches its serde
/// representation, e.g. `PipelinePhase::Embedding` -> `"embedding"`).
pub fn label(phase: PipelinePhase) -> String {
    serde_json::to_value(phase)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| format!("{phase:?}").to_ascii_lowercase())
}

/// The human-readable meaning for `phase` (contract's "Meaning" column).
/// Falls back to an empty string when the registry is somehow missing an
/// entry (should never happen for a real enum variant).
pub fn meaning(phase: PipelinePhase) -> &'static str {
    describe(phase).map_or("", |entry| entry.meaning)
}

/// The job families `phase` applies to (contract's "Applies To" column).
pub fn applies_to(phase: PipelinePhase) -> &'static str {
    describe(phase).map_or("", |entry| entry.applies_to)
}

/// Terminal phases end a job's lifecycle. `degraded`/`failed` are statuses,
/// not phases, so a "terminal" phase only ever means success or cancellation.
pub fn is_terminal(phase: PipelinePhase) -> bool {
    matches!(phase, PipelinePhase::Complete | PipelinePhase::Canceled)
}

#[cfg(test)]
#[path = "phase_tests.rs"]
mod tests;
