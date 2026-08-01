use super::resolvable_identifier;

#[test]
fn uri_start_identifier_offers_canonical_uri() {
    let id = resolvable_identifier(
        "https://docs.rs/anyhow/latest/anyhow",
        "docs_site".to_string(),
    );
    assert_eq!(id.kind, "docs_site");
    assert_eq!(
        id.canonical_uri.as_deref(),
        Some("https://docs.rs/anyhow/latest/anyhow")
    );
    assert!(id.value.is_none());
    assert!(id.node_id.is_none());
}

#[test]
fn non_uri_start_identifier_offers_stable_key_and_node_id() {
    let id = resolvable_identifier("repo_file:abc123", "repo_file".to_string());
    assert_eq!(id.kind, "repo_file");
    assert!(id.canonical_uri.is_none());
    assert_eq!(id.value.as_deref(), Some("repo_file:abc123"));
    assert_eq!(
        id.node_id.as_ref().map(|n| n.0.as_str()),
        Some("repo_file:abc123")
    );
}
