//! Bounded provider cleanup passes over durable debt.
use super::*;

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
    let (debts_to_drain, mut vector_siblings, mut graph_siblings) =
        group_cleanup_debts(pending, provider_ops, &mut summary);

    for debt in debts_to_drain {
        if let Some(group) = graph_siblings.remove(&debt.debt_id.0) {
            if super::graph_retry::drain_group(ledger, provider_ops, &group, &mut summary).await {
                break;
            }
            continue;
        }
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
            if drain_one_debt(
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
            .await
            {
                break;
            }
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

type DebtGroups = BTreeMap<String, Vec<CleanupDebt>>;

fn group_cleanup_debts(
    pending: Vec<CleanupDebt>,
    provider_ops: &dyn CleanupProviderOps,
    summary: &mut DebtDrainSummary,
) -> (Vec<CleanupDebt>, DebtGroups, DebtGroups) {
    let mut vector_groups: BTreeMap<(String, String), Vec<CleanupDebt>> = BTreeMap::new();
    let mut other_debts = Vec::new();
    let mut graph_groups: BTreeMap<(String, String), Vec<CleanupDebt>> = BTreeMap::new();
    let graph_backoff = graph_retry::source_backoff(&pending, provider_ops);
    for debt in pending {
        if provider_ops.graph_retry_only()
            && !finalizer::eligible_retry(&debt, provider_ops.graph_retry_generation())
        {
            summary.failed += 1;
            continue;
        }
        if graph_backoff && matches!(debt.selector, CleanupSelector::GraphItemEvidence { .. }) {
            summary.failed += 1;
            continue;
        }
        if let CleanupSelector::GraphItemEvidence {
            source_id,
            retirement_generation,
            ..
        } = &debt.selector
        {
            graph_groups
                .entry((source_id.0.clone(), retirement_generation.0.clone()))
                .or_default()
                .push(debt);
            continue;
        }
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
    let mut graph_siblings = BTreeMap::new();
    for group in graph_groups.into_values() {
        for debts in group.chunks(64) {
            let first = debts[0].clone();
            graph_siblings.insert(first.debt_id.0.clone(), debts.to_vec());
            other_debts.push(first);
        }
    }
    debts_to_drain.extend(other_debts);
    // Provider effects must finish before deleting the rows that describe them.
    // Keep the existing vector-first order and defer ledger dependencies last.
    debts_to_drain.sort_by_key(|debt| debt.kind == CleanupDebtKind::LedgerPrune);

    (debts_to_drain, vector_siblings, graph_siblings)
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
