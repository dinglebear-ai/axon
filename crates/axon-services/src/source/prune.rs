//! Cleanup-debt drain for `index_source`.
//!
//! After a source generation is committed, `axon-ledger` has recorded
//! [`CleanupDebt`] rows for every superseded item — vector points that belong to
//! the *previous* generation and are now stale (their point ids embed the old
//! generation, so a re-index writes fresh points and leaves the old ones behind).
//! This module drains that debt: it reads the source's pending debt, runs the
//! real [`axon_prune::PruneExecutor`] against the relevant store boundary, and
//! marks each resolved entry in the ledger.
//!
//! Per the pruning contract, deletes are generation-fenced: the executor refuses
//! to delete the *current committed* generation by accident. The committed
//! generation for the just-published source is passed in as the fence for
//! `Vector`/`Ledger` steps. `Graph`/`Memory` steps are identity-scoped (stable
//! keys / memory ids), not generation-fenced.
//!
//! Every debt kind this module can drain — `VectorDelete`, `LedgerPrune`,
//! `GraphPrune`, `MemoryPrune`, `JobRetention` — now routes through the single
//! [`axon_prune::PruneExecutor::execute`] call in [`drain_via_executor`], using
//! the identity carried on [`PruneStep`] (`vector_selector` /
//! `source_id`+`generation` / `graph_stable_keys`+`graph_edge_ids` /
//! `memory_ids`) or, for `JobRetention` (whose `job_ids` identity has no
//! matching `PruneStep` field — see `step_map::debt_to_step`'s doc comment),
//! a per-debt field on [`LedgerPruneTarget`] itself. There is no direct-store
//! fallback: a debt kind whose store is not wired for this call fails closed
//! (the executor reports the step `Failed`, debt stays pending) rather than
//! fake-resolving.
//!
//! Failure degrades gracefully — a delete error, an unfenced-current
//! collision, or a ledger error is logged and leaves the debt row pending for a
//! later retry. Acquisition never crashes because of a cleanup failure: the
//! source is already acquired, embedded, and published by the time this runs.
//!
//! ## Authorization
//!
//! The pruning contract requires `axon:admin` for any destructive execution
//! (`docs/pipeline-unification/runtime/pruning-contract.md`, "Safety Rules").
//! This drain is **not** a user-invoked "delete my data" request — it is
//! trusted, in-process, system-triggered maintenance that always runs
//! immediately after `index_source` publishes a new generation, regardless of
//! the caller's own scopes. It is therefore pre-authorized as system-trusted
//! (mirroring `AuthSnapshot::trusted_system` used elsewhere for
//! system-triggered work), but that authorization is passed **explicitly** at
//! this call site via [`PruneAuthz::admin`] rather than silently bypassing the
//! [`PruneExecutor::execute`] admin gate. The gate still runs on every call —
//! it just always resolves to "authorized" for this specific, audited,
//! system-owned path.

use async_trait::async_trait;
use axon_adapters::SourceAdapterRegistry;
use axon_api::source::{
    AdapterReleaseRequest, ArtifactHandle, ArtifactKind, CleanupDebt, CleanupDebtKind,
    CleanupSelector, DocumentCacheInvalidation, DocumentCacheKey, GraphDeleteResult, GraphEdgeId,
    JobId, MemoryForgetRequest, ProviderId, Severity, SourceError, SourceGenerationId, SourceId,
    Timestamp, VectorDeleteSelector, VectorStoreDeleteResult,
};
use axon_core::boundary::DocumentCache;
use axon_jobs::boundary::{JobDeleteResult, JobStore};
use axon_ledger::store::LedgerStore;
use axon_memory::store::MemoryStore;
use axon_prune::{
    PruneAuthz, PruneExecutor, PruneStep, PruneTarget, PruneTargetKind, StepExecution,
};
use std::collections::{BTreeMap, BTreeSet};

use super::result_map::IndexCounts;

mod drain_ops;
mod step_map;

use drain_ops::*;
use step_map::{debt_to_step, job_ids_for_debt, single_step_plan, skip_reason_for_kind};

/// Outcome of a cleanup-debt drain pass (for logging only — never surfaced on
/// the wire).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct DebtDrainSummary {
    /// Debt entries whose steps all resolved and were marked completed.
    pub resolved: u64,
    /// Debt entries left pending (delete failed, fenced, or resolve failed).
    pub failed: u64,
    /// Vector points actually deleted across all resolved entries.
    pub points_deleted: u64,
    /// The ledger could not enumerate pending debt, so cleanup work may remain
    /// even though no individual debt rows were observed.
    pub enumeration_failed: bool,
}

