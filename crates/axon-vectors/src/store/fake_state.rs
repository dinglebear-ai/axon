use axon_api::source::{ApiError, CollectionSpec};

use super::{FakeVectorState, Result};

impl FakeVectorState {
    pub(super) fn collection_spec(
        &self,
        collection: &str,
        stage: axon_error::ErrorStage,
    ) -> Result<&CollectionSpec> {
        self.collections.get(collection).ok_or_else(|| {
            ApiError::new(
                "vector.collection_not_found",
                stage,
                format!("collection {collection} has not been ensured"),
            )
        })
    }
}
