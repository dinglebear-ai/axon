//! Nonblocking recovery of abandoned optimizer transitions.
use super::*;

impl QdrantVectorStore {
    pub(in crate::qdrant) async fn recover_bulk_load_transitions(&self) -> Result<()> {
        let Some(journal) = configured_journal()? else {
            return Ok(());
        };
        self.recover_bulk_load_transitions_from(&journal).await
    }

    pub(super) async fn recover_bulk_load_transitions_from(
        &self,
        journal: &BulkLoadJournal,
    ) -> Result<()> {
        let endpoint = self.bulk_journal_endpoint()?;
        for transition in journal.pending().map_err(|error| {
            ApiError::new(
                "vector.qdrant.bulk_journal",
                ErrorStage::Upserting,
                format!("failed to read bulk-load recovery state: {error}"),
            )
        })? {
            if transition.endpoint != endpoint {
                continue;
            }
            let key = BulkLoadKey {
                endpoint: endpoint.clone(),
                collection: transition.collection.clone(),
            };
            // A live owner must finish its own transition. Never block an
            // async executor on flock or restore a collection still being loaded.
            let Some(_lease) = journal.try_collection_lease(&key).map_err(|error| {
                ApiError::new(
                    "vector.qdrant.bulk_lease",
                    ErrorStage::Upserting,
                    format!("failed to inspect recovery lease: {error}"),
                )
            })?
            else {
                tracing::debug!(collection = %key.collection, "bulk recovery deferred to live owner");
                continue;
            };
            // The snapshot can be stale after another owner completed. Re-read
            // only while holding the collection lease and use its current baseline.
            let Some(transition) = journal
                .pending()
                .map_err(|error| {
                    ApiError::new(
                        "vector.qdrant.bulk_journal",
                        ErrorStage::Upserting,
                        format!("failed to re-read bulk recovery state: {error}"),
                    )
                })?
                .into_iter()
                .find(|entry| entry.endpoint == key.endpoint && entry.collection == key.collection)
            else {
                continue;
            };
            self.set_indexing_threshold(&transition.collection, transition.restore_threshold)
                .await?;
            self.wait_for_optimizer_ready(&transition.collection)
                .await?;
            journal.complete(&key).map_err(|error| {
                ApiError::new(
                    "vector.qdrant.bulk_journal",
                    ErrorStage::Upserting,
                    format!("failed to clear recovered bulk-load state: {error}"),
                )
            })?;
        }
        Ok(())
    }
}
