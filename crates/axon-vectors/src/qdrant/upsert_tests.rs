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
    let chunks = ChunkedUpsertBatches::new(batch(5), 2).collect::<Vec<_>>();

    assert_eq!(chunks.len(), 3);
    assert_eq!(chunks[0].points.len(), 2);
    assert_eq!(chunks[1].points.len(), 2);
    assert_eq!(chunks[2].points.len(), 1);
    assert_eq!(chunks[0].points[0].point_id, VectorPointId::new("point-0"));
    assert_eq!(chunks[2].points[0].point_id, VectorPointId::new("point-4"));
    assert!(chunks.iter().all(|chunk| chunk.points.len() <= 2));
}

#[test]
fn chunked_upsert_batches_partition_sparse_vectors_once() {
    let chunks = ChunkedUpsertBatches::new(batch(5), 2).collect::<Vec<_>>();

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
fn chunked_upsert_batches_scale_without_losing_or_reordering_points() {
    let point_count = UPSERT_BATCH_SIZE * 8 + 17;
    let chunks =
        ChunkedUpsertBatches::new(batch(point_count), UPSERT_BATCH_SIZE).collect::<Vec<_>>();

    assert_eq!(chunks.len(), 9);
    assert!(
        chunks
            .iter()
            .all(|chunk| chunk.points.len() <= UPSERT_BATCH_SIZE)
    );
    let point_ids = chunks
        .iter()
        .flat_map(|chunk| chunk.points.iter().map(|point| point.point_id.0.clone()))
        .collect::<Vec<_>>();
    let sparse_ids = chunks
        .iter()
        .flat_map(|chunk| {
            chunk
                .sparse_vectors
                .as_ref()
                .expect("sparse vectors preserved")
                .iter()
                .map(|sparse| sparse.chunk_id.0.clone())
        })
        .collect::<Vec<_>>();

    assert_eq!(point_ids.len(), point_count);
    assert_eq!(sparse_ids.len(), point_count);
    for (n, (point_id, sparse_id)) in point_ids.iter().zip(&sparse_ids).enumerate() {
        assert_eq!(point_id, &format!("point-{n}"));
        assert_eq!(sparse_id, &format!("chunk-{n}"));
    }
    assert_eq!(point_ids.first().map(String::as_str), Some("point-0"));
    let expected_last = format!("point-{}", point_count - 1);
    assert_eq!(
        point_ids.last().map(String::as_str),
        Some(expected_last.as_str())
    );
}

#[test]
fn chunked_upsert_batches_empty_batch_makes_no_requests() {
    assert!(ChunkedUpsertBatches::new(batch(0), 2).next().is_none());
}
