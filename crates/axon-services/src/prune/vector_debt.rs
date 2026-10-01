//! Exact vector-debt scope matching for operator generation pruning.
use super::*;
use axon_api::source::{CleanupDebtId, CleanupDebtKind, CleanupSelector};

/// This is intentionally honest rather than fabricated: no store in this
/// codebase currently exposes a read-only "how many would this delete"
/// primitive (see module docs). A real estimate lands once `VectorStore` (and
/// the artifact/graph/memory/ledger stores) grow a count-by-filter API.
pub(super) struct NullScopeSource;

impl PruneScopeSource for NullScopeSource {
    fn estimate(&self, _selector: &PruneSelector) -> PruneEstimate {
        PruneEstimate::default()
    }
}

impl VectorOnlyPruneTarget<'_> {
    /// Snapshot only debt covered by this exact generation and collection.
    /// Never acknowledge debt created after deletion or another provider's work.
    pub(super) async fn matching_vector_debt(
        &self,
        selector: &VectorDeleteSelector,
    ) -> Result<Vec<CleanupDebtId>, String> {
        let VectorDeleteSelector::Generation {
            collection,
            source_id,
            generation,
        } = selector
        else {
            return Ok(Vec::new());
        };
        let Some(ledger) = &self.ledger else {
            return Ok(Vec::new());
        };
        let metadata = ledger
            .get_manifest_metadata(source_id.clone(), generation.clone())
            .await
            .map_err(|err| err.message)?;
        let recorded_collection = metadata
            .as_ref()
            .and_then(|m| m.get(axon_ledger::GENERATION_VECTOR_COLLECTION_METADATA_KEY))
            .and_then(|v| v.as_str());
        let pending = ledger
            .list_pending_cleanup_debt(source_id.clone())
            .await
            .map_err(|err| err.message)?;
        Ok(pending.into_iter().filter(|debt| {
                debt.kind == CleanupDebtKind::VectorDelete
                    && debt.generation.as_ref() == Some(generation)
                    && debt.vector_collection.as_deref().or(recorded_collection) == Some(collection.as_str())
                    && matches!(&debt.selector,
                        CleanupSelector::Generation { source_id: debt_source, generation: debt_generation }
                        | CleanupSelector::SourceItem { source_id: debt_source, generation: debt_generation, .. }
                        if debt_source == source_id && debt_generation == generation)
            }).map(|debt| debt.debt_id).collect())
    }
}
