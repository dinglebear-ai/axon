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
use axon_api::source::{
    CleanupDebt, CleanupDebtKind, JobId, MemoryForgetRequest, SourceGenerationId, SourceId,
    Timestamp, VectorDeleteSelector,
};
use axon_graph::store::GraphStore;
use axon_jobs::boundary::{JobDeleteResult, JobStore};
use axon_ledger::store::LedgerStore;
use axon_memory::store::MemoryStore;
use axon_prune::{
    PruneAuthz, PruneExecutor, PruneStep, PruneTarget, PruneTargetKind, StepExecution,
};
use axon_vectors::store::VectorStore;

use super::result_map::IndexCounts;

mod step_map;
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
    drain_cleanup_debt_full(ledger, vector_store, None, None, collection, counts).await
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
    drain_cleanup_debt_full_with_jobs(
        ledger,
        vector_store,
        graph_store,
        memory_store,
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
    for debt in pending {
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
        drain_one_debt(ledger, &executor, &authz, &debt, &mut summary).await;
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

/// Execute one debt entry and, on clean success, mark it resolved. Every
/// drainable kind (`VectorDelete`/`LedgerPrune`/`GraphPrune`/`MemoryPrune`/
/// `JobRetention`) routes through the same [`drain_via_executor`] path.
async fn drain_one_debt(
    ledger: &dyn LedgerStore,
    executor: &PruneExecutor<LedgerPruneTarget<'_>>,
    authz: &PruneAuthz,
    debt: &CleanupDebt,
    summary: &mut DebtDrainSummary,
) {
    match debt.kind {
        CleanupDebtKind::VectorDelete
        | CleanupDebtKind::LedgerPrune
        | CleanupDebtKind::GraphPrune
        | CleanupDebtKind::MemoryPrune
        | CleanupDebtKind::JobRetention => {
            drain_via_executor(ledger, executor, authz, debt, summary).await;
        }
        CleanupDebtKind::ArtifactDelete | CleanupDebtKind::CachePrune => {
            // No real drain available for this kind yet. This is not a
            // "not wired" placeholder — it is a documented gap per kind (see
            // `skip_reason_for_kind`): either the store boundary has no real
            // per-item deletion API, or (for cache) the owning crate is out
            // of this module's territory. Faking a drain for either of these
            // would violate the pruning contract's "no fake drains"
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

/// Drive the `axon-prune` executor for a debt kind that maps onto a
/// `PruneStep` (`Vector`: `vector_selector`; `Ledger`: `source_id`+
/// `generation`; `Graph`: `graph_stable_keys`/`graph_edge_ids`; `Memory`:
/// `memory_ids`).
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

    let plan = single_step_plan(step, debt.debt_id.clone());
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

/// [`PruneTarget`] backed by the real vector store, ledger, and (optionally)
/// graph/memory/job stores. Vector/ledger deletes are scoped to the debt's
/// superseded generation and fenced against the committed generation;
/// graph/memory/job-retention deletes are identity-scoped (stable keys /
/// memory ids / job ids) and not generation-fenced.
///
/// Constructed fresh per debt entry by
/// [`drain_cleanup_debt_full_with_jobs`] rather than once for a whole batch,
/// so `job_ids` can carry the current debt's identity (see that function's
/// doc comment for why `job_ids` can't instead ride on `PruneStep`).
struct LedgerPruneTarget<'a> {
    vector_store: &'a dyn VectorStore,
    ledger: &'a dyn LedgerStore,
    graph_store: Option<&'a dyn GraphStore>,
    memory_store: Option<&'a dyn MemoryStore>,
    job_store: Option<&'a dyn JobStore>,
    collection: String,
    source_id: SourceId,
    committed_generation: SourceGenerationId,
    /// Job ids named by the current `JobRetention` debt's
    /// `CleanupSelector::JobRows`. Empty for every other debt kind.
    job_ids: Vec<JobId>,
}

#[async_trait]
impl PruneTarget for LedgerPruneTarget<'_> {
    async fn current_generation(&self, _source_id: Option<&str>) -> Option<SourceGenerationId> {
        // The committed generation is the fence for every generation-scoped
        // step in this drain — all steps belong to the one source just
        // published.
        Some(self.committed_generation.clone())
    }

    async fn apply(&self, step: &PruneStep) -> Result<StepExecution, String> {
        match step.target {
            PruneTargetKind::Vector | PruneTargetKind::Ledger => {
                let Some(generation) = &step.generation else {
                    return Ok(StepExecution::skipped("no generation on step"));
                };
                // Defensive: never delete the committed generation even if
                // fencing was bypassed. The executor already fences, this is
                // belt-and-suspenders.
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
                    _ => unreachable!("outer match already narrowed to Vector | Ledger"),
                }
            }
            PruneTargetKind::Graph => {
                let Some(graph_store) = self.graph_store else {
                    return Err("no GraphStore wired for this drain".to_string());
                };
                let mut deleted = 0u64;
                let mut touched = false;
                if let Some(stable_keys) = &step.graph_stable_keys {
                    if !stable_keys.is_empty() {
                        touched = true;
                        let result = graph_store
                            .delete_nodes(stable_keys.clone())
                            .await
                            .map_err(|err| err.message.clone())?;
                        deleted += result.nodes_deleted;
                    }
                }
                if let Some(edge_ids) = &step.graph_edge_ids {
                    if !edge_ids.is_empty() {
                        touched = true;
                        let result = graph_store
                            .delete_edges(edge_ids.clone())
                            .await
                            .map_err(|err| err.message.clone())?;
                        deleted += result.edges_deleted;
                    }
                }
                if !touched {
                    return Ok(StepExecution::skipped("no graph identity on step"));
                }
                Ok(StepExecution::deleted(deleted))
            }
            PruneTargetKind::Memory => {
                let Some(memory_store) = self.memory_store else {
                    return Err("no MemoryStore wired for this drain".to_string());
                };
                let Some(memory_ids) = &step.memory_ids else {
                    return Ok(StepExecution::skipped("no memory identity on step"));
                };
                if memory_ids.is_empty() {
                    return Ok(StepExecution::skipped("no memory identity on step"));
                }
                for memory_id in memory_ids {
                    let request = MemoryForgetRequest {
                        memory_id: memory_id.clone(),
                        reason: Some("cleanup debt drain".to_string()),
                        timestamp: Timestamp(chrono::Utc::now().to_rfc3339()),
                    };
                    memory_store
                        .forget(request)
                        .await
                        .map_err(|err| err.message.clone())?;
                }
                Ok(StepExecution::deleted(memory_ids.len() as u64))
            }
            PruneTargetKind::JobRetention => self.apply_job_retention().await,
            other => Ok(StepExecution::skipped(format!(
                "unsupported prune target for this drain: {other:?}"
            ))),
        }
    }
}

impl LedgerPruneTarget<'_> {
    /// Drain this debt's `self.job_ids` (a `JobRetention` debt's
    /// `CleanupSelector::JobRows`) via `JobStore::delete_jobs`. Split out of
    /// `apply()` to keep that function under the monolith line cap.
    async fn apply_job_retention(&self) -> Result<StepExecution, String> {
        let Some(job_store) = self.job_store else {
            return Err("no JobStore wired for this drain".to_string());
        };
        if self.job_ids.is_empty() {
            return Ok(StepExecution::skipped("no job identity on step"));
        }
        let JobDeleteResult {
            deleted,
            skipped_live,
            missing,
        } = job_store
            .delete_jobs(&self.job_ids)
            .await
            .map_err(|err| err.message.clone())?;
        // Rows still live (running/claimed) or already gone are not store
        // errors — `delete_jobs` refuses to touch a live row rather than
        // erroring, and a missing row just means someone else already
        // cleaned it up. Both are reported here for observability; they do
        // not fail the step (fail-closed is reserved for an actual store
        // error, propagated above via `?`).
        if !skipped_live.is_empty() || !missing.is_empty() {
            tracing::debug!(
                deleted = deleted.len(),
                skipped_live = skipped_live.len(),
                missing = missing.len(),
                "job retention drain: some job rows were skipped (still live) or already gone"
            );
        }
        Ok(StepExecution::deleted(deleted.len() as u64))
    }
}

#[cfg(test)]
#[path = "prune_tests.rs"]
mod tests;
