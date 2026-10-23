//! Bounded historical generation receipt lookup for the fake ledger.

use super::*;

pub(super) async fn generation_receipt(
    state: &Arc<Mutex<FakeLedgerState>>,
    source_id: SourceId,
    generation: SourceGenerationId,
) -> Result<Option<crate::store::GenerationReceipt>> {
    Ok(state
        .lock()
        .await
        .generations
        .get(&(source_id, generation))
        .map(|generation| crate::store::GenerationReceipt {
            status: generation.status,
            publish_state: generation.publish_state,
            published: generation.published_at.is_some(),
        }))
}
