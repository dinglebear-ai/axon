use std::sync::Arc;
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
};

use super::super::collection_spec::detect_collection_spec;
use super::*;
use crate::qdrant::{
    configure_grpc_transport, configure_parallelism, configure_rest_transport,
    configure_write_transport, grpc_connection_parts,
};
use serde_json::json;

fn collection_spec(name: &str) -> CollectionSpec {
    CollectionSpec {
        collection: name.to_string(),
        dense: VectorConfig {
            name: "dense".to_string(),
            dimensions: 1024,
            distance: VectorDistance::Cosine,
        },
        payload_indexes: vec![PayloadIndexSpec {
            field_name: "source_id".to_string(),
            field_schema: PayloadFieldSchema::Keyword,
            required_for_filters: true,
        }],
        sparse: Some(SparseVectorConfig {
            name: "bm42".to_string(),
            modifier: SparseVectorModifier::Idf,
        }),
        aliases: Vec::new(),
        distance: Some(VectorDistance::Cosine),
        metadata: MetadataMap::new(),
    }
}

fn sequential_response_server(responses: Vec<(u16, String)>) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind sequential test server");
    let address = listener.local_addr().expect("test server address");
    let server = thread::spawn(move || {
        for (status, body) in responses {
            let (mut stream, _) = listener.accept().expect("accept test request");
            let mut request = [0_u8; 8192];
            let _ = stream.read(&mut request).expect("read test request");
            let reason = match status {
                200 => "OK",
                404 => "Not Found",
                409 => "Conflict",
                _ => "Test",
            };
            write!(
                stream,
                "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .expect("write test response");
        }
    });
    (format!("http://{address}"), server)
}

#[tokio::test]
async fn collection_create_conflict_refetches_and_rejects_incompatible_race_winner() {
    let incompatible = serde_json::json!({
        "result": { "config": { "params": {
            "vectors": { "dense": { "size": 384, "distance": "Cosine" } },
            "sparse_vectors": { "bm42": { "modifier": "idf" } }
        } } }
    })
    .to_string();
    let (base_url, server) = sequential_response_server(vec![
        (404, String::new()),
        (409, String::new()),
        (200, incompatible),
    ]);
    let store = QdrantVectorStore::new(base_url, "qdrant-test");

    let error = store
        .ensure_collection_inner(collection_spec("axon-race"))
        .await
        .expect_err("an incompatible concurrent collection create must fail closed");

    assert_eq!(error.code.to_string(), "vector.collection_drift");
    server.join().expect("sequential test server");
}

#[tokio::test]
async fn optional_payload_index_conflict_rejects_incompatible_schema() {
    let actual = serde_json::json!({
        "result": {
            "config": { "params": {
                "vectors": { "dense": { "size": 1024, "distance": "Cosine" } },
                "sparse_vectors": { "bm42": { "modifier": "idf" } }
            } },
            "payload_schema": { "optional_tag": { "data_type": "integer" } }
        }
    })
    .to_string();
    let (base_url, server) = sequential_response_server(vec![(409, String::new()), (200, actual)]);
    let store = QdrantVectorStore::new(base_url, "qdrant-test");
    let http = store.http().expect("qdrant HTTP wrapper");
    let mut spec = collection_spec("axon-index-race");
    spec.payload_indexes = vec![PayloadIndexSpec {
        field_name: "optional_tag".to_string(),
        field_schema: PayloadFieldSchema::Keyword,
        required_for_filters: false,
    }];

    let error = store
        .ensure_payload_indexes(&http, &spec, ErrorStage::Upserting)
        .await
        .expect_err("optional index conflict must verify its schema");

    assert_eq!(error.code.to_string(), "vector.collection_drift");
    server.join().expect("sequential test server");
}

#[tokio::test]
async fn optional_payload_index_conflict_accepts_matching_schema() {
    let actual = serde_json::json!({
        "result": {
            "config": { "params": {
                "vectors": { "dense": { "size": 1024, "distance": "Cosine" } },
                "sparse_vectors": { "bm42": { "modifier": "idf" } }
            } },
            "payload_schema": { "optional_tag": { "data_type": "keyword" } }
        }
    })
    .to_string();
    let (base_url, server) = sequential_response_server(vec![(409, String::new()), (200, actual)]);
    let store = QdrantVectorStore::new(base_url, "qdrant-test");
    let http = store.http().expect("qdrant HTTP wrapper");
    let mut spec = collection_spec("axon-index-race");
    spec.payload_indexes = vec![PayloadIndexSpec {
        field_name: "optional_tag".to_string(),
        field_schema: PayloadFieldSchema::Keyword,
        required_for_filters: false,
    }];

    store
        .ensure_payload_indexes(&http, &spec, ErrorStage::Upserting)
        .await
        .expect("matching optional index conflict is idempotent");

    server.join().expect("sequential test server");
}

