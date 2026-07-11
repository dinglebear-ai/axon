//! Cleanup-debt drain for `index_source`.
//!
//! After a source generation is committed, `axon-ledger` has recorded
//! [`CleanupDebt`] rows for every superseded item — vector points that belong to
//! the *previous* generation and are now stale (their point ids embed the old
//! generation, so a re-index writes fresh points and leaves the old ones behind).
//! This module drains that debt: it reads the source's pending debt, runs the
//! real [`axon_prune::PruneExecutor`] against the [`VectorStore`] with
//! generation-fenced deletes, and marks each resolved entry in the ledger.
//!
//! Per the pruning contract, deletes are generation-fenced: the executor refuses
//! to delete the *current committed* generation by accident. The committed
//! generation for the just-published source is passed in as the fence.
//!
//! Failure degrades gracefully — a vector delete error, an unfenced-current
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
use axon_api::source::{
    CleanupDebt, CleanupDebtKind, CleanupSelector, JobId, MemoryForgetRequest, SourceGenerationId,
    SourceId, Timestamp, VectorDeleteSelector,
};
use axon_graph::store::GraphStore;
use axon_ledger::store::LedgerStore;
use axon_memory::store::MemoryStore;
use axon_prune::{
    PruneAuthz, PruneExecutor, PrunePlan, PruneStep, PruneTarget, PruneTargetKind, StepExecution,
};
use axon_vectors::store::VectorStore;
use uuid::Uuid;

use super::result_map::IndexCounts;

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
/// This is the vector-only entry point `index_source` calls today. Use
/// [`drain_cleanup_debt_full`] to also drain `GraphPrune`/`MemoryPrune` debt
/// when a `GraphStore`/`MemoryStore` are available — wiring the real stores
/// into the `index_source` call site is a followup for whichever fleet owns
/// `crates/axon-services/src/source.rs` (out of this module's territory).
pub async fn drain_cleanup_debt(
    ledger: &dyn LedgerStore,
    vector_store: &dyn VectorStore,
    collection: &str,
    counts: &IndexCounts,
) -> DebtDrainSummary {
    drain_cleanup_debt_full(ledger, vector_store, None, None, collection, counts).await
}