/// Provider operations needed by cleanup. Production implements this at the
/// reserved-call boundary so every destructive call participates in its
/// scheduler; prune remains provider-handle-neutral.
#[async_trait]
pub(crate) trait CleanupProviderOps: Send + Sync {
    async fn vector_delete(
        &self,
        selector: VectorDeleteSelector,
    ) -> Result<VectorStoreDeleteResult, axon_api::source::ApiError>;
    async fn graph_delete_nodes(
        &self,
        stable_keys: Vec<String>,
    ) -> Result<GraphDeleteResult, axon_api::source::ApiError>;
    async fn graph_delete_edges(
        &self,
        edge_ids: Vec<GraphEdgeId>,
    ) -> Result<GraphDeleteResult, axon_api::source::ApiError>;
    async fn artifact_delete(
        &self,
        handle: ArtifactHandle,
    ) -> Result<(), axon_api::source::ApiError>;
}

/// Attach the request's exact collection to publish-created vector debt before
/// its first drain. Once set, the identity cannot be changed by a later call.
pub(crate) async fn bind_vector_cleanup_collection(
    ledger: &dyn LedgerStore,
    source_id: &SourceId,
    collection: &str,
) -> axon_ledger::store::Result<()> {
    if collection.trim().is_empty() {
        return Err(axon_api::source::ApiError::new(
            "source.cleanup.vector_collection_missing",
            axon_api::source::ErrorStage::Cleaning,
            "vector cleanup collection identity must not be empty",
        ));
    }
    for mut debt in ledger.list_pending_cleanup_debt(source_id.clone()).await? {
        if debt.kind != CleanupDebtKind::VectorDelete {
            continue;
        }
        match debt.vector_collection.as_deref() {
            Some(existing) if existing != collection => {
                return Err(axon_api::source::ApiError::new(
                    "source.cleanup.vector_collection_immutable",
                    axon_api::source::ErrorStage::Cleaning,
                    "vector cleanup debt is already bound to a different collection",
                )
                .with_source_id(source_id.0.clone()));
            }
            Some(_) => continue,
            None => debt.vector_collection = Some(collection.to_string()),
        }
        ledger.record_cleanup_debt(debt).await?;
    }
    Ok(())
}

/// Sweep unresolved cleanup debt across all sources in bounded, stable pages.
///
/// Each source is fenced against the generation currently committed in the
/// ledger at sweep time. A failed provider operation remains pending and is
/// retried by the next periodic sweep without requiring another publication.
pub(crate) async fn drain_all_cleanup_debt<F, Fut>(
    ledger: &dyn LedgerStore,
    page_size: usize,
    mut drain_source: F,
) -> DebtDrainSummary
where
    F: FnMut(IndexCounts) -> Fut,
    Fut: std::future::Future<Output = DebtDrainSummary>,
{
    let mut cursor = None;
    let mut visited_sources = BTreeSet::new();
    let mut total = DebtDrainSummary::default();
    loop {
        let page = match ledger
            .list_pending_cleanup_debt_after(cursor.clone(), page_size.max(1))
            .await
        {
            Ok(page) => page,
            Err(error) => {
                tracing::warn!(error = %error.message, "failed to page cleanup debt");
                total.failed += 1;
                break;
            }
        };
        let Some(last) = page.last() else { break };
        cursor = Some(last.debt_id.clone());

        for debt in page {
            if !visited_sources.insert(debt.source_id.clone()) {
                continue;
            }
            let committed = match ledger.committed_generation(debt.source_id.clone()).await {
                Ok(Some(generation)) => generation,
                Ok(None) => {
                    tracing::warn!(source_id = %debt.source_id.0, "cleanup debt has no committed generation; leaving pending");
                    total.failed += 1;
                    continue;
                }
                Err(error) => {
                    tracing::warn!(source_id = %debt.source_id.0, error = %error.message, "failed to read cleanup generation fence");
                    total.failed += 1;
                    continue;
                }
            };
            let counts = IndexCounts {
                job_id: debt.job_id,
                source_id: debt.source_id,
                generation: committed,
                items_discovered: 0,
                documents_prepared: 0,
                chunks_prepared: 0,
                vector_points_written: 0,
                removed: 0,
                published_manifest: None,
                graph_candidates: Vec::new(),
                warnings: Vec::new(),
                artifacts: Vec::new(),
                inline: None,
            };
            let summary = drain_source(counts).await;
            total.resolved += summary.resolved;
            total.failed += summary.failed;
            total.points_deleted += summary.points_deleted;
        }
    }
    total
}

