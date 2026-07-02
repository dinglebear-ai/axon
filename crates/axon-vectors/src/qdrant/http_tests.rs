use super::*;

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