/// Full cleanup-debt drain: vector, ledger, graph, and memory boundaries.
///
/// `graph_store`/`memory_store` are optional — when `None`, `GraphPrune`/
/// `MemoryPrune` debt is left pending with a "no store wired" skip reason
/// (never faked as resolved), matching the "no fake drains" requirement in
/// `docs/pipeline-unification/runtime/pruning-contract.md`.
pub async fn drain_cleanup_debt_full(
    ledger: &dyn LedgerStore,
    vector_store: &dyn VectorStore,
    graph_store: Option<&dyn GraphStore>,
    memory_store: Option<&dyn MemoryStore>,
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

    let target = LedgerPruneTarget {
        vector_store,
        ledger,
        collection: collection.to_string(),
        source_id: source_id.clone(),
        committed_generation,
    };
    let executor = PruneExecutor::new(target);

    // System-trusted authorization for this automatic, in-process cleanup
    // drain — see the module-level "Authorization" note. Passed explicitly
    // (never implicitly defaulted) so the executor's admin gate is exercised
    // and the authorization decision is visible at the call site.
    let authz = PruneAuthz::admin();

    let mut summary = DebtDrainSummary::default();
    for debt in pending {
        drain_one_debt(
            ledger,
            &executor,
            graph_store,
            memory_store,
            &authz,
            &debt,
            &mut summary,
        )
        .await;
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

/// Execute one debt entry and, on clean success, mark it resolved.
async fn drain_one_debt(
    ledger: &dyn LedgerStore,
    executor: &PruneExecutor<LedgerPruneTarget<'_>>,
    graph_store: Option<&dyn GraphStore>,
    memory_store: Option<&dyn MemoryStore>,
    authz: &PruneAuthz,
    debt: &CleanupDebt,
    summary: &mut DebtDrainSummary,
) {
    match debt.kind {
        CleanupDebtKind::VectorDelete | CleanupDebtKind::LedgerPrune => {
            drain_via_executor(ledger, executor, authz, debt, summary).await;
        }
        CleanupDebtKind::GraphPrune => {
            drain_graph_debt(ledger, graph_store, authz, debt, summary).await;
        }
        CleanupDebtKind::MemoryPrune => {
            drain_memory_debt(ledger, memory_store, authz, debt, summary).await;
        }
        CleanupDebtKind::ArtifactDelete
        | CleanupDebtKind::JobRetention
        | CleanupDebtKind::CachePrune => {
            // No real drain available for this kind yet. This is not a
            // "not wired" placeholder — it is a documented gap per kind (see
            // `skip_reason_for_kind`): either the store boundary has no real
            // per-item deletion API, or (for job/cache) the owning crate is
            // out of this module's territory. Faking a drain for any of
            // these would violate the pruning contract's "no fake drains"
            // requirement, so they are left pending for their owning
            // executor until the prerequisite lands.
            tracing::debug!(
                debt_id = %debt.debt_id.0,
                kind = ?debt.kind,
                reason = skip_reason_for_kind(debt.kind),
                "skipping cleanup debt: no real drain available for this kind"
            );
        }
    }
}

/// Drive the `axon-prune` executor for a debt kind whose identity fits
/// `PruneStep`'s existing fields (`Vector`: `vector_selector`; `Ledger`:
/// `source_id`+`generation`).
async fn drain_via_executor(
    ledger: &dyn LedgerStore,
    executor: &PruneExecutor<LedgerPruneTarget<'_>>,
    authz: &PruneAuthz,
    debt: &CleanupDebt,
    summary: &mut DebtDrainSummary,
) {
    let Some(step) = debt_to_step(debt) else {
        tracing::debug!(
            debt_id = %debt.debt_id.0,
            kind = ?debt.kind,
            reason = skip_reason_for_kind(debt.kind),
            "skipping cleanup debt: selector does not carry the identity this kind needs"
        );
        return;
    };

    let plan = single_step_plan(step);
    let result = match executor.execute(&plan, authz).await {
        Ok(result) => result,
        Err(denied) => {
            // Generation fence / admin / confirmation refusal. Leave pending.
            tracing::warn!(
                debt_id = %debt.debt_id.0,
                reason = %denied,
                "cleanup debt delete refused; leaving pending"
            );
            summary.failed += 1;
            return;
        }
    };

    if result.cleanup_debt_remaining > 0 {
        tracing::warn!(
            debt_id = %debt.debt_id.0,
            remaining = result.cleanup_debt_remaining,
            "cleanup debt delete failed partway; leaving pending"
        );
        summary.failed += 1;
        return;
    }

    if let Err(err) = ledger.resolve_cleanup_debt(debt.debt_id.clone()).await {
        tracing::warn!(
            error = %err.message,
            debt_id = %debt.debt_id.0,
            "delete succeeded but failed to mark debt resolved; leaving pending"
        );
        summary.failed += 1;
        return;
    }

    summary.resolved += 1;
    summary.points_deleted += result.deleted_counts.vector_points;
}

/// Drain a `GraphPrune` debt: delete the named graph nodes (and their
/// incident edges) by stable key.
///
/// `GraphStore::delete_nodes` has no per-item identity slot in `PruneStep`
/// (an `axon-prune`/axon-api DTO outside this module's territory) yet, so
/// this calls the store directly rather than through `PruneExecutor`. It
/// still enforces the same admin gate the executor would
/// (`docs/pipeline-unification/runtime/pruning-contract.md`, "destructive
/// prune requires axon:admin") using the same `PruneAuthz` passed at the
/// drain call site, and is idempotent (deleting an already-deleted node is a
/// no-op). Wiring an identity-bearing `PruneStep` variant so this can route
/// through `PruneExecutor::execute()` like the other kinds is a named
/// followup for whichever fleet owns `axon-prune`/`axon-api::source::prune`.
async fn drain_graph_debt(
    ledger: &dyn LedgerStore,
    graph_store: Option<&dyn GraphStore>,
    authz: &PruneAuthz,
    debt: &CleanupDebt,
    summary: &mut DebtDrainSummary,
) {
    let Some(graph_store) = graph_store else {
        tracing::debug!(
            debt_id = %debt.debt_id.0,
            "skipping graph cleanup debt: no GraphStore wired for this drain call"
        );
        return;
    };
    if !authz.is_admin {
        tracing::warn!(
            debt_id = %debt.debt_id.0,
            "graph cleanup debt delete refused: axon:admin required; leaving pending"
        );
        summary.failed += 1;
        return;
    }
    let CleanupSelector::GraphNodes { stable_keys } = &debt.selector else {
        tracing::warn!(
            debt_id = %debt.debt_id.0,
            "graph cleanup debt selector is not GraphNodes; leaving pending"
        );
        summary.failed += 1;
        return;
    };
    match graph_store.delete_nodes(stable_keys.clone()).await {
        Ok(_) => resolve_debt(ledger, debt, summary).await,
        Err(err) => {
            tracing::warn!(
                error = %err.message,
                debt_id = %debt.debt_id.0,
                "graph node delete failed; leaving pending"
            );
            summary.failed += 1;
        }
    }
}

/// Drain a `MemoryPrune` debt: forget the named memory records.
///
/// Same rationale as [`drain_graph_debt`] for calling the store directly
/// instead of through `PruneExecutor` — `MemoryStore::forget` is a real,
/// scoped, idempotent delete, but `PruneStep` has no `MemoryId`-bearing
/// field yet.
async fn drain_memory_debt(
    ledger: &dyn LedgerStore,
    memory_store: Option<&dyn MemoryStore>,
    authz: &PruneAuthz,
    debt: &CleanupDebt,
    summary: &mut DebtDrainSummary,
) {
    let Some(memory_store) = memory_store else {
        tracing::debug!(
            debt_id = %debt.debt_id.0,
            "skipping memory cleanup debt: no MemoryStore wired for this drain call"
        );
        return;
    };
    if !authz.is_admin {
        tracing::warn!(
            debt_id = %debt.debt_id.0,
            "memory cleanup debt delete refused: axon:admin required; leaving pending"
        );
        summary.failed += 1;
        return;
    }
    let CleanupSelector::MemoryRecords { ids } = &debt.selector else {
        tracing::warn!(
            debt_id = %debt.debt_id.0,
            "memory cleanup debt selector is not MemoryRecords; leaving pending"
        );
        summary.failed += 1;
        return;
    };
    for memory_id in ids {
        let request = MemoryForgetRequest {
            memory_id: memory_id.clone(),
            reason: Some(format!("cleanup debt {}", debt.debt_id.0)),
            timestamp: Timestamp(chrono::Utc::now().to_rfc3339()),
        };
        if let Err(err) = memory_store.forget(request).await {
            tracing::warn!(
                error = %err.message,
                debt_id = %debt.debt_id.0,
                memory_id = %memory_id.0,
                "memory forget failed; leaving pending"
            );
            summary.failed += 1;
            return;
        }
    }
    resolve_debt(ledger, debt, summary).await;
}

/// Mark a debt resolved after a successful direct-call drain (graph/memory).
async fn resolve_debt(
    ledger: &dyn LedgerStore,
    debt: &CleanupDebt,
    summary: &mut DebtDrainSummary,
) {
    if let Err(err) = ledger.resolve_cleanup_debt(debt.debt_id.clone()).await {
        tracing::warn!(
            error = %err.message,
            debt_id = %debt.debt_id.0,
            "delete succeeded but failed to mark debt resolved; leaving pending"
        );
        summary.failed += 1;
        return;
    }
    summary.resolved += 1;
}

/// Name the specific, current reason a debt kind cannot be drained yet. Kept
/// in one place so the reasons stay in sync with the prerequisites named in
/// the pruning contract's "Cleanup Debt Execution" section and don't drift
/// into a vague "not wired" blanket excuse.
///
/// Only `ArtifactDelete`/`JobRetention`/`CachePrune` reach this function today
/// — `VectorDelete`/`LedgerPrune` drain via [`drain_via_executor`] and
/// `GraphPrune`/`MemoryPrune` via their direct-call drains in
/// [`drain_one_debt`].
///
/// Followups (tracked against out-of-territory crates, not yet beaded
/// individually):
/// - `ArtifactDelete`: no durable `ArtifactStore` exists in this codebase yet
///   (see `docs/pipeline-unification/runtime/pruning-contract.md`'s "artifact
///   deletes" ownership row) — there is nothing to call.
/// - `JobRetention`: `axon-jobs`' `cleanup_jobs`/`clear_jobs` are real but are
///   bulk, age/kind-scoped operations, and `axon-jobs` is out of this
///   module's territory — draining `CleanupSelector::JobRows { job_ids }`
///   needs a per-job-id delete added there.
/// - `CachePrune`: no `CacheStore` boundary exists in this codebase.
fn skip_reason_for_kind(kind: CleanupDebtKind) -> &'static str {
    match kind {
        CleanupDebtKind::VectorDelete
        | CleanupDebtKind::LedgerPrune
        | CleanupDebtKind::GraphPrune
        | CleanupDebtKind::MemoryPrune => "drained (should not reach the skip path)",
        CleanupDebtKind::ArtifactDelete => "no ArtifactStore exists yet",
        CleanupDebtKind::JobRetention => {
            "axon-jobs cleanup is bulk age/kind-scoped; no per-job-id delete exists (out of territory)"
        }
        CleanupDebtKind::CachePrune => "no CacheStore boundary exists yet",
    }
}

/// Map a `VectorDelete`/`LedgerPrune` cleanup-debt entry to a single prune
/// step that fits `PruneStep`'s existing `vector_selector`/`source_id`+
/// `generation` fields. Returns `None` for any other kind, or when the
/// selector doesn't carry the identity the kind needs.
fn debt_to_step(debt: &CleanupDebt) -> Option<PruneStep> {
    match debt.kind {
        CleanupDebtKind::VectorDelete => {
            let (source_id, generation) = debt_scope(debt)?;
            Some(PruneStep {
                target: PruneTargetKind::Vector,
                description: format!(
                    "delete superseded vector points for debt {}",
                    debt.debt_id.0
                ),
                estimated_deletes: 1,
                vector_selector: Some(VectorDeleteSelector::Generation {
                    collection: "axon".to_string(),
                    source_id: source_id.clone(),
                    generation: generation.clone(),
                }),
                source_id: Some(source_id),
                generation: Some(generation),
            })
        }
        CleanupDebtKind::LedgerPrune => {
            let CleanupSelector::LedgerGenerations {
                source_id,
                up_to_generation,
            } = &debt.selector
            else {
                return None;
            };
            Some(PruneStep {
                target: PruneTargetKind::Ledger,
                description: format!(
                    "delete superseded ledger generation rows for debt {}",
                    debt.debt_id.0
                ),
                estimated_deletes: 1,
                vector_selector: None,
                source_id: Some(source_id.clone()),
                generation: Some(up_to_generation.clone()),
            })
        }
        _ => None,
    }
}

/// Extract the `(source_id, superseded_generation)` a vector-delete debt names.
/// The generation is the *previous* (now stale) generation the debt targets.
fn debt_scope(debt: &CleanupDebt) -> Option<(SourceId, SourceGenerationId)> {
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
        // A source-wide vector delete carries the generation on the debt row.
        CleanupSelector::Source { source_id } => debt
            .generation
            .clone()
            .map(|generation| (source_id.clone(), generation)),
        // Document/Chunk/Artifact selectors are not generation-fenced source
        // deletes; leave them to their owning executor.
        _ => None,
    }
}

