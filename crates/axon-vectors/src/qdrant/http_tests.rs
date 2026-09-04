use super::*;
use httpmock::{Method::PUT, MockServer};

#[test]
fn endpoint_strips_userinfo_and_query_into_base_and_key() {
    let endpoint = QdrantEndpoint::parse("http://token:secret@qdrant.internal:6333/x?api_key=k1");
    assert_eq!(endpoint.root(), "http://qdrant.internal:6333");
    assert_eq!(
        endpoint.collection_path("axon", "points/query"),
        "http://qdrant.internal:6333/collections/axon/points/query"
    );
    // The base carries no credentials, path, or query.
    assert!(!endpoint.root().contains("secret"));
    assert!(!endpoint.root().contains("token"));
    assert!(!endpoint.root().contains("api_key"));
    assert!(!endpoint.root().ends_with("/x"));
}

#[test]
fn endpoint_extracts_api_key_from_query_when_no_userinfo() {
    let endpoint = QdrantEndpoint::parse("https://host:6333?api_key=abc123");
    assert_eq!(endpoint.root(), "https://host:6333");
    assert_eq!(endpoint.api_key(), Some("abc123"));
}

#[test]
fn endpoint_bare_token_userinfo_is_treated_as_api_key() {
    let endpoint = QdrantEndpoint::parse("http://sometoken@host:6333");
    assert_eq!(endpoint.api_key(), Some("sometoken"));
    assert_eq!(endpoint.root(), "http://host:6333");
}

#[test]
fn endpoint_without_port_keeps_scheme_and_host() {
    let endpoint = QdrantEndpoint::parse("http://localhost");
    assert_eq!(endpoint.root(), "http://localhost");
    assert_eq!(endpoint.api_key(), None);
}

#[test]
fn collection_path_with_empty_suffix_targets_the_collection_root() {
    let endpoint = QdrantEndpoint::parse("http://host:6333");
    assert_eq!(
        endpoint.collection_path("axon", ""),
        "http://host:6333/collections/axon"
    );
}

#[test]
fn qdrant_http_new_reuses_the_shared_client_across_many_constructions() {
    let before = shared_client_build_count();
    for i in 0..5 {
        QdrantHttp::new("http://localhost:6333", &format!("qdrant-{i}"))
            .expect("client construction never fails");
    }
    let after = shared_client_build_count();
    assert!(
        after == before || after == before + 1,
        "the shared client may initialize once, never once per QdrantHttp::new call"
    );
    for i in 5..10 {
        QdrantHttp::new("http://localhost:6333", &format!("qdrant-{i}"))
            .expect("client construction never fails");
    }
    assert_eq!(
        shared_client_build_count(),
        after,
        "later QdrantHttp::new calls must keep reusing the same client"
    );
}

#[tokio::test]
async fn data_put_rejects_conflict_but_idempotent_create_accepts_it() {
    let server = MockServer::start_async().await;
    let conflict = server
        .mock_async(|when, then| {
            when.method(PUT).path("/conflict");
            then.status(409);
        })
        .await;
    let http = QdrantHttp::new(&server.base_url(), "qdrant-test").expect("client");
    let url = format!("{}/conflict", server.base_url());

    let error = http
        .put_json(
            axon_error::ErrorStage::Upserting,
            &url,
            &serde_json::json!({"points": []}),
            "qdrant_upsert",
        )
        .await
        .expect_err("data mutation conflict must fail");
    assert!(error.to_string().contains("409"));

    let error = http
        .put_json(
            axon_error::ErrorStage::Upserting,
            &url,
            &serde_json::json!({"points": []}),
            "qdrant_mark_unchanged_items_committed",
        )
        .await
        .expect_err("carry-forward data conflict must fail");
    assert!(error.to_string().contains("409"));

    let outcome = http
        .put_json_idempotent_create(
            axon_error::ErrorStage::Upserting,
            &url,
            &serde_json::json!({"field_name": "source_id"}),
            "qdrant_payload_index",
        )
        .await
        .expect("idempotent resource creation accepts conflict");
    assert_eq!(outcome, PutCreateOutcome::AlreadyExists);
    conflict.assert_calls_async(3).await;
}
