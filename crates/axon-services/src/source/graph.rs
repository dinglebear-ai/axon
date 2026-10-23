//! SourceGraph write for `index_source` — the `graphing` stage.
//!
//! After a source is acquired and indexed, this module upserts two kinds of
//! [`GraphCandidate`] into the durable `axon-graph` SQLite store:
//!
//! 1. the **baseline skeleton**: one container node for the source itself
//!    (kind chosen per family from the closed registry, keyed by the source's
//!    canonical URI), one document node per indexed manifest item (kind
//!    derived from the item's [`ItemKind`]), and one containment edge
//!    (container → document) per item, each backed by a `text_mention`
//!    evidence record so the candidate validates;
//! 2. the **real parser-produced candidates** carried up from every prepared
//!    document in this generation (`source-pipeline.md`'s `parsing` stage
//!    output — repo→package edges, compose topology, session tool calls, …),
//!    collected during vectorization and forwarded here via
//!    [`IndexCounts::graph_candidates`] instead of being dropped after
//!    preparation.
//!
//! Every candidate — baseline or parser-produced — is individually
//! re-validated against `axon-graph`'s closed kind registry
//! ([`axon_graph::candidate::validate_candidate`]) before the batch write:
//! The graph store fails the *whole* transaction on the first invalid
//! candidate, so a single malformed candidate from a parser must
//! not be allowed to also block a source's valid baseline skeleton from
//! landing. Invalid candidates are dropped with a warning (fail-closed at the
//! candidate level), not published.
//!
//! Per the crate-ownership rule, `axon-graph` owns the store, the closed kind
//! registry, and candidate/merge-key/authority validation; this module only
//! assembles and filters [`GraphCandidate`] values and sends one candidate
//! bounded candidate batches through the scheduler-owned reserved-call facade. When no target pool is available (no
//! unified SQLite runtime), the write is skipped and a degraded
//! [`GraphWriteSummary`] with zero counts is returned — acquisition never
//! crashes because of the graph write.

use std::collections::HashSet;
use std::sync::Arc;

use axon_api::source::{
    EnrichmentKind, EnrichmentStatus, GraphCandidate, GraphCandidateProducer, GraphEdgeCandidate,
    GraphEvidence, GraphNodeCandidate, GraphWriteSummary, ItemKind, ManifestItem, MetadataMap,
    SourceId, SourceItemKey, SourceKind, SourceManifest, SourceScope,
};
use axon_graph::candidate::validate_candidate;
use axon_ledger::store::LedgerStore;
use sqlx::SqlitePool;
use tokio::sync::Semaphore;

use super::result_map::IndexCounts;
use crate::context::TargetLocalSourceRuntime;
use crate::reserved_call::ProviderCallContext;

mod baseline;
use baseline::*;
mod publication;
pub(crate) mod staging;

/// Confidence stamped on baseline skeleton nodes/edges. These are structural
/// containment facts derived directly from the acquired manifest, not inferred
/// text mentions, so they carry high confidence.
const BASELINE_CONFIDENCE: f32 = 0.95;
const BASELINE_GRAPH_BATCH_SIZE: usize = 512;
const GRAPH_WRITE_CANDIDATE_BATCH_SIZE: usize = 64;

/// Producer version reported on every baseline candidate.
const PRODUCER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Build and persist the source graph for a completed index: the baseline
/// skeleton plus every parser-produced candidate from this generation.
///
/// Reads the just-published manifest for `counts.source_id`/`counts.generation`
/// from the ledger, assembles one container node + one node/edge per document,
/// unions in `extra_candidates` (already individually validated here so one bad
/// candidate cannot sink the baseline write), runs the minimal `enriching`
/// stage over the valid extras, and upserts everything into the durable graph
/// in one batch. Returns the real [`GraphWriteSummary`] from the store result.
///
/// A missing pool, a missing manifest, or a store error degrades to a zero-count
/// summary (with `degraded = true`) rather than failing the index — the source
/// is already acquired and published by the time this runs.
#[cfg(test)]
pub async fn write_baseline_graph(
    kind: SourceKind,
    pool: Option<Arc<SqlitePool>>,
    ledger: &dyn LedgerStore,
    counts: &IndexCounts,
    canonical_uri: &str,
    extra_candidates: Vec<GraphCandidate>,
) -> GraphWriteSummary {
    write_baseline_graph_with_db_gate(
        None,
        None,
        kind,
        pool,
        ledger,
        counts,
        canonical_uri,
        None,
        extra_candidates,
        None,
    )
    .await
}

