use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};

use httpmock::MockServer;

use super::*;
use crate::qdrant::configure_async_writes;

fn point(n: usize) -> VectorPoint {
    let point_id = format!("point-{n}");
    let chunk_id = format!("chunk-{n}");
    crate::testing::test_clean_point(crate::testing::TestPointSpec {
        collection: "axon-test",
        point_id: &point_id,
        chunk_id: &chunk_id,
        vector: &[n as f32],
        text: "test chunk",
        namespace: "chunk",
        batch_id: &uuid::Uuid::from_u128(7).to_string(),
        model: "test-model",
        dimensions: 1,
        job_id: "00000000-0000-0000-0000-000000000007",
    })
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

fn valid_batch(points: usize) -> VectorPointBatch {
    let batch_uuid = uuid::Uuid::from_u128(7);
    let batch_id = batch_uuid.to_string();
    VectorPointBatch {
        batch_id: BatchId::new(batch_uuid),
        collection: "axon-test".to_string(),
        points: (0..points)
            .map(|n| {
                let point_id = format!("point-{n}");
                let chunk_id = format!("chunk-{n}");
                let vector = [n as f32];
                crate::testing::test_clean_point(crate::testing::TestPointSpec {
                    collection: "axon-test",
                    point_id: &point_id,
                    chunk_id: &chunk_id,
                    vector: &vector,
                    text: "test vector point",
                    namespace: "dense",
                    batch_id: &batch_id,
                    model: "test-model",
                    dimensions: 1,
                    job_id: "job-qdrant-parallel-test",
                })
            })
            .collect(),
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
        payload_indexes: Vec::new(),
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
    let point_buffer = 512;
    let point_count = point_buffer * 8 + 17;
    let chunks = ChunkedUpsertBatches::new(batch(point_count), point_buffer).collect::<Vec<_>>();

    assert_eq!(chunks.len(), 9);
    assert!(
        chunks
            .iter()
            .all(|chunk| chunk.points.len() <= point_buffer)
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

fn read_http_request(stream: &mut TcpStream) {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set request read timeout");
    let mut request = Vec::new();
    let mut buffer = [0_u8; 4096];
    let mut header_end = None;
    let mut content_length = None;
    loop {
        let read = stream.read(&mut buffer).expect("read HTTP request");
        assert!(read > 0, "client closed before request body completed");
        request.extend_from_slice(&buffer[..read]);
        if header_end.is_none()
            && let Some(end) = request.windows(4).position(|window| window == b"\r\n\r\n")
        {
            let end = end + 4;
            let headers = String::from_utf8_lossy(&request[..end]);
            content_length = headers.lines().find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())
                    .flatten()
            });
            header_end = Some(end);
        }
        if let Some(end) = header_end
            && request.len() >= end + content_length.unwrap_or(0)
        {
            break;
        }
    }
}

fn concurrent_put_server(
    expected_requests: usize,
) -> (String, Arc<AtomicUsize>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind concurrent test server");
    let address = listener.local_addr().expect("test server address");
    listener
        .set_nonblocking(true)
        .expect("make concurrent test server nonblocking");
    let arrivals = Arc::new(AtomicUsize::new(0));
    let server_arrivals = Arc::clone(&arrivals);
    let barrier = Arc::new(Barrier::new(expected_requests));
    let handle = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut workers = Vec::with_capacity(expected_requests);
        while workers.len() < expected_requests && Instant::now() < deadline {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let arrivals = Arc::clone(&server_arrivals);
                    let barrier = Arc::clone(&barrier);
                    workers.push(thread::spawn(move || {
                        read_http_request(&mut stream);
                        arrivals.fetch_add(1, Ordering::SeqCst);
                        barrier.wait();
                        stream
                            .write_all(
                                b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
                            )
                            .expect("write HTTP response");
                    }));
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("accept concurrent test request: {error}"),
            }
        }
        assert_eq!(
            workers.len(),
            expected_requests,
            "all concurrent requests must connect before the server deadline"
        );
        for worker in workers {
            worker.join().expect("concurrent request worker");
        }
    });
    (format!("http://{address}"), arrivals, handle)
}

fn test_collection_spec() -> CollectionSpec {
    CollectionSpec {
        collection: "axon-test".to_string(),
        dense: VectorConfig {
            name: "dense".to_string(),
            dimensions: 1,
            distance: VectorDistance::Cosine,
        },
        payload_indexes: Vec::new(),
        sparse: Some(SparseVectorConfig {
            name: "bm42".to_string(),
            modifier: SparseVectorModifier::Idf,
        }),
        aliases: Vec::new(),
        distance: Some(VectorDistance::Cosine),
        metadata: MetadataMap::new(),
    }
}

