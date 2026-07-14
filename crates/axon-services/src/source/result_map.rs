//! Mapping from the per-family `*SourceIndexOutput` shape to the
//! transport-neutral [`SourceResult`] DTO.
//!
//! Every family bridge returns the same numeric shape (documents/chunks/
//! vector-points/generation/source-id + a `removed_*` count that differs only in
//! field name). This module normalizes that into [`IndexCounts`] and maps it
//! onto a [`SourceResult`] via [`to_source_result`], so each dispatch arm shares
//! one mapping instead of hand-building the DTO.

use axon_api::source::{
    AdapterRef, GraphCandidate, GraphWriteSummary, JobDescriptor, JobId, LedgerSummary,
    LifecycleStatus, SourceCounts, SourceGenerationId, SourceId, SourceKind, SourceResult,
    SourceScope, SourceWarning,
};
use axon_error::ApiError;
use uuid::Uuid;

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
    /// Parser-produced graph candidates carried up from every prepared
    /// document in this generation (`source-pipeline.md`'s `parsing` stage
    /// output), forwarded to the `graphing` stage instead of being dropped
    /// after vectorization.
    pub graph_candidates: Vec<GraphCandidate>,
    /// Non-fatal adapter/service degradations collected during acquisition or
    /// preparation and surfaced on the transport-neutral result.
    pub warnings: Vec<SourceWarning>,
    pub artifacts: Vec<axon_api::source::ArtifactRef>,
    pub inline: Option<axon_api::source::InlineSourceResult>,
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
        warnings: counts.warnings,
        inline: counts.inline,
        job: None,
        watch: None,
        artifacts: counts.artifacts,
        errors: Vec::new(),
    }
}

/// Build a `SourceResult` for a `SourceRequest` that was enqueued as a
/// detached `JobKind::Source` job instead of dispatched inline.
///
/// `descriptor.status` reflects whatever the unified store returned —
/// normally `Queued` for a freshly-created row, but a matching in-flight or
/// already-`Completed` job returned by an idempotency-key hit surfaces its
/// real status here too. All other counts are zero; the caller polls
/// `job.status_url` for the real outcome.
pub fn queued_result(
    kind: SourceKind,
    adapter: AdapterRef,
    scope: SourceScope,
    canonical_uri: String,
    descriptor: JobDescriptor,
) -> SourceResult {
    let zero = SourceCounts {
        items_total: 0,
        items_changed: 0,
        documents_total: 0,
        chunks_total: 0,
        vector_points_total: 0,
        bytes_total: 0,
    };
    let source_id = SourceId::new(&canonical_uri);
    SourceResult {
        job_id: descriptor.job_id,
        source_id: source_id.clone(),
        canonical_uri,
        source_kind: kind,
        adapter,
        scope,
        status: descriptor.status,
        ledger: LedgerSummary {
            source_id,
            generation: SourceGenerationId::new(""),
            committed_generation: None,
            status: descriptor.status,
            counts: zero.clone(),
        },
        graph: GraphWriteSummary {
            nodes_upserted: 0,
            edges_upserted: 0,
            evidence_records: 0,
            degraded: false,
        },
        counts: zero,
        warnings: Vec::new(),
        inline: None,
        job: Some(descriptor),
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

/// Build a degraded [`SourceResult`] when the data plane is not configured.
///
/// Mirrors the CLI's `require_data_plane` guard, but as a `Failed`
/// `SourceResult` with an explanatory warning instead of an `Err`, so the
/// transport contract (`Ok(SourceResult)`) is preserved.
pub fn degraded_no_data_plane(
    input: &str,
    kind: SourceKind,
    adapter: AdapterRef,
    scope: SourceScope,
) -> SourceResult {
    failed_result(
        input,
        kind,
        adapter,
        scope,
        "data_plane_unconfigured",
        "source indexing requires a running data plane (set qdrant_url + tei_url; \
         available under serve/mcp/--wait)",
    )
}

/// Build a failed [`SourceResult`] for an unsupported / empty input.
pub fn unsupported_result(input: &str, message: &str) -> SourceResult {
    failed_result(
        input,
        SourceKind::Web,
        adapter_ref("unsupported"),
        SourceScope::Site,
        "unsupported_source",
        message,
    )
}

/// Build a failed [`SourceResult`] from a resolving/routing/authorizing-stage
/// [`ApiError`], carrying the error's code/message as the single warning.
pub fn route_error_result(input: &str, err: ApiError) -> SourceResult {
    let mut result = unsupported_result(input, &err.message);
    result.warnings.clear();
    result.warnings.push(SourceWarning {
        code: err.code.0,
        severity: axon_api::source::Severity::Failed,
        message: err.message,
        source_item_key: None,
        retryable: false,
    });
    result
}

/// Shared constructor for a `Failed` [`SourceResult`] carrying a single warning.
fn failed_result(
    input: &str,
    kind: SourceKind,
    adapter: AdapterRef,
    scope: SourceScope,
    code: &str,
    message: &str,
) -> SourceResult {
    let zero = SourceCounts {
        items_total: 0,
        items_changed: 0,
        documents_total: 0,
        chunks_total: 0,
        vector_points_total: 0,
        bytes_total: 0,
    };
    let source_id = SourceId::new(input);
    SourceResult {
        job_id: JobId::new(Uuid::nil()),
        source_id: source_id.clone(),
        canonical_uri: input.to_string(),
        source_kind: kind,
        adapter,
        scope,
        status: LifecycleStatus::Failed,
        ledger: LedgerSummary {
            source_id,
            generation: SourceGenerationId::new(""),
            committed_generation: None,
            status: LifecycleStatus::Failed,
            counts: zero.clone(),
        },
        graph: GraphWriteSummary {
            nodes_upserted: 0,
            edges_upserted: 0,
            evidence_records: 0,
            degraded: true,
        },
        counts: zero,
        warnings: vec![SourceWarning {
            code: code.to_string(),
            severity: axon_api::source::Severity::Failed,
            message: message.to_string(),
            source_item_key: None,
            retryable: false,
        }],
        inline: None,
        job: None,
        watch: None,
        artifacts: Vec::new(),
        errors: Vec::new(),
    }
}
