//! Vector point construction for unified source vectorization.

use axon_api::source::*;
use axon_vectors::point::{VectorPointBatchBuildContext, build_points_for_document};

use super::timestamp;

pub(super) struct VectorPointBuild {
    pub(super) batch: VectorPointBatch,
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) points_by_document: std::collections::BTreeMap<DocumentId, u32>,
}

pub(super) fn point_batch(
    collection: CollectionSpec,
    documents: &[PreparedDocument],
    embeddings: &mut EmbeddingResult,
) -> anyhow::Result<VectorPointBuild> {
    // Move dense vectors into the chunk index. A 512-chunk Qwen3 batch carries
    // ~2 MiB of f32 payload alone, so cloning every vector here doubles the
    // hottest post-TEI allocation for no semantic benefit.
    let mut by_chunk = std::mem::take(&mut embeddings.vectors)
        .into_iter()
        .map(|vector| (vector.chunk_id.clone(), vector))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut points = Vec::new();
    let point_context = VectorPointBatchBuildContext {
        embedded_at: timestamp(),
    };
    let mut points_by_document = std::collections::BTreeMap::new();
    for document in documents {
        let document_embeddings = EmbeddingResult {
            batch_id: embeddings.batch_id.clone(),
            job_id: embeddings.job_id,
            provider_id: embeddings.provider_id.clone(),
            model: embeddings.model.clone(),
            dimensions: embeddings.dimensions,
            vectors: document
                .chunks
                .iter()
                .filter_map(|chunk| by_chunk.remove(&chunk.chunk_id))
                .collect(),
            usage: ProviderUsage {
                input_tokens: None,
                output_tokens: None,
                requests: 0,
                duration_ms: 0,
            },
            warnings: Vec::new(),
        };
        let (document_points, _compatibility_skip_count) =
            build_points_for_document(&collection, document, document_embeddings, &point_context)?;
        let document_point_count = u32::try_from(document_points.len()).unwrap_or(u32::MAX);
        points_by_document.insert(document.document_id.clone(), document_point_count);
        points.extend(document_points);
    }
    Ok(VectorPointBuild {
        batch: VectorPointBatch {
            batch_id: embeddings.batch_id.clone(),
            collection: collection.collection,
            points,
            model: embeddings.model.clone(),
            dimensions: embeddings.dimensions,
            sparse_vectors: None,
            payload_indexes: collection.payload_indexes,
        },
        points_by_document,
    })
}
