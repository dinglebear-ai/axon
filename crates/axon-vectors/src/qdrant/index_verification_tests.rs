use super::*;

#[tokio::test]
async fn successful_index_put_still_verifies_persisted_schema() {
    let actual = json!({
        "result": {
            "config": { "params": {
                "vectors": { "dense": { "size": 1024, "distance": "Cosine" } },
                "sparse_vectors": { "bm42": { "modifier": "idf" } }
            } },
            "payload_schema": { "source_generation": { "data_type": "keyword" } }
        }
    })
    .to_string();
    let (base_url, server) = sequential_response_server(vec![(200, String::new()), (200, actual)]);
    let store = QdrantVectorStore::new(base_url, "qdrant-test");
    let mut spec = collection_spec("axon-generation-index");
    spec.payload_indexes = vec![PayloadIndexSpec {
        field_name: "source_generation".into(),
        field_schema: PayloadFieldSchema::Integer,
        required_for_filters: false,
    }];
    let error = store
        .ensure_payload_indexes(&store.http().unwrap(), &spec, ErrorStage::Upserting)
        .await
        .expect_err("HTTP success is insufficient evidence of the persisted index type");
    assert_eq!(error.code.to_string(), "vector.collection_drift");
    server.join().unwrap();
}

#[tokio::test]
async fn successful_index_put_with_no_persisted_index_is_rejected() {
    let actual = json!({"result": {
        "config": {"params": {
            "vectors": {"dense": {"size": 1024, "distance": "Cosine"}},
            "sparse_vectors": {"bm42": {"modifier": "idf"}}
        }}, "payload_schema": {}
    }})
    .to_string();
    let (base_url, server) = sequential_response_server(vec![(200, String::new()), (200, actual)]);
    let store = QdrantVectorStore::new(base_url, "qdrant-test");
    let spec = collection_spec("axon-missing-index");
    let error = store
        .ensure_payload_indexes(&store.http().unwrap(), &spec, ErrorStage::Upserting)
        .await
        .expect_err("index must be present after successful PUT");
    assert_eq!(
        error.code.to_string(),
        "vector.payload_index_verification_failed"
    );
    server.join().unwrap();
}

#[tokio::test]
async fn integer_generation_index_wire_request_and_persisted_type_agree() {
    let server = httpmock::MockServer::start_async().await;
    let put = server
        .mock_async(|when, then| {
            when.method("PUT")
                .path("/collections/axon-generation-index/index")
                .json_body(json!({"field_name":"source_generation","field_schema":"integer"}));
            then.status(200);
        })
        .await;
    let get = server
        .mock_async(|when, then| {
            when.method("GET")
                .path("/collections/axon-generation-index");
            then.status(200).json_body(json!({"result": {
                "config": {"params": {
                    "vectors": {"dense": {"size":1024,"distance":"Cosine"}},
                    "sparse_vectors": {"bm42":{"modifier":"idf"}}
                }}, "payload_schema": {"source_generation":{"data_type":"integer","points":0}}
            }}));
        })
        .await;
    let store = QdrantVectorStore::new(server.base_url(), "qdrant-test");
    let mut spec = collection_spec("axon-generation-index");
    spec.payload_indexes = vec![PayloadIndexSpec {
        field_name: "source_generation".into(),
        field_schema: PayloadFieldSchema::Integer,
        required_for_filters: true,
    }];
    store
        .ensure_payload_indexes(&store.http().unwrap(), &spec, ErrorStage::Upserting)
        .await
        .unwrap();
    put.assert_calls_async(1).await;
    get.assert_calls_async(1).await;
}

#[tokio::test]
async fn existing_keyword_generation_index_fails_before_any_index_writes() {
    let server = httpmock::MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method("GET")
                .path("/collections/axon-generation-index");
            then.status(200).json_body(json!({"result": {
                "config": {"params": {
                    "vectors": {"dense": {"size":1024,"distance":"Cosine"}},
                    "sparse_vectors": {"bm42":{"modifier":"idf"}}
                }}, "payload_schema": {"source_generation":{"data_type":"keyword","points":0}}
            }}));
        })
        .await;
    let put = server
        .mock_async(|when, then| {
            when.method("PUT");
            then.status(200);
        })
        .await;
    let store = QdrantVectorStore::new(server.base_url(), "qdrant-test");
    let error = store
        .ensure_collection_inner(collection_spec("axon-generation-index"))
        .await
        .expect_err("legacy keyword index must not be silently accepted");
    assert_eq!(error.code.to_string(), "vector.collection_drift");
    put.assert_calls_async(0).await;
}
