//! Reconcile transient sweeper contention before emitting terminal cleanup warnings.
use super::*;
use std::{future::Future, time::Duration};
use tokio_util::sync::CancellationToken;

pub(crate) async fn drain<F, Fut>(
    ledger: &dyn LedgerStore,
    counts: &IndexCounts,
    cancellation: Option<&CancellationToken>,
    budget: Duration,
    interval: Duration,
    mut pass: F,
) -> DebtDrainSummary
where
    F: FnMut(bool) -> Fut,
    Fut: Future<Output = DebtDrainSummary>,
{
    let deadline = tokio::time::Instant::now() + budget;
    let mut total = DebtDrainSummary::default();
    let mut retry_only = false;
    loop {
        if retry_only && cancellation.is_some_and(CancellationToken::is_cancelled) {
            return total;
        }
        // Do not drop an in-flight retirement: its source lease must release.
        let current = pass(retry_only).await;
        retry_only = true;
        total.resolved += current.resolved;
        total.points_deleted += current.points_deleted;
        total.failed = current.failed;
        total.enumeration_failed = current.enumeration_failed;
        if current.failed == 0 || current.enumeration_failed {
            return total;
        }
        let pending = match ledger
            .list_pending_cleanup_debt(counts.source_id.clone())
            .await
        {
            Ok(pending) => pending,
            Err(_) => {
                total.enumeration_failed = true;
                return total;
            }
        };
        // LEARNED: another drain can finish the failed item before this pass
        // reports its outcome. Terminal warnings must describe remaining debt.
        total.failed = pending.len() as u64;
        let retry_busy = pending.iter().any(|debt| {
            matches!(&debt.selector, CleanupSelector::GraphItemEvidence { retirement_generation, .. }
                if retirement_generation == &counts.generation)
                && debt.last_error.as_ref().is_some_and(|error| error.code == "graph.source_busy")
        });
        if !retry_busy || tokio::time::Instant::now() >= deadline {
            return total;
        }
        if cancellation.is_some_and(CancellationToken::is_cancelled) {
            return total;
        }
        let wait = tokio::time::sleep_until((tokio::time::Instant::now() + interval).min(deadline));
        if let Some(cancellation) = cancellation {
            tokio::select! {
                _ = wait => {},
                _ = cancellation.cancelled() => return total,
            }
        } else {
            wait.await;
        }
        if tokio::time::Instant::now() >= deadline {
            match ledger
                .list_pending_cleanup_debt(counts.source_id.clone())
                .await
            {
                Ok(pending) => total.failed = pending.len() as u64,
                Err(_) => total.enumeration_failed = true,
            }
            return total;
        }
    }
}

/// Retry the contended graph group and its superseded-ledger dependency.
/// Other provider failures and the committed ledger generation remain excluded.
pub(super) fn eligible_retry(debt: &CleanupDebt, generation: Option<&SourceGenerationId>) -> bool {
    match &debt.selector {
        CleanupSelector::LedgerGenerations {
            up_to_generation, ..
        } => {
            debt.kind == CleanupDebtKind::LedgerPrune
                && generation.is_some_and(|current| current != up_to_generation)
                && debt.last_error.is_none()
        }
        CleanupSelector::GraphItemEvidence {
            retirement_generation,
            ..
        } => {
            generation == Some(retirement_generation)
                && debt
                    .last_error
                    .as_ref()
                    .is_none_or(|error| error.code == "graph.source_busy")
        }
        _ => false,
    }
}