#[test]
fn qdrant_grpc_transport_is_selectable_without_removing_rest_fallback() {
    let mut store = QdrantVectorStore::new("http://127.0.0.1:6333", "qdrant-test");
    assert_eq!(store.write_transport(), QdrantWriteTransport::Rest);

    configure_grpc_transport(&mut store, "http://127.0.0.1:6334").unwrap();

    assert_eq!(store.write_transport(), QdrantWriteTransport::Grpc);
    configure_rest_transport(&mut store);
    assert_eq!(store.write_transport(), QdrantWriteTransport::Rest);
}

#[test]
fn qdrant_grpc_transport_reuses_rest_credentials_without_leaking_them_into_url() {
    let (url, api_key) = grpc_connection_parts(
        "http://secret-token@qdrant.internal:6333?api_key=ignored", // gitleaks:allow — synthetic credential fixture
        "http://qdrant.internal:6334/path?api_key=grpc-fallback", // gitleaks:allow — synthetic credential fixture
    );
    assert_eq!(url, "http://qdrant.internal:6334");
    assert_eq!(api_key.as_deref(), Some("grpc-fallback"));
    assert!(!url.contains("secret"));
    assert!(!url.contains("api_key"));
}

#[test]
fn grpc_transport_rejects_inherited_credentials_over_remote_plaintext() {
    let mut store =
        QdrantVectorStore::new("https://rest-secret@qdrant.internal:6333", "qdrant-test");
    let error = configure_write_transport(&mut store, "grpc", Some("http://qdrant.internal:6334"))
        .expect_err("inherited credentials over plaintext gRPC must fail closed");
    assert_eq!(error.code.0, "vector.qdrant.insecure_credentials");
    assert!(!error.to_string().contains("rest-secret"));
}

#[test]
fn grpc_transport_allows_plaintext_credentials_on_loopback() {
    let mut store = QdrantVectorStore::new("http://local-secret@127.0.0.1:6333", "qdrant-test");
    configure_write_transport(&mut store, "grpc", Some("http://127.0.0.1:6334"))
        .expect("loopback plaintext remains supported for local development");
}

#[test]
fn qdrant_write_transport_rejects_unknown_values_and_missing_grpc_url() {
    let mut store = QdrantVectorStore::new("http://127.0.0.1:6333", "qdrant-test");
    let unknown = configure_write_transport(&mut store, "magic", None).unwrap_err();
    assert_eq!(unknown.code.0, "vector.qdrant.transport_config");
    assert!(unknown.message.contains("rest or grpc"));

    let missing = configure_write_transport(&mut store, "grpc", None).unwrap_err();
    assert_eq!(missing.code.0, "vector.qdrant.grpc_url_missing");
    assert_eq!(store.write_transport(), QdrantWriteTransport::Rest);
}

#[test]
fn qdrant_parallelism_configuration_is_bounded_away_from_zero() {
    let mut store = QdrantVectorStore::new("http://127.0.0.1:9", "qdrant-test");
    configure_parallelism(&mut store, 0, 0);
    assert_eq!(store.write_parallelism(), 1);
    assert_eq!(store.payload_index_parallelism(), 1);

    configure_parallelism(&mut store, 4, 8);
    assert_eq!(store.write_parallelism(), 4);
    assert_eq!(store.payload_index_parallelism(), 8);
}

#[test]
fn qdrant_stores_share_parallelism_gates_for_the_same_endpoint_and_profile() {
    let mut first = QdrantVectorStore::new("http://qdrant-shared.test", "first");
    let mut second = QdrantVectorStore::new("http://qdrant-shared.test/", "second");
    let mut other = QdrantVectorStore::new("http://qdrant-other.test", "other");
    configure_parallelism(&mut first, 4, 8);
    configure_parallelism(&mut second, 4, 8);
    configure_parallelism(&mut other, 4, 8);

    assert!(Arc::ptr_eq(
        &first.parallelism_gates,
        &second.parallelism_gates
    ));
    assert!(!Arc::ptr_eq(
        &first.parallelism_gates,
        &other.parallelism_gates
    ));
}

