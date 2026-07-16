//! Bounded batch embed+upsert for bulk memory operations (import), split
//! out of `vector.rs` to keep that file under the monolith line cap.

use axon_api::source::*;
use uuid::Uuid;

use super::VectorBackedMemoryStore;
use super::document::{
    build_memory_vector_batch, embedding_for_document, embedding_inputs, prepare_memory_document,
};
use crate::store::Result;

impl VectorBackedMemoryStore {
    /// Embed and upsert `records` in chunks bounded by
    /// `config.batch_limits.embed_batch_size`. Returns one outcome per
    /// record, in input order, so a caller can recover from a partial
    /// failure (one bad chunk) without losing the successful chunks.
    pub(super) async fn upsert_records_batched(
        &self,
        records: &[MemoryRecord],
    ) -> Result<Vec<(MemoryId, std::result::Result<Vec<VectorPointId>, ApiError>)>> {
        self.ensure_collection().await?;
        let batch_size = self.config.batch_limits.embed_batch_size.max(1);
        let mut outcomes = Vec::with_capacity(records.len());

        for chunk in records.chunks(batch_size) {
            let mut prepared = Vec::with_capacity(chunk.len());
            for record in chunk {
                match prepare_memory_document(record) {
                    Ok(document) => prepared.push((record.memory_id.clone(), document)),
                    Err(error) => {
                        outcomes.push((record.memory_id.clone(), Err(error)));
                    }
                }
            }
            if prepared.is_empty() {
                continue;
            }
            let batch_id = BatchId::new(Uuid::new_v4());
            let job_id = JobId::new(Uuid::new_v4());
            let embedding = self
                .embeddings
                .embed(EmbeddingBatch {
                    batch_id,
                    job_id,
                    provider_id: self.config.embedding_provider_id.clone(),
                    model: self.config.embedding_model.clone(),
                    items: prepared
                        .iter()
                        .flat_map(|(_, document)| embedding_inputs(document))
                        .collect(),
                    instruction: None,
                    priority: JobPriority::Normal,
                    metadata: MetadataMap::new(),
                })
                .await;

            let embedding = match embedding {
                Ok(embedding) => embedding,
                Err(error) => {
                    // The whole chunk failed together (e.g. provider outage) —
                    // every prepared record in it goes to review, not just one.
                    for (memory_id, _) in &prepared {
                        outcomes.push((memory_id.clone(), Err(error.clone())));
                    }
                    continue;
                }
            };

            let mut built = Vec::with_capacity(prepared.len());
            let mut points = Vec::new();
            for (memory_id, document) in prepared {
                let doc_embedding = embedding_for_document(&embedding, &document);
                match build_memory_vector_batch(&self.config, document, doc_embedding) {
                    Ok(batch) => {
                        let point_ids = batch
                            .points
                            .iter()
                            .map(|point| point.point_id.clone())
                            .collect::<Vec<_>>();
                        points.extend(batch.points);
                        built.push((memory_id, point_ids));
                    }
                    Err(error) => outcomes.push((memory_id, Err(error))),
                }
            }
            if points.is_empty() {
                continue;
            }

            let upsert_result = self
                .vectors
                .upsert(VectorPointBatch {
                    batch_id: embedding.batch_id,
                    collection: self.config.collection.clone(),
                    points,
                    model: embedding.model,
                    dimensions: embedding.dimensions,
                    sparse_vectors: None,
                    payload_indexes: super::payload::memory_payload_indexes(),
                })
                .await;

            match upsert_result {
                Ok(_) => {
                    for (memory_id, point_ids) in built {
                        outcomes.push((memory_id, Ok(point_ids)));
                    }
                }
                Err(error) => {
                    for (memory_id, _) in built {
                        outcomes.push((memory_id, Err(error.clone())));
                    }
                }
            }
        }

        Ok(outcomes)
    }
}
