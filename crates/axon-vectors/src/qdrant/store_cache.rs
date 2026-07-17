use axon_api::source::*;

use super::QdrantVectorStore;
use super::http::QdrantHttp;
use super::store_impl::detect_collection_spec;
use crate::store::Result;

impl QdrantVectorStore {
    pub(super) fn http(&self) -> Result<QdrantHttp> {
        QdrantHttp::new(self.url(), &self.provider_id().0)
    }

    pub(super) async fn cached_collection_spec(&self, collection: &str) -> Option<CollectionSpec> {
        let epoch = self.collection_spec_cache_epoch(collection);
        self.collection_specs
            .read()
            .await
            .get(collection)
            .filter(|(cached_epoch, _)| *cached_epoch == epoch)
            .map(|(_, spec)| spec.clone())
    }

    pub(super) async fn cache_collection_spec(&self, spec: CollectionSpec) {
        let epoch = self.collection_spec_cache_epoch(&spec.collection);
        self.collection_specs
            .write()
            .await
            .insert(spec.collection.clone(), (epoch, spec));
    }

    pub(super) async fn fetch_collection_spec(
        &self,
        http: &QdrantHttp,
        collection: &str,
        stage: axon_error::ErrorStage,
    ) -> Result<Option<CollectionSpec>> {
        let url = http.endpoint().collection_path(collection, "");
        let body = http.get_json(stage, &url, "qdrant_get_collection").await?;
        Ok(body.and_then(|body| detect_collection_spec(collection, &body)))
    }

    pub(super) async fn require_collection_spec(
        &self,
        http: &QdrantHttp,
        collection: &str,
        stage: axon_error::ErrorStage,
    ) -> Result<CollectionSpec> {
        if let Some(spec) = self.cached_collection_spec(collection).await {
            return Ok(spec);
        }
        let spec = self
            .fetch_collection_spec(http, collection, stage)
            .await?
            .ok_or_else(|| {
                ApiError::new(
                    "vector.collection_not_found",
                    stage,
                    format!("collection {collection} has not been ensured"),
                )
            })?;
        self.cache_collection_spec(spec.clone()).await;
        Ok(spec)
    }
}