#[tokio::test]
async fn qdrant_stores_share_one_aggregate_limit_across_config_and_url_aliases() {
    let mut first = QdrantVectorStore::new("http://qdrant-admission.test", "first");
    configure_parallelism(&mut first, 1, 1);
    let mut reconfigured =
        QdrantVectorStore::new("HTTP://QDRANT-ADMISSION.TEST:80/", "reconfigured");
    configure_parallelism(&mut reconfigured, 8, 8);

    assert!(Arc::ptr_eq(
        &first.parallelism_gates,
        &reconfigured.parallelism_gates
    ));
    let permit = first
        .write_slots()
        .acquire_owned()
        .await
        .expect("first store acquires the endpoint's sole write slot");
    assert!(
        reconfigured.write_slots().try_acquire_owned().is_err(),
        "a differently configured alias must not multiply endpoint capacity"
    );
    drop(permit);
    let _permit = reconfigured
        .write_slots()
        .try_acquire_owned()
        .expect("shared capacity returns after release");
}

#[test]
fn qdrant_delete_receipt_marks_observed_count_as_estimated() {
    let result = qdrant_delete_result("axon-test".to_string(), 7, "pre_delete_exact_match_count");

    assert_eq!(result.points_matched, 7);
    assert_eq!(result.points_deleted, 7);
    assert_eq!(
        result.metadata["points_deleted_count_basis"],
        json!("pre_delete_exact_match_count")
    );
    assert_eq!(result.metadata["points_deleted_is_estimate"], json!(true));
    assert_eq!(result.warnings.len(), 1);
    assert_eq!(
        result.warnings[0].code,
        "vector.qdrant_delete_count_estimated"
    );
}

#[tokio::test]
async fn require_collection_spec_uses_cached_spec_without_network() {
    let store = QdrantVectorStore::new("http://127.0.0.1:9", "qdrant-test");
    let expected = collection_spec("axon-cache");
    store.cache_collection_spec(expected.clone()).await;
    let http = store.http().expect("http wrapper");

    let actual = store
        .require_collection_spec(&http, "axon-cache", ErrorStage::Retrieving)
        .await
        .expect("cached collection spec");

    assert_eq!(actual.collection, expected.collection);
    assert_eq!(actual.dense, expected.dense);
    assert_eq!(actual.sparse, expected.sparse);
    assert_eq!(actual.payload_indexes, expected.payload_indexes);
}

#[tokio::test]
async fn collection_spec_cache_is_shared_across_store_clones() {
    let store = QdrantVectorStore::new("http://127.0.0.1:9", "qdrant-test");
    store
        .cache_collection_spec(collection_spec("axon-shared"))
        .await;
    let cloned = store.clone();

    let cached = cloned
        .cached_collection_spec("axon-shared")
        .await
        .expect("clone sees shared cache");

    assert_eq!(cached.collection, "axon-shared");
    assert_eq!(cached.dense.dimensions, 1024);
}

#[tokio::test]
async fn collection_spec_cache_invalidation_reaches_existing_store_instances() {
    let store = QdrantVectorStore::new("http://127.0.0.1:9", "qdrant-test");
    store
        .cache_collection_spec(collection_spec("axon-reset"))
        .await;
    assert!(store.cached_collection_spec("axon-reset").await.is_some());

    QdrantVectorStore::invalidate_collection_spec_cache("http://127.0.0.1:9", "axon-reset");

    assert!(
        store.cached_collection_spec("axon-reset").await.is_none(),
        "raw reset must invalidate caches held by already-live contexts"
    );
}

#[test]
fn detect_named_mode_collection_with_sparse_and_indexes() {
    let body = json!({
        "result": {
            "config": {
                "params": {
                    "vectors": { "dense": { "size": 1024, "distance": "Cosine" } },
                    "sparse_vectors": { "bm42": { "modifier": "idf" } }
                }
            },
            "payload_schema": {
                "source_id": { "data_type": "keyword" },
                "chunk_index": { "data_type": "integer" }
            }
        }
    });
    let spec = detect_collection_spec("axon", &body, ErrorStage::Upserting)
        .expect("valid schema")
        .expect("named spec");
    assert_eq!(spec.dense.name, "dense");
    assert_eq!(spec.dense.dimensions, 1024);
    assert_eq!(spec.dense.distance, VectorDistance::Cosine);
    let sparse = spec.sparse.expect("sparse config");
    assert_eq!(sparse.name, "bm42");
    assert_eq!(sparse.modifier, SparseVectorModifier::Idf);
    assert!(
        spec.payload_indexes
            .iter()
            .any(|index| index.field_name == "source_id"
                && index.field_schema == PayloadFieldSchema::Keyword)
    );
    assert!(
        spec.payload_indexes
            .iter()
            .any(|index| index.field_name == "chunk_index"
                && index.field_schema == PayloadFieldSchema::Integer)
    );
}

