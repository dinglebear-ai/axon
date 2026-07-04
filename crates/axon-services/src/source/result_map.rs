//! Mapping from the per-family `*SourceIndexOutput` shape to the
//! transport-neutral [`SourceResult`] DTO.
//!
//! Every family bridge returns the same numeric shape (documents/chunks/
//! vector-points/generation/source-id + a `removed_*` count that differs only in
//! field name). This module normalizes that into [`IndexCounts`] and maps it
//! onto a [`SourceResult`] via [`to_source_result`], so each dispatch arm shares
//! one mapping instead of hand-building the DTO.

use axon_api::source::{
    AdapterRef, GraphWriteSummary, JobId, LedgerSummary, LifecycleStatus, SourceCounts,
    SourceGenerationId, SourceId, SourceKind, SourceResult, SourceScope,
};

/// The normalized numeric shape shared by every `*SourceIndexOutput`.
///
/// The bridges name their removal count differently (`removed_files`,
/// `removed_entries`, `removed_pages`, …); this collapses them to a single
/// `removed` field so mapping is uniform.
pub struct IndexCounts {
    pub job_id: JobId,
    pub source_id: SourceId,
    pub generation: SourceGenerationId,
    pub documents_prepared: u64,
    pub chunks_prepared: u64,
    pub vector_points_written: u64,
    pub removed: u64,
}

/// Build a [`SourceResult`] from a family's normalized index output.
///
/// `kind`/`adapter`/`scope` identify the routed family; `counts` carries the
/// bridge's numeric result; `graph` is the summary of the baseline source-graph
/// write. Status is always `Completed` here — the bridges return `Ok(..)` only
/// after a committed generation, and any acquisition or data-plane failure is
/// surfaced as an `Err` before this is reached.
pub fn to_source_result(
    kind: SourceKind,
    adapter: AdapterRef,
    scope: SourceScope,
    canonical_uri: String,
    counts: IndexCounts,
    graph: GraphWriteSummary,
) -> SourceResult {
    let source_counts = SourceCounts {
        items_total: counts.documents_prepared,
        items_changed: counts.documents_prepared,
        documents_total: counts.documents_prepared,
        chunks_total: counts.chunks_prepared,
        vector_points_total: counts.vector_points_written,
        bytes_total: 0,
    };

    let ledger = LedgerSummary {
        source_id: counts.source_id.clone(),
        generation: counts.generation.clone(),
        committed_generation: Some(counts.generation.clone()),
        status: LifecycleStatus::Completed,
        counts: source_counts.clone(),
    };

    SourceResult {
        job_id: counts.job_id,
        source_id: counts.source_id,
        canonical_uri,
        source_kind: kind,
        adapter,
        scope,
        status: LifecycleStatus::Completed,
        ledger,
        graph,
        counts: source_counts,
        warnings: Vec::new(),
        inline: None,
        job: None,
        watch: None,
        artifacts: Vec::new(),
        errors: Vec::new(),
    }
}

/// Convenience constructor for an [`AdapterRef`].
pub fn adapter_ref(name: &str) -> AdapterRef {
    AdapterRef {
        name: name.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    }
}