fn pending_debt_or_degraded(
    pending: Result<Vec<CleanupDebt>, axon_api::source::ApiError>,
) -> Result<Vec<CleanupDebt>, (axon_api::source::ApiError, DebtDrainSummary)> {
    pending.map_err(|error| {
        (
            error,
            DebtDrainSummary {
                enumeration_failed: true,
                ..DebtDrainSummary::default()
            },
        )
    })
}

#[cfg(test)]
pub(crate) async fn drain_due_adapter_releases(
    ledger: &dyn LedgerStore,
    registry: &SourceAdapterRegistry,
    limit: usize,
) -> Option<Timestamp> {
    let debts = ledger.list_adapter_release_debt(limit).await.ok()?;
    let now = Timestamp::from(chrono::Utc::now());
    let mut summary = DebtDrainSummary::default();
    for debt in debts.iter().filter(|debt| {
        debt.next_retry_at
            .as_ref()
            .is_none_or(|retry| retry.0 <= now.0)
    }) {
        drain_adapter_release(ledger, Some(registry), debt, &mut summary).await;
    }
    ledger
        .list_adapter_release_debt(limit)
        .await
        .ok()?
        .into_iter()
        .filter_map(|debt| debt.next_retry_at)
        .filter(|retry| retry.0 > now.0)
        .min_by(|a, b| a.0.cmp(&b.0))
}

