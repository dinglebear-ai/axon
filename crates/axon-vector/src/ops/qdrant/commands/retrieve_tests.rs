use super::canonical_first_url_candidates;

#[test]
fn canonical_first_url_candidates_try_normalized_before_raw_target() {
    let candidates = canonical_first_url_candidates("example.com/docs/");
    assert_eq!(candidates[0], "https://example.com/docs/");
    assert_eq!(candidates[1], "https://example.com/docs");
    assert_eq!(candidates[2], "example.com/docs/");
}

#[test]
fn canonical_first_url_candidates_deduplicate_normalized_input() {
    let candidates = canonical_first_url_candidates("https://example.com/docs");
    assert_eq!(
        candidates,
        vec![
            "https://example.com/docs".to_string(),
            "https://example.com/docs/".to_string()
        ]
    );
}
