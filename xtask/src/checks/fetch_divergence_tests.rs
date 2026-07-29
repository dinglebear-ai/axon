use super::*;

#[test]
fn test_files_are_ignored() {
    assert!(is_ignored(
        "crates/axon-adapters/src/web_engine/scrape_tests.rs"
    ));
    assert!(is_ignored("crates/axon-adapters/tests/foo.rs"));
    assert!(is_ignored("crates/axon-adapters/src/testing.rs"));
    assert!(!is_ignored("crates/axon-adapters/src/web_engine/scrape.rs"));
}

#[test]
fn every_approved_exception_carries_a_real_reason() {
    for (path, reason) in APPROVED_EXCEPTIONS {
        assert!(!path.is_empty(), "empty exception path");
        assert!(
            reason.len() > 40,
            "exception for {path} needs a reason a reviewer can evaluate, got: {reason:?}"
        );
    }
}

#[test]
fn approved_exceptions_are_unique() {
    let mut seen = std::collections::HashSet::new();
    for (path, _) in APPROVED_EXCEPTIONS {
        assert!(seen.insert(*path), "duplicate exception entry: {path}");
    }
}

#[test]
fn exception_lookup_matches_exact_paths_only() {
    assert!(is_exception("crates/axon-adapters/src/web_engine/scrape.rs").is_some());
    // A near-miss must NOT inherit an exception.
    assert!(is_exception("crates/axon-adapters/src/web_engine/scrape2.rs").is_none());
    assert!(is_exception("crates/axon-extract/src/verticals/reddit.rs").is_none());
}

#[test]
fn constructor_patterns_cover_every_client_kind_in_use() {
    // reqwest (both builder and new), the ssrf-guarded wrapper, the shared
    // build_client helpers, and wreq. Missing one silently allows drift.
    for expected in [
        "reqwest::Client::builder()",
        "reqwest::Client::new()",
        "build_ssrf_guarded_client_builder(",
        "build_client(",
        "wreq::Client::builder()",
    ] {
        assert!(
            CLIENT_CONSTRUCTORS.contains(&expected),
            "missing constructor pattern: {expected}"
        );
    }
}

#[test]
fn acquisition_roots_cover_the_web_fetching_crates() {
    assert!(ACQUISITION_ROOTS.contains(&"crates/axon-adapters/src"));
    assert!(ACQUISITION_ROOTS.contains(&"crates/axon-extract/src"));
}