/// Drain pending cleanup debt for the just-published source.
///
/// Reads the source's pending debt from the ledger, executes each entry's
/// generation-fenced vector delete via the prune executor, and marks resolved
/// entries in the ledger. `committed_generation` (the newly published
/// generation) is the fence: no delete may target it.
///
/// Never returns an error — every failure path logs and degrades to leaving the
/// debt pending, so a cleanup problem cannot fail an already-committed index.
///
/// This is the vector-only entry point; prefer [`drain_cleanup_debt_full`] so
/// `GraphPrune`/`MemoryPrune` debt also drains when a `GraphStore`/
/// `MemoryStore` are available.
/// Full cleanup-debt drain: vector, ledger, graph, and memory boundaries.
///
/// `graph_store`/`memory_store` are optional — when `None`, `GraphPrune`/
/// `MemoryPrune` debt is left pending (the executor step fails closed with
/// "no store wired", never faked as resolved), matching the "no fake drains"
/// requirement in `docs/pipeline-unification/runtime/pruning-contract.md`.
///
/// This is the job-store-unaware entry point (`job_store` is always `None`,
/// so any `JobRetention` debt fails closed exactly like an unwired
/// `GraphStore`/`MemoryStore`) — kept so existing call sites' signatures stay
/// untouched. Prefer [`drain_cleanup_debt_full_with_jobs`] once the caller
/// has a `JobStore` handle available to drain `JobRetention` debt too.
/// Full cleanup-debt drain across every boundary this module can drive:
/// vector, ledger, graph, memory, and job-retention.
///
/// `graph_store`/`memory_store`/`job_store` are each optional — when `None`,
/// that boundary's debt kind is left pending (the executor step fails closed
/// with "no store wired", never faked as resolved), matching the "no fake
/// drains" requirement in
/// `docs/pipeline-unification/runtime/pruning-contract.md`.
///
/// Unlike `Vector`/`Ledger`/`Graph`/`Memory` identity, a `JobRetention`
/// debt's `job_ids` (from `CleanupSelector::JobRows`) have no matching field
/// on the transport-neutral `PruneStep` DTO, so [`LedgerPruneTarget`] is
/// (re)constructed once per debt (cheap — every field but `job_ids` is an
/// unchanged reference/clone) rather than once for the whole batch, purely so
/// it can carry that one debt's job ids into `apply()`.
/// Full cleanup-debt drain plus direct core boundaries for artifact/cache debt.
///
/// `ArtifactDelete` and `CachePrune` do not currently have `PruneStep` identity
/// fields, so they drain directly against `ArtifactStore` / `DocumentCache`
/// when supplied. If the relevant boundary is absent, the debt stays pending.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn drain_cleanup_debt_with_provider_ops(
    ledger: &dyn LedgerStore,
    provider_ops: &dyn CleanupProviderOps,
    memory_store: Option<&dyn MemoryStore>,
    job_store: Option<&dyn JobStore>,
    document_cache: Option<&dyn DocumentCache>,
    adapter_registry: Option<&SourceAdapterRegistry>,
    collection: &str,
    counts: &IndexCounts,
) -> DebtDrainSummary {
    let source_id = counts.source_id.clone();
    let committed_generation = counts.generation.clone();

    let pending =
        match pending_debt_or_degraded(ledger.list_pending_cleanup_debt(source_id.clone()).await) {
            Ok(pending) => pending,
            Err((error, summary)) => {
                tracing::warn!(
                    error = %error.message,
                    source_id = %source_id.0,
                    "failed to list pending cleanup debt; skipping drain"
                );
                return summary;
            }
        };
    if pending.is_empty() {
        return DebtDrainSummary::default();
    }

    // System-trusted authorization for this automatic, in-process cleanup
    // drain — see the module-level "Authorization" note. Passed explicitly
    // (never implicitly defaulted) so the executor's admin gate is exercised
    // and the authorization decision is visible at the call site.
    let authz = PruneAuthz::admin();

    let mut summary = DebtDrainSummary::default();
    let mut vector_groups: BTreeMap<(String, String), Vec<CleanupDebt>> = BTreeMap::new();
    let mut other_debts = Vec::new();
    for debt in pending {
        if debt.kind == CleanupDebtKind::VectorDelete {
            if let Some((source_id, generation)) = vector_debt_scope(&debt) {
                vector_groups
                    .entry((source_id.0, generation.0))
                    .or_default()
                    .push(debt);
                continue;
            }
        }
        other_debts.push(debt);
    }

    // Publishing records item-scoped vector debt, but vector cleanup is
    // deliberately generation-wide. Retain one representative per generation
    // for execution and remember the sibling rows that delete also covers.
    let mut debts_to_drain = Vec::new();
    let mut vector_siblings: BTreeMap<String, Vec<CleanupDebt>> = BTreeMap::new();
    for debts in vector_groups.into_values() {
        let mut debts = debts.into_iter();
        let Some(representative) = debts.next() else {
            continue;
        };
        vector_siblings.insert(representative.debt_id.0.clone(), debts.collect());
        debts_to_drain.push(representative);
    }
    debts_to_drain.extend(other_debts);

    for debt in debts_to_drain {
        let target = LedgerPruneTarget {
            provider_ops,
            ledger,
            memory_store,
            job_store,
            source_id: source_id.clone(),
            committed_generation: committed_generation.clone(),
            job_ids: job_ids_for_debt(&debt),
        };
        let executor = PruneExecutor::new(target);
        if let Some(siblings) = vector_siblings.remove(&debt.debt_id.0) {
            let resolved_before = summary.resolved;
            drain_via_executor(ledger, &executor, &authz, &debt, &mut summary).await;
            if summary.resolved > resolved_before {
                for sibling in &siblings {
                    resolve_debt(ledger, sibling, &mut summary).await;
                }
            }
        } else {
            drain_one_debt(
                ledger,
                &executor,
                &authz,
                &debt,
                collection,
                provider_ops,
                document_cache,
                adapter_registry,
                &mut summary,
            )
            .await;
        }
    }

    tracing::debug!(
        source_id = %source_id.0,
        resolved = summary.resolved,
        failed = summary.failed,
        points_deleted = summary.points_deleted,
        "cleanup debt drain complete"
    );
    summary
}

fn vector_debt_scope(debt: &CleanupDebt) -> Option<(SourceId, SourceGenerationId)> {
    match &debt.selector {
        CleanupSelector::SourceItem {
            source_id,
            generation,
            ..
        }
        | CleanupSelector::Generation {
            source_id,
            generation,
        } => Some((source_id.clone(), generation.clone())),
        CleanupSelector::Source { source_id } => debt
            .generation
            .clone()
            .map(|generation| (source_id.clone(), generation)),
        _ => None,
    }
}

#[cfg(test)]
#[path = "prune_tests.rs"]
mod tests;
