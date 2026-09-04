use super::*;

#[test]
fn endpoint_strips_userinfo_and_query_into_base_and_key() {
    let endpoint = QdrantEndpoint::parse("http://token:secret@qdrant.internal:6333/x?api_key=k1");
    assert_eq!(endpoint.root(), "http://qdrant.internal:6333/x");
    assert_eq!(
        endpoint.collection_path("axon", "points/query"),
        "http://qdrant.internal:6333/x/collections/axon/points/query"
    );
    // The base carries no credentials or query, while retaining its proxy prefix.
    assert!(!endpoint.root().contains("secret"));
    assert!(!endpoint.root().contains("token"));
    assert!(!endpoint.root().contains("api_key"));
}

#[test]
fn endpoint_extracts_api_key_from_query_when_no_userinfo() {
    let endpoint = QdrantEndpoint::parse("https://host:6333?api_key=abc123");
    assert_eq!(endpoint.root(), "https://host:6333");
    assert_eq!(endpoint.api_key(), Some("abc123"));
}

#[test]
fn remote_plaintext_endpoint_rejects_credentials() {
    let error = QdrantHttp::new("http://token@qdrant.internal:6333", "qdrant")
        .expect_err("remote credentials over plaintext HTTP must fail closed");
    assert_eq!(error.code.0, "vector.qdrant.insecure_credentials");
    assert!(!error.to_string().contains("token"));
}

#[test]
fn loopback_plaintext_endpoint_allows_credentials_for_local_development() {
    QdrantHttp::new("http://token@127.0.0.1:6333", "qdrant")
        .expect("loopback HTTP credentials stay available for local development");
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
fn endpoint_preserves_ipv6_and_configured_path_prefix() {
    let endpoint = QdrantEndpoint::parse("http://[2001:db8::1]:6333/qdrant/v1/");
    assert_eq!(endpoint.root(), "http://[2001:db8::1]:6333/qdrant/v1");
    assert_eq!(
        endpoint.collection_path("team docs", "points/query?wait=true"),
        "http://[2001:db8::1]:6333/qdrant/v1/collections/team%20docs/points/query?wait=true"
    );
}

#[test]
fn endpoint_removes_credentials_without_discarding_prefix() {
    let endpoint = QdrantEndpoint::parse(
        "https://token:secret@example.test/api/qdrant?api_key=other#fragment",
    );
    assert_eq!(endpoint.root(), "https://example.test/api/qdrant");
    assert_eq!(endpoint.api_key(), Some("secret"));
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
