//! Bounded batch embed+upsert for bulk memory operations (import), split
//! out of `vector.rs` to keep that file under the monolith line cap.

use axon_api::source::*;
use uuid::Uuid;

use super::VectorBackedMemoryStore;
use super::payload::{memory_payload, memory_payload_indexes};
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
            let batch_id = BatchId::new(Uuid::new_v4());
            let job_id = JobId::new(Uuid::new_v4());
            let chunk_ids: Vec<ChunkId> = chunk
                .iter()
                .map(|record| ChunkId::new(format!("memory:{}", record.memory_id.0)))
                .collect();
            let embedding = self
                .embeddings
                .embed(EmbeddingBatch {
                    batch_id,
                    job_id,
                    provider_id: self.config.embedding_provider_id.clone(),
                    model: self.config.embedding_model.clone(),
                    items: chunk
                        .iter()
                        .zip(&chunk_ids)
                        .map(|(record, chunk_id)| EmbeddingInput {
                            chunk_id: chunk_id.clone(),
                            text: record.body.clone(),
                            content_kind: ContentKind::PlainText,
                            metadata: MetadataMap::new(),
                        })
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
                    // every record in it goes to review, not just one.
                    for record in chunk {
                        outcomes.push((record.memory_id.clone(), Err(error.clone())));
                    }
                    continue;
                }
            };

            let mut points = Vec::with_capacity(chunk.len());
            for (record, chunk_id) in chunk.iter().zip(&chunk_ids) {
                let Some(vector) = embedding
                    .vectors
                    .iter()
                    .find(|vector| vector.chunk_id == *chunk_id)
                    .cloned()
                else {
                    outcomes.push((
                        record.memory_id.clone(),
                        Err(ApiError::new(
                            "memory.embedding_missing",
                            axon_error::ErrorStage::Embedding,
                            format!(
                                "embedding provider did not return memory {}",
                                record.memory_id.0
                            ),
                        )),
                    ));
                    continue;
                };
                let point_id = VectorPointId::new(format!("memory:{}", record.memory_id.0));
                let payload =
                    memory_payload(record, &point_id, &embedding, &self.config.collection);
                points.push((
                    record.memory_id.clone(),
                    point_id,
                    chunk_id.clone(),
                    vector.values,
                    payload,
                ));
            }
            if points.is_empty() {
                continue;
            }

            let upsert_result = self
                .vectors
                .upsert(VectorPointBatch {
                    batch_id: embedding.batch_id,
                    collection: self.config.collection.clone(),
                    points: points
                        .iter()
                        .map(|(_, point_id, chunk_id, vector, payload)| VectorPoint {
                            point_id: point_id.clone(),
                            chunk_id: chunk_id.clone(),
                            vector: vector.clone(),
                            sparse_vector: None,
                            payload: payload.clone(),
                        })
                        .collect(),
                    model: embedding.model,
                    dimensions: embedding.dimensions,
                    sparse_vectors: None,
                    payload_indexes: memory_payload_indexes(),
                })
                .await;

            match upsert_result {
                Ok(_) => {
                    for (memory_id, point_id, ..) in points {
                        outcomes.push((memory_id, Ok(vec![point_id])));
                    }
                }
                Err(error) => {
                    for (memory_id, ..) in points {
                        outcomes.push((memory_id, Err(error.clone())));
                    }
                }
            }
        }

        Ok(outcomes)
    }
}
