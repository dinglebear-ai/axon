//! Per-debt drain implementations for the cleanup-debt drain loop.
//!
//! Split out of `prune.rs` to stay under the monolith line cap; owns the
//! kind-specific drain executors (`drain_one_debt` dispatch, artifact
//! delete, cache prune, executor-backed vector/source drains).

use super::*;

/// Execute one debt entry and, on clean success, mark it resolved. Every
/// drainable kind (`VectorDelete`/`LedgerPrune`/`GraphPrune`/`MemoryPrune`/
/// `JobRetention`) routes through the same [`drain_via_executor`] path.
pub(super) async fn drain_one_debt(
    ledger: &dyn LedgerStore,
    executor: &PruneExecutor<LedgerPruneTarget<'_>>,
    authz: &PruneAuthz,
    debt: &CleanupDebt,
    collection: &str,
    artifact_store: Option<&dyn ArtifactStore>,
    document_cache: Option<&dyn DocumentCache>,
    summary: &mut DebtDrainSummary,
) {
    match debt.kind {
        CleanupDebtKind::VectorDelete
        | CleanupDebtKind::LedgerPrune
        | CleanupDebtKind::GraphPrune
        | CleanupDebtKind::MemoryPrune
        | CleanupDebtKind::JobRetention => {
            drain_via_executor(ledger, executor, authz, debt, collection, summary).await;
        }
        CleanupDebtKind::ArtifactDelete => {
            drain_artifact_delete(ledger, artifact_store, debt, summary).await;
        }
        CleanupDebtKind::CachePrune => {
            drain_cache_prune(ledger, document_cache, debt, summary).await;
        }
    }
}

pub(super) async fn drain_artifact_delete(
    ledger: &dyn LedgerStore,
    artifact_store: Option<&dyn ArtifactStore>,
    debt: &CleanupDebt,
    summary: &mut DebtDrainSummary,
) {
    let Some(artifact_store) = artifact_store else {
        trace_unwired(debt);
        return;
    };
    let CleanupSelector::Artifact { artifact_id } = &debt.selector else {
        trace_unwired(debt);
        return;
    };
    let delete = artifact_store
        .delete(ArtifactHandle {
            artifact_id: artifact_id.clone(),
            artifact_kind: ArtifactKind::RawContent,
            uri: None,
        })
        .await;
    if let Err(err) = delete {
        tracing::warn!(
            error = %err.message,
            debt_id = %debt.debt_id.0,
            "artifact cleanup debt delete failed; leaving pending"
        );
        summary.failed += 1;
        return;
    }
    resolve_debt(ledger, debt, summary).await;
}

pub(super) async fn drain_cache_prune(
    ledger: &dyn LedgerStore,
    document_cache: Option<&dyn DocumentCache>,
    debt: &CleanupDebt,
    summary: &mut DebtDrainSummary,
) {
    let Some(document_cache) = document_cache else {
        trace_unwired(debt);
        return;
    };
    let CleanupSelector::CacheKeys { keys } = &debt.selector else {
        trace_unwired(debt);
        return;
    };
    for key in keys {
        let parsed: DocumentCacheKey = match serde_json::from_str(key) {
            Ok(key) => key,
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    debt_id = %debt.debt_id.0,
                    cache_key = %key,
                    "cache cleanup debt key is not a DocumentCacheKey; leaving pending"
                );
                summary.failed += 1;
                return;
            }
        };
        if let Err(err) = document_cache
            .invalidate(DocumentCacheInvalidation::Key { key: parsed })
            .await
        {
            tracing::warn!(
                error = %err.message,
                debt_id = %debt.debt_id.0,
                "cache cleanup debt invalidate failed; leaving pending"
            );
            summary.failed += 1;
            return;
        }
    }
    resolve_debt(ledger, debt, summary).await;
}

pub(super) async fn resolve_debt(
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

pub(super) fn trace_unwired(debt: &CleanupDebt) {
    tracing::debug!(
        debt_id = %debt.debt_id.0,
        kind = ?debt.kind,
        reason = skip_reason_for_kind(debt.kind),
        "skipping cleanup debt: no real drain available for this kind"
    );
}

/// Drive the `axon-prune` executor for a debt kind that maps onto a
/// `PruneStep` (`Vector`: `vector_selector`; `Ledger`: `source_id`+
/// `generation`; `Graph`: `graph_stable_keys`/`graph_edge_ids`; `Memory`:
/// `memory_ids`).
pub(super) async fn drain_via_executor(
    ledger: &dyn LedgerStore,
    executor: &PruneExecutor<LedgerPruneTarget<'_>>,
    authz: &PruneAuthz,
    debt: &CleanupDebt,
    collection: &str,
    summary: &mut DebtDrainSummary,
) {
    let Some(step) = debt_to_step(debt, collection) else {
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
pub(super) struct LedgerPruneTarget<'a> {
    pub(super) vector_store: &'a dyn VectorStore,
    pub(super) ledger: &'a dyn LedgerStore,
    pub(super) graph_store: Option<&'a dyn GraphStore>,
    pub(super) memory_store: Option<&'a dyn MemoryStore>,
    pub(super) job_store: Option<&'a dyn JobStore>,
    pub(super) collection: String,
    pub(super) source_id: SourceId,
    pub(super) committed_generation: SourceGenerationId,
    /// Job ids named by the current `JobRetention` debt's
    /// `CleanupSelector::JobRows`. Empty for every other debt kind.
    pub(super) job_ids: Vec<JobId>,
}

#[async_trait]
impl PruneTarget for LedgerPruneTarget<'_> {
    async fn current_generation(
        &self,
        _source_id: Option<&str>,
    ) -> Result<Option<SourceGenerationId>, String> {
        // The committed generation is the fence for every generation-scoped
        // step in this drain — all steps belong to the one source just
        // published.
        Ok(Some(self.committed_generation.clone()))
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
                let mut records = Vec::with_capacity(memory_ids.len());
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
                    let record = memory_store
                        .get(memory_id.clone())
                        .await
                        .map_err(|err| err.message.clone())?
                        .ok_or_else(|| {
                            format!("memory {} missing after cleanup-debt forget", memory_id.0)
                        })?;
                    records.push(record);
                }
                let job_store = self.job_store.ok_or_else(|| {
                    "no JobStore wired for canonical memory cleanup publication".to_string()
                })?;
                crate::memory::sync::enqueue_memory_records(
                    job_store,
                    &records,
                    "cleanup_debt_forget",
                )
                .await
                .map_err(|error| error.to_string())?;
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