#[test]
fn detect_unnamed_mode_collection_uses_default_dense_name() {
    let body = json!({
        "result": { "config": { "params": {
            "vectors": { "size": 384, "distance": "Dot" }
        } } }
    });
    let spec = detect_collection_spec("legacy", &body, ErrorStage::Upserting)
        .expect("valid schema")
        .expect("unnamed spec");
    assert_eq!(spec.dense.name, "dense");
    assert_eq!(spec.dense.dimensions, 384);
    assert_eq!(spec.dense.distance, VectorDistance::Dot);
    assert!(spec.sparse.is_none());
}

#[test]
fn detect_returns_none_for_error_envelope() {
    let body = json!({ "status": { "error": "boom" } });
    assert!(
        detect_collection_spec("axon", &body, ErrorStage::Upserting)
            .expect("error envelope is absence")
            .is_none()
    );
}

#[test]
fn detect_collection_schema_fails_closed_on_unknown_or_missing_values() {
    let cases = [
        json!({"result":{"config":{"params":{"vectors":{"dense":{"size":1024}}}}}}),
        json!({"result":{"config":{"params":{"vectors":{"dense":{"size":1024,"distance":"Angular"}}}}}}),
        json!({"result":{"config":{"params":{"vectors":{"dense":{"size":u64::from(u32::MAX) + 1,"distance":"Cosine"}}}}}}),
        json!({"result":{"config":{"params":{"vectors":{"dense":{"size":1024,"distance":"Cosine"}},"sparse_vectors":{"bm42":{}}}}}}),
        json!({"result":{"config":{"params":{"vectors":{"dense":{"size":1024,"distance":"Cosine"}},"sparse_vectors":{"bm42":{"modifier":"bm25"}}}}}}),
        json!({"result":{"config":{"params":{"vectors":{"dense":{"size":1024,"distance":"Cosine"}}}},"payload_schema":{"tag":{}}}}),
        json!({"result":{"config":{"params":{"vectors":{"dense":{"size":1024,"distance":"Cosine"}}}},"payload_schema":{"tag":{"data_type":"uuid"}}}}),
    ];
    for body in cases {
        let error = detect_collection_spec("axon", &body, ErrorStage::Upserting)
            .expect_err("unknown or missing schema values must fail closed");
        assert_eq!(
            error.code.to_string(),
            "vector.collection_schema_unrecognized"
        );
    }
}

#[test]
fn delete_body_for_points_lists_ids() {
    let selector = VectorDeleteSelector::Points {
        collection: "axon".to_string(),
        point_ids: vec![VectorPointId::new("p1"), VectorPointId::new("p2")],
    };
    let body = delete_body(&selector).expect("delete body");
    assert_eq!(body["points"], json!(["p1", "p2"]));
}

#[test]
fn delete_body_for_chunks_uses_any_match_filter() {
    let selector = VectorDeleteSelector::Chunks {
        collection: "axon".to_string(),
        chunk_ids: vec![ChunkId::new("c1")],
    };
    let body = delete_body(&selector).expect("delete body");
    assert_eq!(body["filter"]["must"][0]["key"], json!("chunk_id"));
    assert_eq!(body["filter"]["must"][0]["match"]["any"], json!(["c1"]));
}

#[test]
fn delete_body_for_generation_fences_on_source_and_generation() {
    let selector = VectorDeleteSelector::Generation {
        collection: "axon".to_string(),
        source_id: SourceId::new("src"),
        generation: SourceGenerationId::new("7"),
    };
    let body = delete_body(&selector).expect("delete body");
    let must = body["filter"]["must"].as_array().expect("must array");
    assert_eq!(must.len(), 2);
    let keys: Vec<&str> = must.iter().filter_map(|c| c["key"].as_str()).collect();
    assert!(keys.contains(&"source_id"));
    assert!(keys.contains(&"source_generation"));
    let generation = must
        .iter()
        .find(|condition| condition["key"] == "source_generation")
        .expect("source generation condition");
    assert_eq!(generation["match"]["value"], json!(7));
}

#[test]
fn generation_delete_uses_server_side_count_and_filter_delete() {
    let filter = generation_delete_filter(&SourceId::new("src"), &SourceGenerationId::new("7"))
        .expect("generation filter");
    let count_body = json!({
        "filter": filter,
        "exact": true,
    });
    let delete_body = json!({ "filter": filter });

    assert_eq!(count_body["filter"]["must"].as_array().unwrap().len(), 2);
    assert_eq!(count_body["exact"], json!(true));
    assert_eq!(delete_body["filter"], count_body["filter"]);
}
