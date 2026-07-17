use super::*;
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

#[tokio::test]
async fn require_collection_spec_uses_cached_spec_without_network() {
    let store = QdrantVectorStore::new("http://127.0.0.1:9", "qdrant-test");
    let expected = collection_spec("axon-cache");
    store.cache_collection_spec(expected.clone()).await;
    let http = store.http().expect("http wrapper");

    let actual = store
        .require_collection_spec(&http, "axon-cache", axon_error::ErrorStage::Retrieving)
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
    let spec = detect_collection_spec("axon", &body).expect("named spec");
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
    let spec = detect_collection_spec("legacy", &body).expect("unnamed spec");
    assert_eq!(spec.dense.name, "dense");
    assert_eq!(spec.dense.dimensions, 384);
    assert_eq!(spec.dense.distance, VectorDistance::Dot);
    assert!(spec.sparse.is_none());
}

#[test]
fn detect_returns_none_for_error_envelope() {
    let body = json!({ "status": { "error": "boom" } });
    assert!(detect_collection_spec("axon", &body).is_none());
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
