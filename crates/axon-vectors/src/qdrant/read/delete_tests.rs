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
fn point_matches_via_url_or_seed_url() {
    assert!(point_matches_url_target(
        Some("https://x/docs"),
        None,
        "https://x/docs",
        false
    ));
    assert!(point_matches_url_target(
        None,
        Some("https://x/docs"),
        "https://x/docs",
        false
    ));
    assert!(!point_matches_url_target(
        Some("https://x/other"),
        Some("https://x/other-seed"),
        "https://x/docs",
        false
    ));
}
