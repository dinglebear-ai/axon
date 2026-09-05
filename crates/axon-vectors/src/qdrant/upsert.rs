//! Bounded Qdrant upsert batching.

use std::collections::HashMap;
use std::io::{self, Write};
use std::sync::Arc;
use std::vec::IntoIter;

use axon_api::source::*;
use futures_util::{StreamExt, stream};

use super::QdrantVectorStore;
use super::convert::UpsertPointsBody;
use super::http::QdrantHttp;
use super::store_impl::request_usage;
use crate::store::Result;
use crate::store_helpers::stage_header;
use crate::validation::validate_upsert_batch;

const MAX_UPSERT_REQUEST_BYTES: usize = 16 * 1024 * 1024;

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
    let wait = if store.async_writes { "false" } else { "true" };
    let url = http.endpoint().collection_path(
        &batch.collection,
        &format!("points?wait={wait}&ordering=strong"),
    );

    let barrier_chunk = store
        .async_writes
        .then(|| completion_barrier_batch(&batch))
        .flatten();
    let write_slots = store.write_slots();
    let provider_id = store.provider_id().0.clone();
    let mut pending = stream::iter(ChunkedUpsertBatches::new(batch, store.point_buffer()))
        .map(|chunk| {
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
                upsert_chunk_rest(
                    http,
                    spec,
                    &chunk,
                    url,
                    stage,
                    MAX_UPSERT_REQUEST_BYTES,
                    store.async_writes,
                )
                .await
            }
        })
        .buffer_unordered(store.write_parallelism());
    let mut requests = 0u64;
    while let Some(result) = pending.next().await {
        requests = requests.saturating_add(result?);
    }
    drop(pending);
    if let Some(barrier_chunk) = barrier_chunk {
        let barrier_url = http
            .endpoint()
            .collection_path(&collection, "points?wait=true&ordering=strong");
        requests = requests.saturating_add(
            upsert_chunk_rest(
                http,
                spec,
                &barrier_chunk,
                &barrier_url,
                stage,
                MAX_UPSERT_REQUEST_BYTES,
                false,
            )
            .await?,
        );
    }

    Ok(VectorStoreWriteResult {
        header: stage_header(PipelinePhase::Upserting),
        collection,
        points_attempted,
        points_written: points_attempted,
        payload_indexes_created,
        usage: request_usage(requests),
    })
}

fn completion_barrier_batch(batch: &VectorPointBatch) -> Option<VectorPointBatch> {
    let point = batch.points.last()?.clone();
    let sparse_vectors = batch.sparse_vectors.as_ref().map(|vectors| {
        vectors
            .iter()
            .find(|vector| vector.chunk_id == point.chunk_id)
            .cloned()
            .into_iter()
            .collect()
    });
    Some(VectorPointBatch {
        batch_id: batch.batch_id,
        collection: batch.collection.clone(),
        points: vec![point],
        model: batch.model.clone(),
        dimensions: batch.dimensions,
        sparse_vectors,
        payload_indexes: Vec::new(),
    })
}

async fn upsert_chunk_rest(
    http: &QdrantHttp,
    spec: &CollectionSpec,
    chunk: &VectorPointBatch,
    url: &str,
    stage: ErrorStage,
    max_request_bytes: usize,
    asynchronous: bool,
) -> Result<u64> {
    let batch_sparse = chunk
        .sparse_vectors
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .map(|sparse| (sparse.chunk_id.0.as_str(), sparse))
        .collect::<HashMap<_, _>>();
    let max_request_bytes = max_request_bytes.max(1);
    let ranges = byte_bounded_ranges(chunk, spec, &batch_sparse, max_request_bytes, stage)?;
    let mut requests = 0u64;

    for range in ranges {
        let body = encode_upsert_body(
            &UpsertPointsBody::new(spec, &chunk.points[range.clone()], &batch_sparse),
            max_request_bytes,
            stage,
        )?;

        let body = body.ok_or_else(|| {
            ApiError::new(
                "vector.qdrant.upsert_size_estimate_failed",
                stage,
                "qdrant upsert range exceeded its conservative byte estimate",
            )
        })?;

        if asynchronous {
            http.put_json_bytes(stage, url, body, "qdrant_upsert_async")
                .await?;
        } else {
            http.put_json_bytes(stage, url, body, "qdrant_upsert")
                .await?;
        }
        requests = requests.saturating_add(1);
    }

    Ok(requests)
}

fn byte_bounded_ranges(
    chunk: &VectorPointBatch,
    spec: &CollectionSpec,
    sparse: &HashMap<&str, &SparseVector>,
    limit: usize,
    stage: ErrorStage,
) -> Result<Vec<std::ops::Range<usize>>> {
    let empty = serde_json::to_vec(&UpsertPointsBody::new(spec, &[], sparse))
        .map_err(|error| ApiError::new("vector.qdrant.encode_failed", stage, error.to_string()))?
        .len();
    let mut ranges = Vec::new();
    let mut start = 0;
    let mut estimated = empty;
    for (index, point) in chunk.points.iter().enumerate() {
        let encoded = serde_json::to_vec(&UpsertPointsBody::new(
            spec,
            std::slice::from_ref(point),
            sparse,
        ))
        .map_err(|error| ApiError::new("vector.qdrant.encode_failed", stage, error.to_string()))?
        .len();
        if encoded > limit {
            return Err(ApiError::new(
                "vector.qdrant.upsert_point_oversized",
                stage,
                "a single vector point exceeds the encoded qdrant request limit",
            )
            .with_context("chunk_id", point.chunk_id.0.clone())
            .with_context("encoded_bytes_min", encoded.to_string())
            .with_context("limit_bytes", limit.to_string()));
        }
        // A singleton contains the fixed wrapper. Its delta from an empty
        // body is the point payload; reserve punctuation for both dense and
        // sparse arrays so the final range is encoded exactly once.
        let contribution = encoded.saturating_sub(empty).saturating_add(4);
        if index > start && estimated.saturating_add(contribution) > limit {
            ranges.push(start..index);
            start = index;
            estimated = empty;
        }
        estimated = estimated.saturating_add(contribution);
    }
    if start < chunk.points.len() {
        ranges.push(start..chunk.points.len());
    }
    Ok(ranges)
}

fn encode_upsert_body(
    body: &UpsertPointsBody<'_>,
    max_request_bytes: usize,
    stage: ErrorStage,
) -> Result<Option<Vec<u8>>> {
    let mut buffer = CappedJsonBuffer::new(max_request_bytes);
    match serde_json::to_writer(&mut buffer, body) {
        Ok(()) => Ok(Some(buffer.bytes)),
        Err(_) if buffer.overflowed => Ok(None),
        Err(error) => Err(ApiError::new(
            "vector.qdrant.encode_failed",
            stage,
            format!("failed to encode qdrant upsert request: {error}"),
        )),
    }
}

struct CappedJsonBuffer {
    bytes: Vec<u8>,
    limit: usize,
    overflowed: bool,
}

impl CappedJsonBuffer {
    fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(limit.min(1024 * 1024)),
            limit,
            overflowed: false,
        }
    }
}

impl Write for CappedJsonBuffer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.bytes.len().saturating_add(buf.len()) > self.limit {
            self.overflowed = true;
            return Err(io::Error::other("qdrant upsert body exceeds byte limit"));
        }
        self.bytes.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
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