#[tokio::test]
async fn qdrant_upsert_splits_on_actual_encoded_byte_limit() {
    let server = MockServer::start_async().await;
    let endpoint = server
        .mock_async(|when, then| {
            when.method("PUT").path("/collections/axon-test/points");
            then.status(200);
        })
        .await;
    let http = QdrantHttp::new(&server.base_url(), "qdrant-test").expect("http");
    let spec = test_collection_spec();
    let mut chunk = batch(2);
    for point in &mut chunk.points {
        point.payload.insert(
            "blob".to_string(),
            serde_json::Value::String("x".repeat(1_500)),
        );
    }
    let url = http
        .endpoint()
        .collection_path("axon-test", "points?wait=true");
    let batch_sparse = chunk
        .sparse_vectors
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .map(|sparse| (sparse.chunk_id.0.as_str(), sparse))
        .collect::<HashMap<_, _>>();
    let single_bytes = serde_json::to_vec(&UpsertPointsBody::new(
        &spec,
        &chunk.points[..1],
        &batch_sparse,
    ))
    .expect("encode one point")
    .len();
    let pair_bytes =
        serde_json::to_vec(&UpsertPointsBody::new(&spec, &chunk.points, &batch_sparse))
            .expect("encode two points")
            .len();
    assert!(
        single_bytes < pair_bytes,
        "second point must increase encoded size"
    );
    let max_request_bytes = single_bytes + (pair_bytes - single_bytes) / 2;

    let requests = upsert_chunk_rest(
        &http,
        &spec,
        &chunk,
        &url,
        ErrorStage::Upserting,
        max_request_bytes,
        false,
    )
    .await
    .expect("byte-bounded upsert");

    assert_eq!(
        requests, 2,
        "two points should split into two bounded requests"
    );
    endpoint.assert_calls_async(2).await;
}

#[tokio::test]
async fn qdrant_upsert_rejects_indivisible_oversized_point() {
    let server = MockServer::start_async().await;
    let http = QdrantHttp::new(&server.base_url(), "qdrant-test").expect("http");
    let spec = test_collection_spec();
    let mut chunk = batch(1);
    chunk.points[0].payload.insert(
        "blob".to_string(),
        serde_json::Value::String("x".repeat(2_000)),
    );
    let url = http
        .endpoint()
        .collection_path("axon-test", "points?wait=true");

    let error = upsert_chunk_rest(
        &http,
        &spec,
        &chunk,
        &url,
        ErrorStage::Upserting,
        512,
        false,
    )
    .await
    .expect_err("single oversized point must fail closed");

    assert_eq!(
        error.code.to_string(),
        "vector.qdrant.upsert_point_oversized"
    );
}

#[tokio::test]
async fn async_upsert_pipelines_then_uses_a_wait_true_barrier() {
    let server = MockServer::start_async().await;
    let upsert = server
        .mock_async(|when, then| {
            when.method("PUT")
                .path("/collections/axon-test/points")
                .query_param("wait", "false");
            then.status(200).json_body(serde_json::json!({
                "result": {"operation_id": 42, "status": "acknowledged"},
                "status": "ok"
            }));
        })
        .await;
    let barrier = server
        .mock_async(|when, then| {
            when.method("PUT")
                .path("/collections/axon-test/points")
                .query_param("wait", "true");
            then.status(200);
        })
        .await;
    let mut store = QdrantVectorStore::new(server.base_url(), "qdrant-test");
    configure_async_writes(&mut store, true);
    let http = store.http().unwrap();

    upsert_batches_rest(
        &store,
        &http,
        &test_collection_spec(),
        valid_batch(1),
        ErrorStage::Upserting,
    )
    .await
    .unwrap();
    upsert.assert_calls_async(1).await;
    barrier.assert_calls_async(1).await;
}

#[tokio::test]
async fn qdrant_upsert_chunks_overlap_with_configured_parallelism() {
    // Do not use httpmock delay timing here. Its delayed-response machinery can
    // serialize requests internally, which makes the mock server itself the
    // bottleneck and cannot prove client fanout. This raw HTTP server holds
    // every response at a barrier until all three request bodies have arrived.
    let (base_url, arrivals, server) = concurrent_put_server(3);
    let http = QdrantHttp::new(&base_url, "qdrant-test").expect("http");
    let mut store = QdrantVectorStore::new(base_url, "qdrant-test");
    crate::qdrant::configure_point_buffer(&mut store, 2);
    crate::qdrant::configure_parallelism(&mut store, 3, 1);
    let spec = CollectionSpec {
        collection: "axon-test".to_string(),
        dense: VectorConfig {
            name: "dense".to_string(),
            dimensions: 1,
            distance: VectorDistance::Cosine,
        },
        payload_indexes: Vec::new(),
        sparse: Some(SparseVectorConfig {
            name: "bm42".to_string(),
            modifier: SparseVectorModifier::Idf,
        }),
        aliases: Vec::new(),
        distance: Some(VectorDistance::Cosine),
        metadata: MetadataMap::new(),
    };

    let task = tokio::spawn(async move {
        upsert_batches_rest(&store, &http, &spec, valid_batch(5), ErrorStage::Upserting).await
    });

    let result = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("parallelism=3 must deliver all three request bodies to the barrier")
        .expect("upsert task")
        .expect("parallel upsert");
    assert_eq!(result.usage.requests, 3);
    assert_eq!(arrivals.load(Ordering::SeqCst), 3);
    server.join().expect("concurrent test server");
}
