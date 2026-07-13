use super::*;

#[test]
fn exact_match() {
    assert!(url_matches_target(
        "https://x/docs",
        "https://x/docs",
        false
    ));
    assert!(url_matches_target("https://x/docs", "https://x/docs", true));
}

#[test]
fn non_prefix_mode_rejects_descendants() {
    assert!(!url_matches_target(
        "https://x/docs/page",
        "https://x/docs",
        false
    ));
}

#[test]
fn prefix_mode_matches_path_boundary_descendants() {
    assert!(url_matches_target(
        "https://x/docs/page",
        "https://x/docs",
        true
    ));
    assert!(url_matches_target(
        "https://x/docs?query=1",
        "https://x/docs",
        true
    ));
    assert!(url_matches_target(
        "https://x/docs#frag",
        "https://x/docs",
        true
    ));
}

#[test]
fn prefix_mode_does_not_match_sibling_with_shared_prefix() {
    // `https://x/docs-old` shares the literal prefix `https://x/docs` but is
    // not a path descendant of it — must not match.
    assert!(!url_matches_target(
        "https://x/docs-old",
        "https://x/docs",
        true
    ));
}

#[test]
fn prefix_mode_with_trailing_slash_target_matches_starts_with() {
    assert!(url_matches_target(
        "https://x/docs/page",
        "https://x/docs/",
        true
    ));
}

#[test]
fn point_matches_any_canonical_value() {
    assert!(point_matches_url_target(
        &["https://x/docs"],
        "https://x/docs",
        false
    ));
    assert!(point_matches_url_target(
        &["https://x/other", "https://x/docs"],
        "https://x/docs",
        false
    ));
    assert!(!point_matches_url_target(
        &["https://x/other", "https://x/other-seed"],
        "https://x/docs",
        false
    ));
}

#[test]
fn canonical_values_uses_target_payload_fields() {
    let payload = serde_json::json!({
        "url": "https://legacy.example/docs",
        "seed_url": "https://legacy.example",
        "item_canonical_uri": "https://x/docs",
        "source_canonical_uri": "https://x",
        "source_item_key": "docs",
        "chunk_locator": { "canonical_uri": "https://x/docs#chunk" }
    });

    let values = canonical_values(&payload);

    assert!(values.contains(&"https://x/docs"));
    assert!(values.contains(&"https://x"));
    assert!(values.contains(&"docs"));
    assert!(values.contains(&"https://x/docs#chunk"));
    assert!(!values.contains(&"https://legacy.example/docs"));
    assert!(!values.contains(&"https://legacy.example"));
}
