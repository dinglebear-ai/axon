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
    CleanupSelector, DocumentCacheInvalidation, DocumentCacheKey, JobId, MemoryForgetRequest,
    ProviderId, Severity, SourceError, SourceGenerationId, SourceId, Timestamp,
    VectorDeleteSelector,
};
use axon_core::boundary::{ArtifactStore, DocumentCache};
use axon_graph::store::GraphStore;
use axon_jobs::boundary::{JobDeleteResult, JobStore};
use axon_ledger::store::LedgerStore;
use axon_memory::store::MemoryStore;
use axon_prune::{
    PruneAuthz, PruneExecutor, PruneStep, PruneTarget, PruneTargetKind, StepExecution,
};
use axon_vectors::store::VectorStore;
use std::collections::BTreeMap;

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
}

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
pub async fn drain_cleanup_debt(
    ledger: &dyn LedgerStore,
    vector_store: &dyn VectorStore,
    collection: &str,
    counts: &IndexCounts,
) -> DebtDrainSummary {
    drain_cleanup_debt_full_with_boundaries(
        ledger,
        vector_store,
        None,
        None,
        None,
        None,
        None,
        None,
        collection,
        counts,
    )
    .await
}

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
pub async fn drain_cleanup_debt_full(
    ledger: &dyn LedgerStore,
    vector_store: &dyn VectorStore,
    graph_store: Option<&dyn GraphStore>,
    memory_store: Option<&dyn MemoryStore>,
    collection: &str,
    counts: &IndexCounts,
) -> DebtDrainSummary {
    drain_cleanup_debt_full_with_boundaries(
        ledger,
        vector_store,
        graph_store,
        memory_store,
        None,
        None,
        None,
        None,
        collection,
        counts,
    )
    .await
}

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
pub async fn drain_cleanup_debt_full_with_jobs(
    ledger: &dyn LedgerStore,
    vector_store: &dyn VectorStore,
    graph_store: Option<&dyn GraphStore>,
    memory_store: Option<&dyn MemoryStore>,
    job_store: Option<&dyn JobStore>,
    collection: &str,
    counts: &IndexCounts,
) -> DebtDrainSummary {
    drain_cleanup_debt_full_with_boundaries(
        ledger,
        vector_store,
        graph_store,
        memory_store,
        job_store,
        None,
        None,
        None,
        collection,
        counts,
    )
    .await
}

/// Full cleanup-debt drain plus direct core boundaries for artifact/cache debt.
///
/// `ArtifactDelete` and `CachePrune` do not currently have `PruneStep` identity
/// fields, so they drain directly against `ArtifactStore` / `DocumentCache`
/// when supplied. If the relevant boundary is absent, the debt stays pending.
#[allow(clippy::too_many_arguments)]
pub async fn drain_cleanup_debt_full_with_boundaries(
    ledger: &dyn LedgerStore,
    vector_store: &dyn VectorStore,
    graph_store: Option<&dyn GraphStore>,
    memory_store: Option<&dyn MemoryStore>,
    job_store: Option<&dyn JobStore>,
    artifact_store: Option<&dyn ArtifactStore>,
    document_cache: Option<&dyn DocumentCache>,
    adapter_registry: Option<&SourceAdapterRegistry>,
    collection: &str,
    counts: &IndexCounts,
) -> DebtDrainSummary {
    let source_id = counts.source_id.clone();
    let committed_generation = counts.generation.clone();

    let pending = match ledger.list_pending_cleanup_debt(source_id.clone()).await {
        Ok(pending) => pending,
        Err(err) => {
            tracing::warn!(
                error = %err.message,
                source_id = %source_id.0,
                "failed to list pending cleanup debt; skipping drain"
            );
            return DebtDrainSummary::default();
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
            vector_store,
            ledger,
            graph_store,
            memory_store,
            job_store,
            collection: collection.to_string(),
            source_id: source_id.clone(),
            committed_generation: committed_generation.clone(),
            job_ids: job_ids_for_debt(&debt),
        };
        let executor = PruneExecutor::new(target);
        if let Some(siblings) = vector_siblings.remove(&debt.debt_id.0) {
            let resolved_before = summary.resolved;
            drain_via_executor(ledger, &executor, &authz, &debt, collection, &mut summary).await;
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
                artifact_store,
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
