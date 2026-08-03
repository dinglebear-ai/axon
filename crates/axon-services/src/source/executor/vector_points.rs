//! Vector point construction for unified source vectorization.

use axon_api::source::*;
use axon_vectors::point::{VectorPointBatchBuildContext, VectorPointBatchBuilder};

use super::timestamp;

pub(super) struct VectorPointBuild {
    pub(super) batch: VectorPointBatch,
    pub(super) skipped_redaction: u64,
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) points_by_document: std::collections::BTreeMap<DocumentId, u32>,
}

pub(super) fn point_batch(
    collection: CollectionSpec,
    documents: &[PreparedDocument],
    embeddings: &EmbeddingResult,
) -> anyhow::Result<VectorPointBuild> {
    let by_chunk = embeddings
        .vectors
        .iter()
        .cloned()
        .map(|vector| (vector.chunk_id.clone(), vector))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut points = Vec::new();
    let mut skipped_redaction = 0u64;
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
                .filter_map(|chunk| by_chunk.get(&chunk.chunk_id).cloned())
                .collect(),
            usage: embeddings.usage.clone(),
            warnings: embeddings.warnings.clone(),
        };
        let (batch, document_skipped) = VectorPointBatchBuilder::new(
            collection.clone(),
            document.clone(),
            document_embeddings,
            VectorPointBatchBuildContext {
                embedded_at: timestamp(),
            },
        )
        .build_with_skipped_count()?;
        let document_point_count = u32::try_from(batch.points.len()).unwrap_or(u32::MAX);
        points_by_document.insert(document.document_id.clone(), document_point_count);
        points.extend(batch.points);
        skipped_redaction += document_skipped;
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
        skipped_redaction,
        points_by_document,
    })
}
