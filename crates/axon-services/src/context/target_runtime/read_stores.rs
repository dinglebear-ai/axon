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

pub(super) fn build_qdrant_store(
    cfg: &Config,
) -> Result<QdrantVectorStore, axon_api::source::ApiError> {
    let mut store = build_qdrant_store_base(cfg);
    configure_qdrant_transport(&mut store)?;
    Ok(store)
}

fn build_qdrant_store_base(cfg: &Config) -> QdrantVectorStore {
    let mut store = QdrantVectorStore::new(cfg.qdrant_url.clone(), VECTOR_PROVIDER_ID);
    axon_vectors::qdrant::configure_point_buffer(&mut store, cfg.qdrant_point_buffer);
    axon_vectors::qdrant::configure_parallelism(
        &mut store,
        axon_core::config::parse::tuning::qdrant_upsert_parallelism(),
        axon_core::config::parse::tuning::qdrant_payload_index_parallelism(),
    );
    axon_vectors::qdrant::configure_collection_settings(
        &mut store,
        axon_vectors::qdrant::QdrantCollectionSettings {
            dense_on_disk: true,
            hnsw_m: axon_core::config::parse::tuning::qdrant_hnsw_m() as u64,
            hnsw_ef_construct: axon_core::config::parse::tuning::qdrant_hnsw_ef_construct() as u64,
            hnsw_on_disk: axon_core::config::parse::tuning::qdrant_hnsw_on_disk(),
            indexing_threshold: axon_core::config::parse::tuning::qdrant_indexing_threshold_kb()
                as u64,
            quantization_enabled: axon_core::config::parse::tuning::qdrant_quantization_enabled(),
            quantization_quantile: 0.99,
            quantization_always_ram:
                axon_core::config::parse::tuning::qdrant_quantization_always_ram(),
        },
    );
    axon_vectors::qdrant::configure_bulk_load(
        &mut store,
        axon_core::config::parse::tuning::qdrant_bulk_load(),
        axon_core::config::parse::tuning::qdrant_bulk_indexing_threshold_kb() as u64,
        axon_core::config::parse::tuning::qdrant_indexing_threshold_kb() as u64,
    );
    axon_vectors::qdrant::configure_async_writes(
        &mut store,
        axon_core::config::parse::tuning::qdrant_async_writes(),
    );
    store
}

fn configure_qdrant_transport(
    store: &mut QdrantVectorStore,
) -> Result<(), axon_api::source::ApiError> {
    axon_vectors::qdrant::configure_write_transport(
        store,
        &axon_core::config::parse::tuning::qdrant_write_transport(),
        axon_core::config::parse::tuning::qdrant_grpc_url().as_deref(),
    )
}

/// Build the read-plane stores from Config. Store constructors do not perform
/// I/O; only the embedding identity is derived from the live TEI provider,
/// with a config/default fallback when it is unreachable.
pub async fn build_read_stores_from_config(cfg: &Config) -> TargetReadStores {
    let identity = resolve_embedding_identity(cfg).await;
    let embedding_provider = build_tei_provider(cfg, &identity);
    let mut vector_store = build_qdrant_store_base(cfg);
    if let Err(error) = configure_qdrant_transport(&mut vector_store) {
        tracing::warn!(%error, "failed to configure Qdrant read-plane transport; using REST");
    }
    TargetReadStores {
        vector_store: Arc::new(vector_store),
        embedding_provider: Arc::new(embedding_provider),
        embedding_provider_id: ProviderId::new(EMBEDDING_PROVIDER_ID),
        embedding_model: identity.model,
        embedding_dimensions: identity.dimensions,
    }
}