pub(crate) async fn write_baseline_graph_with_db_gate(
    runtime: Option<&TargetLocalSourceRuntime>,
    graph_context: Option<ProviderCallContext>,
    kind: SourceKind,
    pool: Option<Arc<SqlitePool>>,
    ledger: &dyn LedgerStore,
    counts: &IndexCounts,
    canonical_uri: &str,
    published_manifest: Option<SourceManifest>,
    extra_candidates: Vec<GraphCandidate>,
    db_stage_slots: Option<Arc<Semaphore>>,
) -> GraphWriteSummary {
    let operation = write_baseline_graph_inner(
        runtime,
        graph_context,
        kind,
        pool,
        ledger,
        counts,
        canonical_uri,
        published_manifest,
        extra_candidates,
        db_stage_slots,
    );
    if let Some(runtime) = runtime {
        return lease::with_source_lease(
            runtime.ledger.clone(),
            counts.source_id.clone(),
            counts.job_id,
            async {
                if runtime
                    .ledger
                    .committed_generation(counts.source_id.clone())
                    .await?
                    .as_ref()
                    != Some(&counts.generation)
                {
                    return Ok(degraded_summary());
                }
                Ok(operation.await)
            },
        )
        .await
        .unwrap_or_else(|_| degraded_summary());
    }
    operation.await
}

pub(crate) async fn write_baseline_graph_inner(
    runtime: Option<&TargetLocalSourceRuntime>,
    graph_context: Option<ProviderCallContext>,
    kind: SourceKind,
    pool: Option<Arc<SqlitePool>>,
    ledger: &dyn LedgerStore,
    counts: &IndexCounts,
    canonical_uri: &str,
    published_manifest: Option<SourceManifest>,
    extra_candidates: Vec<GraphCandidate>,
    db_stage_slots: Option<Arc<Semaphore>>,
) -> GraphWriteSummary {
    let Some(pool) = pool else {
        tracing::debug!("no unified sqlite pool; skipping baseline graph write");
        return degraded_summary();
    };

    let mut manifest = if let Some(manifest) = published_manifest {
        manifest
    } else {
        match ledger
            .get_manifest(counts.source_id.clone(), counts.generation.clone())
            .await
        {
            Ok(Some(manifest)) => manifest,
            Ok(None) => {
                tracing::debug!(
                    source_id = %counts.source_id.0,
                    generation = %counts.generation.0,
                    "no manifest for indexed generation; skipping baseline graph write"
                );
                return degraded_summary();
            }
            Err(err) => {
                tracing::warn!(
                    error = %err.message,
                    source_id = %counts.source_id.0,
                    "failed to read manifest for baseline graph; skipping"
                );
                return degraded_summary();
            }
        }
    };

    if runtime.is_some()
        && let Err(error) = lease::filter_skipped(ledger, &mut manifest).await
    {
        tracing::warn!(error = %error, "cannot establish graph item dispositions");
        return degraded_summary();
    }

    // Enriching-stage observability is derived directly from the borrowed
    // candidates. Do not clone the generation's graph payload into a temporary
    // SourceEnrichment merely to count it before the real graph write.
    let (valid_extras, rejected) = filter_valid_candidates(extra_candidates, &counts.source_id);
    let parse_hint_count = valid_extras
        .iter()
        .filter_map(|candidate| candidate.producer.parser.as_deref())
        .collect::<HashSet<_>>()
        .len();
    let (enrichment_kind, enrichment_status) = if valid_extras.is_empty() {
        (EnrichmentKind::None, EnrichmentStatus::NotNeeded)
    } else {
        (EnrichmentKind::Extraction, EnrichmentStatus::Completed)
    };
    tracing::info!(
        source_id = %counts.source_id.0,
        enrichment_kind = ?enrichment_kind,
        enrichment_status = ?enrichment_status,
        parse_hints = parse_hint_count,
        graph_candidates = valid_extras.len(),
        "enriching stage validated parser-produced graph candidates"
    );

    let baseline_batch_count = manifest
        .items
        .len()
        .max(1)
        .div_ceil(BASELINE_GRAPH_BATCH_SIZE);
    let candidates =
        baseline_candidates(kind, counts, canonical_uri, &manifest).chain(valid_extras);

    let _db_permit = match db_stage_slots {
        Some(slots) => match slots.acquire_owned().await {
            Ok(permit) => Some(permit),
            Err(_) => {
                tracing::warn!(
                    source_id = %counts.source_id.0,
                    "source database-stage admission gate closed before graph upsert"
                );
                return degraded_summary();
            }
        },
        None => None,
    };
    let write = publication::upsert_candidate_batches(
        runtime,
        graph_context,
        &pool,
        candidates,
        GRAPH_WRITE_CANDIDATE_BATCH_SIZE,
    )
    .await;
    match write {
        Ok(result) => GraphWriteSummary {
            // Every baseline chunk repeats the container node so its edges can
            // resolve locally. Hide those implementation-only repeat upserts
            // from the public summary.
            nodes_upserted: result
                .nodes_upserted
                .saturating_sub(baseline_batch_count.saturating_sub(1) as u64),
            edges_upserted: result.edges_upserted,
            evidence_records: result.evidence_records,
            degraded: rejected != 0 || has_incomplete_extraction(counts),
        },
        Err(err) => {
            tracing::warn!(
                error = %err.message,
                source_id = %counts.source_id.0,
                "baseline graph upsert failed; returning degraded summary"
            );
            degraded_summary()
        }
    }
}

