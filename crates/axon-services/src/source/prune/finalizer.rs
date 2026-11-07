//! Select graph retirement retries and their ledger dependencies.
use super::*;

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
