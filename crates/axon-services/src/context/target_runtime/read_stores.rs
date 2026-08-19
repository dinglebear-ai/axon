//! Read-plane store composition for the target-local runtime.

use std::sync::Arc;

use axon_api::source::ProviderId;
use axon_core::config::Config;
use axon_embedding::provider::EmbeddingProvider;
use axon_vectors::qdrant::QdrantVectorStore;
use axon_vectors::store::VectorStore;

use super::{
    EMBEDDING_PROVIDER_ID, VECTOR_PROVIDER_ID, build_tei_provider, resolve_embedding_identity,
};

/// Read-plane stores plus their provider identity, built from Config.
pub struct TargetReadStores {
    pub vector_store: Arc<dyn VectorStore>,
    pub embedding_provider: Arc<dyn EmbeddingProvider>,
    pub embedding_provider_id: ProviderId,
    pub embedding_model: String,
    pub embedding_dimensions: u32,
}

/// Build the read-plane stores from Config. Store constructors do not perform
/// I/O; only the embedding identity is derived from the live TEI provider,
/// with a config/default fallback when it is unreachable.
pub async fn build_read_stores_from_config(cfg: &Config) -> TargetReadStores {
    let identity = resolve_embedding_identity(cfg).await;
    let embedding_provider = build_tei_provider(cfg, &identity);
    let mut vector_store = QdrantVectorStore::new(cfg.qdrant_url.clone(), VECTOR_PROVIDER_ID);
    axon_vectors::qdrant::configure_point_buffer(&mut vector_store, cfg.qdrant_point_buffer);
    axon_vectors::qdrant::configure_parallelism(
        &mut vector_store,
        axon_core::config::parse::tuning::qdrant_upsert_parallelism(),
        axon_core::config::parse::tuning::qdrant_payload_index_parallelism(),
    );
    TargetReadStores {
        vector_store: Arc::new(vector_store),
        embedding_provider: Arc::new(embedding_provider),
        embedding_provider_id: ProviderId::new(EMBEDDING_PROVIDER_ID),
        embedding_model: identity.model,
        embedding_dimensions: identity.dimensions,
    }
}
