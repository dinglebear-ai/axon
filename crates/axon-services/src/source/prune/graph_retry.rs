//! Persist bounded retry delays without abandoning graph retirement debt.
use super::*;

async fn persist_failure(
    ledger: &dyn LedgerStore,
    debt: &CleanupDebt,
    error: &axon_api::source::ApiError,
) {
    let mut retry = debt.clone();
    retry.attempts = retry.attempts.saturating_add(1);
    let delay = 30_u64
        .saturating_mul(2_u64.saturating_pow(retry.attempts.saturating_sub(1).min(4)))
        .min(300);
    retry.next_retry_at = Some(Timestamp::from(
        chrono::Utc::now() + chrono::Duration::seconds(delay as i64),
    ));
    retry.last_error = Some(SourceError {
        code: error.code.to_string(),
        severity: Severity::Warning,
        message: error.message.clone(),
        source_item_key: match &debt.selector {
            CleanupSelector::GraphItemEvidence {
                source_item_key, ..
            } => Some(source_item_key.clone()),
            _ => None,
        },
        retryable: true,
        provider_id: None,
        cause: None,
    });
    // Completed rows must never be resurrected by a concurrent stale retry.
    if let Err(persist_error) = ledger.record_cleanup_debt(retry).await {
        tracing::warn!(debt_id = %debt.debt_id.0, error = %persist_error.message, "failed to persist graph retirement retry delay; debt remains pending");
    }
}

/// A finalizer retries only its own generation's stale busy delay. It still
/// acquires the source writer lease and checks current manifest/status in the provider.
fn retry_after_publication(debt: &CleanupDebt, provider_ops: &dyn CleanupProviderOps) -> bool {
    matches!(&debt.selector, CleanupSelector::GraphItemEvidence { retirement_generation, .. }
        if provider_ops.graph_retry_generation() == Some(retirement_generation))
        && debt
            .last_error
            .as_ref()
            .is_some_and(|error| error.code == "graph.source_busy")
}

/// A persisted busy lease delay applies to the source, not just its first item.
pub(super) fn source_backoff(debts: &[CleanupDebt], provider_ops: &dyn CleanupProviderOps) -> bool {
    let now = Timestamp::from(chrono::Utc::now());
    debts.iter().any(|debt| {
        matches!(debt.selector, CleanupSelector::GraphItemEvidence { .. })
            && !retry_after_publication(debt, provider_ops)
            && debt
                .last_error
                .as_ref()
                .is_some_and(|error| error.code == "graph.source_busy")
            && debt
                .next_retry_at
                .as_ref()
                .is_some_and(|retry| retry.0 > now.0)
    })
}

pub(super) async fn drain(
    ledger: &dyn LedgerStore,
    provider_ops: &dyn CleanupProviderOps,
    debt: &CleanupDebt,
    summary: &mut DebtDrainSummary,
) -> bool {
    if let CleanupSelector::GraphItemEvidence {
        source_id,
        source_item_key,
        retirement_generation,
    } = &debt.selector
    {
        if !retry_after_publication(debt, provider_ops)
            && debt
                .next_retry_at
                .as_ref()
                .is_some_and(|retry| retry.0 > Timestamp::from(chrono::Utc::now()).0)
        {
            summary.failed += 1;
            return false;
        }
        match provider_ops
            .graph_retire_item(
                source_id.clone(),
                source_item_key.clone(),
                retirement_generation.clone(),
            )
            .await
        {
            Ok(_) => super::drain_ops::resolve_debt(ledger, debt, summary).await,
            Err(error) => {
                persist_failure(ledger, debt, &error).await;
                summary.failed += 1;
                tracing::warn!(debt_id = %debt.debt_id.0, error = %error, "graph item retirement deferred");
                // LEARNED: a source writer lease covers publication and all its
                // graph writes. Retrying every item only competes with its owner.
                return error.code.0.as_str() == "graph.source_busy";
            }
        }
        return false;
    }
    false
}
