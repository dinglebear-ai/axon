//! Bounded Qdrant upsert batching.

use std::collections::HashMap;
use std::sync::Arc;
use std::vec::IntoIter;

use axon_api::source::*;
use futures_util::stream::{self, StreamExt, TryStreamExt};

use super::QdrantVectorStore;
use super::convert::upsert_points_json;
use super::http::QdrantHttp;
use super::store_impl::request_usage;
use crate::store::Result;
use crate::store_helpers::stage_header;
use crate::validation::validate_upsert_batch;

pub(super) async fn upsert_batches_rest(
    store: &QdrantVectorStore,
    http: &QdrantHttp,
    spec: &CollectionSpec,
    batch: VectorPointBatch,
    stage: ErrorStage,
) -> Result<VectorStoreWriteResult> {
    validate_upsert_batch(spec, &batch, stage)?;

    let collection = batch.collection.clone();
    let points_attempted = batch.points.len() as u64;
    let payload_indexes_created = batch
        .payload_indexes
        .iter()
        .map(|index| index.field_name.clone())
        .collect();
    let url = http
        .endpoint()
        .collection_path(&batch.collection, "points?wait=true");

    let bodies = ChunkedUpsertBatches::new(batch, store.point_buffer())
        .map(|chunk| upsert_points_json(spec, &chunk))
        .collect::<Result<Vec<_>>>()?;
    let requests = bodies.len() as u64;
    let write_slots = store.write_slots();
    let provider_id = store.provider_id().0.clone();
    stream::iter(bodies)
        .map(|body| {
            let url = &url;
            let write_slots = Arc::clone(&write_slots);
            let provider_id = provider_id.clone();
            async move {
                let _permit = write_slots.acquire_owned().await.map_err(|_| {
                    ApiError::new(
                        "vector.qdrant.write_admission_closed",
                        stage,
                        "Qdrant write admission gate is closed",
                    )
                    .with_provider_id(provider_id)
                })?;
                http.put_json(stage, url, &body, "qdrant_upsert").await
            }
        })
        .buffer_unordered(store.write_parallelism())
        .try_collect::<Vec<_>>()
        .await?;

    Ok(VectorStoreWriteResult {
        header: stage_header(PipelinePhase::Upserting),
        collection,
        points_attempted,
        points_written: points_attempted,
        payload_indexes_created,
        usage: request_usage(requests),
    })
}

struct ChunkedUpsertBatches {
    batch_id: BatchId,
    collection: String,
    points: IntoIter<VectorPoint>,
    model: String,
    dimensions: u32,
    sparse_vectors: Option<HashMap<String, SparseVector>>,
    payload_indexes: Vec<PayloadIndexSpec>,
    chunk_size: usize,
}

impl ChunkedUpsertBatches {
    fn new(batch: VectorPointBatch, chunk_size: usize) -> Self {
        Self {
            batch_id: batch.batch_id,
            collection: batch.collection,
            points: batch.points.into_iter(),
            model: batch.model,
            dimensions: batch.dimensions,
            sparse_vectors: batch.sparse_vectors.map(|vectors| {
                vectors
                    .into_iter()
                    .map(|sparse| (sparse.chunk_id.0.clone(), sparse))
                    .collect()
            }),
            payload_indexes: batch.payload_indexes,
            chunk_size: chunk_size.max(1),
        }
    }
}

impl Iterator for ChunkedUpsertBatches {
    type Item = VectorPointBatch;

    fn next(&mut self) -> Option<Self::Item> {
        let points = self
            .points
            .by_ref()
            .take(self.chunk_size)
            .collect::<Vec<_>>();
        if points.is_empty() {
            return None;
        }
        let sparse_vectors = self.sparse_vectors.as_mut().map(|sparse_by_chunk| {
            points
                .iter()
                .filter_map(|point| sparse_by_chunk.remove(&point.chunk_id.0))
                .collect()
        });
        Some(VectorPointBatch {
            batch_id: self.batch_id,
            collection: self.collection.clone(),
            points,
            model: self.model.clone(),
            dimensions: self.dimensions,
            sparse_vectors,
            payload_indexes: self.payload_indexes.clone(),
        })
    }
}

#[cfg(test)]
#[path = "upsert_tests.rs"]
mod tests;
