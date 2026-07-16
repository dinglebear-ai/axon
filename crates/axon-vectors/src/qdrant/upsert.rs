//! Bounded Qdrant upsert batching.

use std::collections::BTreeSet;

use axon_api::source::*;

use super::convert::upsert_points_json;
use super::http::QdrantHttp;
use super::store_impl::request_usage;
use crate::store::Result;
use crate::store_helpers::stage_header;
use crate::validation::validate_upsert_batch;

/// Points per Qdrant upsert request.
///
/// This keeps large source writes from creating unbounded JSON bodies while
/// still staying large enough to avoid excessive HTTP overhead for normal jobs.
const UPSERT_BATCH_SIZE: usize = 512;

pub(super) async fn upsert_batches_rest(
    http: &QdrantHttp,
    spec: &CollectionSpec,
    batch: VectorPointBatch,
    stage: axon_error::ErrorStage,
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

    let mut requests = 0u64;
    for chunk in chunked_upsert_batches(&batch, UPSERT_BATCH_SIZE) {
        let body = upsert_points_json(spec, &chunk)?;
        http.put_json(stage, &url, &body, "qdrant_upsert").await?;
        requests += 1;
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

fn chunked_upsert_batches(batch: &VectorPointBatch, chunk_size: usize) -> Vec<VectorPointBatch> {
    let chunk_size = chunk_size.max(1);
    batch
        .points
        .chunks(chunk_size)
        .map(|points| VectorPointBatch {
            batch_id: batch.batch_id,
            collection: batch.collection.clone(),
            points: points.to_vec(),
            model: batch.model.clone(),
            dimensions: batch.dimensions,
            sparse_vectors: sparse_vectors_for_points(batch.sparse_vectors.as_ref(), points),
            payload_indexes: batch.payload_indexes.clone(),
        })
        .collect()
}

fn sparse_vectors_for_points(
    sparse_vectors: Option<&Vec<SparseVector>>,
    points: &[VectorPoint],
) -> Option<Vec<SparseVector>> {
    let sparse_vectors = sparse_vectors?;
    let chunk_ids = points
        .iter()
        .map(|point| point.chunk_id.clone())
        .collect::<BTreeSet<_>>();
    Some(
        sparse_vectors
            .iter()
            .filter(|sparse| chunk_ids.contains(&sparse.chunk_id))
            .cloned()
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(n: usize) -> VectorPoint {
        VectorPoint {
            point_id: VectorPointId::new(format!("point-{n}")),
            chunk_id: ChunkId::new(format!("chunk-{n}")),
            vector: vec![n as f32],
            sparse_vector: None,
            payload: MetadataMap::new(),
        }
    }

    fn batch(points: usize) -> VectorPointBatch {
        VectorPointBatch {
            batch_id: BatchId::new(uuid::Uuid::from_u128(7)),
            collection: "axon-test".to_string(),
            points: (0..points).map(point).collect(),
            model: "test-model".to_string(),
            dimensions: 1,
            sparse_vectors: Some(
                (0..points)
                    .map(|n| SparseVector {
                        chunk_id: ChunkId::new(format!("chunk-{n}")),
                        indices: vec![n as u32],
                        values: vec![1.0],
                    })
                    .collect(),
            ),
            payload_indexes: vec![PayloadIndexSpec {
                field_name: "source_id".to_string(),
                field_schema: PayloadFieldSchema::Keyword,
                required_for_filters: true,
            }],
        }
    }

    #[test]
    fn chunked_upsert_batches_are_bounded_and_ordered() {
        let chunks = chunked_upsert_batches(&batch(5), 2);

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].points.len(), 2);
        assert_eq!(chunks[1].points.len(), 2);
        assert_eq!(chunks[2].points.len(), 1);
        assert_eq!(chunks[0].points[0].point_id, VectorPointId::new("point-0"));
        assert_eq!(chunks[2].points[0].point_id, VectorPointId::new("point-4"));
        assert!(chunks.iter().all(|chunk| chunk.points.len() <= 2));
    }

    #[test]
    fn chunked_upsert_batches_filter_batch_sparse_vectors_to_chunk_points() {
        let chunks = chunked_upsert_batches(&batch(5), 2);

        let sparse_ids = chunks
            .iter()
            .map(|chunk| {
                chunk
                    .sparse_vectors
                    .as_ref()
                    .expect("sparse vectors preserved")
                    .iter()
                    .map(|sparse| sparse.chunk_id.0.clone())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

        assert_eq!(
            sparse_ids,
            vec![
                vec!["chunk-0".to_string(), "chunk-1".to_string()],
                vec!["chunk-2".to_string(), "chunk-3".to_string()],
                vec!["chunk-4".to_string()],
            ]
        );
    }

    #[test]
    fn chunked_upsert_batches_empty_batch_makes_no_requests() {
        assert!(chunked_upsert_batches(&batch(0), 2).is_empty());
    }
}