/// Re-validate every extra (parser-produced) candidate against `axon-graph`'s
/// closed kind registry before it enters the write batch. `upsert_candidates`
/// fails the whole batch on the first invalid candidate, so filtering here —
/// fail-closed at the *candidate* level, not the whole index — keeps one
/// malformed parser candidate from also blocking the baseline skeleton write.
/// axon-parse already sanitizes at parse time (`validate::sanitize_result`);
/// this is the write path's own gate, so it never trusts an upstream caller to
/// have done so.
fn filter_valid_candidates(
    candidates: Vec<GraphCandidate>,
    source_id: &SourceId,
) -> (Vec<GraphCandidate>, u64) {
    let before = candidates.len();
    let valid: Vec<_> = candidates
        .into_iter()
        .filter(|candidate| match validate_candidate(candidate) {
            Ok(()) => true,
            Err(err) => {
                tracing::warn!(
                    source_id = %source_id.0,
                    candidate_id = %candidate.candidate_id,
                    error = %err.message,
                    "dropping invalid graph candidate before write"
                );
                false
            }
        })
        .collect();
    let rejected = (before - valid.len()) as u64;
    (valid, rejected)
}

/// A degraded, no-write summary. Used whenever the graph write is skipped or
/// fails so the source result still reports a truthful `degraded` flag.
fn degraded_summary() -> GraphWriteSummary {
    GraphWriteSummary {
        nodes_upserted: 0,
        edges_upserted: 0,
        evidence_records: 0,
        degraded: true,
    }
}

#[cfg(test)]
#[path = "graph_tests.rs"]
mod tests;

pub(crate) mod lease;

pub(super) fn has_incomplete_extraction(counts: &IndexCounts) -> bool {
    counts.warnings.iter().any(|warning| {
        matches!(
            warning.code.as_str(),
            "parse.graph_evidence_rejected" | "graph.candidates_rejected"
        )
    })
}