/// Wrap a step in a minimal, execution-ordered plan for the executor.
fn single_step_plan(step: PruneStep) -> PrunePlan {
    PrunePlan {
        job_id: JobId::new(Uuid::new_v4()),
        selector: axon_api::source::prune::PruneSelector::Generation {
            source_id: step.source_id.clone().unwrap_or_else(|| SourceId::new("")),
            generation: step
                .generation
                .clone()
                .unwrap_or_else(|| SourceGenerationId::new("")),
        },
        destructive: true,
        requires_admin: true,
        estimated: Default::default(),
        steps: vec![step],
        warnings: Vec::new(),
    }
}

/// [`PruneTarget`] backed by the real vector store and ledger. Deletes are
/// scoped to the debt's superseded generation and fenced against the
/// committed generation.
struct LedgerPruneTarget<'a> {
    vector_store: &'a dyn VectorStore,
    ledger: &'a dyn LedgerStore,
    collection: String,
    source_id: SourceId,
    committed_generation: SourceGenerationId,
}

#[async_trait]
impl PruneTarget for LedgerPruneTarget<'_> {
    async fn current_generation(&self, _source_id: Option<&str>) -> Option<SourceGenerationId> {
        // The committed generation is the fence for every step in this drain —
        // all steps belong to the one source just published.
        Some(self.committed_generation.clone())
    }

    async fn apply(&self, step: &PruneStep) -> Result<StepExecution, String> {
        let Some(generation) = &step.generation else {
            return Ok(StepExecution::skipped("no generation on step"));
        };
        // Defensive: never delete the committed generation even if fencing was
        // bypassed. The executor already fences, this is belt-and-suspenders.
        if generation == &self.committed_generation {
            return Ok(StepExecution::skipped(
                "refusing to delete committed generation",
            ));
        }
        match step.target {
            PruneTargetKind::Vector => {
                let deleted = self
                    .vector_store
                    .delete(VectorDeleteSelector::Generation {
                        collection: self.collection.clone(),
                        source_id: self.source_id.clone(),
                        generation: generation.clone(),
                    })
                    .await
                    .map_err(|err| err.message.clone())?;
                Ok(StepExecution::deleted(deleted.points_deleted))
            }
            PruneTargetKind::Ledger => {
                let source_id = step
                    .source_id
                    .clone()
                    .unwrap_or_else(|| self.source_id.clone());
                let deleted = self
                    .ledger
                    .delete_generation(source_id, generation.clone())
                    .await
                    .map_err(|err| err.message.clone())?;
                Ok(StepExecution::deleted(deleted))
            }
            other => Ok(StepExecution::skipped(format!(
                "unsupported prune target for this drain: {other:?}"
            ))),
        }
    }
}

#[cfg(test)]
#[path = "prune_tests.rs"]
mod tests;
