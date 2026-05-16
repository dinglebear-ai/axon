use super::*;

#[test]
fn product_authority_ratio_counts_docs_like_urls_with_query_product_token() {
    let candidates = vec![
        make_candidate(
            "https://docs.widget.dev/docs/en/plugins",
            "Widget plugins marketplace official docs",
            0.8,
        )
        .candidate,
        make_candidate(
            "https://docs.other.dev/cli/plugins",
            "Other plugins marketplace commands install list inspect",
            0.7,
        )
        .candidate,
    ];
    let query_tokens = vec!["widget".to_string(), "plugins".to_string()];

    let ratio = product_authority_ratio(&candidates, &query_tokens, 0.35);

    assert!((ratio - 0.5).abs() < f64::EPSILON);
}

#[test]
fn product_authority_boost_ignores_malformed_urls() {
    let query_tokens = vec!["uv".to_string()];
    assert_eq!(
        product_authority_boost_for_url("not a url", &query_tokens, 0.35),
        0.0
    );
}

#[test]
fn product_authority_ratio_is_zero_without_matching_product_token() {
    let candidates = vec![
        make_candidate(
            "https://docs.widget.dev/docs/en/plugins",
            "Widget plugins marketplace official docs",
            0.8,
        )
        .candidate,
    ];
    let query_tokens = vec!["plugins".to_string()];

    assert_eq!(
        product_authority_ratio(&candidates, &query_tokens, 0.35),
        0.0
    );
}

#[test]
fn product_authority_ratio_ignores_generic_package_publish_tokens() {
    let candidates = vec![
        make_candidate(
            "https://docs.astral.sh/uv/guides/package",
            "Publish your package with uv publish",
            0.8,
        )
        .candidate,
    ];
    let query_tokens = vec![
        "publish".to_string(),
        "python".to_string(),
        "package".to_string(),
        "pypi".to_string(),
    ];

    assert_eq!(
        product_authority_ratio(&candidates, &query_tokens, 0.35),
        0.0
    );
}

#[test]
fn product_authority_ratio_ignores_language_tokens_as_product_identity() {
    let candidates = vec![
        make_candidate(
            "https://playwright.dev/python/docs/ci",
            "Python continuous integration guide",
            0.8,
        )
        .candidate,
    ];
    let query_tokens = vec![
        "uv".to_string(),
        "python".to_string(),
        "dependencies".to_string(),
    ];

    assert_eq!(
        product_authority_ratio(&candidates, &query_tokens, 0.35),
        0.0
    );
}

#[test]
fn product_authority_ratio_requires_host_or_early_path_identity_match() {
    let candidates = vec![
        make_candidate(
            "https://www.postgresql.org/docs/current/runtime-config-error-handling.html",
            "PostgreSQL runtime error handling docs",
            0.8,
        )
        .candidate,
    ];
    let query_tokens = vec![
        "rust".to_string(),
        "error".to_string(),
        "handling".to_string(),
    ];

    assert_eq!(
        product_authority_ratio(&candidates, &query_tokens, 0.35),
        0.0
    );
}

#[test]
fn product_authority_ratio_counts_early_path_identity_match() {
    let candidates = vec![
        make_candidate(
            "https://docs.astral.sh/uv/concepts/projects/dependencies",
            "uv Python dependency management",
            0.8,
        )
        .candidate,
    ];
    let query_tokens = vec![
        "uv".to_string(),
        "python".to_string(),
        "dependencies".to_string(),
    ];

    assert_eq!(
        product_authority_ratio(&candidates, &query_tokens, 0.35),
        1.0
    );
}

#[test]
fn score_policy_boosts_docs_rs_crate_page_for_crate_queries() {
    let candidates = vec![
        make_candidate(
            "https://docs.other.dev/docs/layout",
            "Other layout documentation with rendering concepts",
            0.30,
        ),
        make_candidate(
            "https://docs.rs/gpui/latest/gpui/",
            "GPUI crate documentation application views windows render",
            0.20,
        ),
    ];
    let query_tokens = vec!["gpui".to_string(), "views".to_string()];
    let policy = CandidateScorePolicy {
        authoritative_domains: &[],
        authoritative_boost: 0.0,
        product_authority_boost: 0.35,
        min_relevance_score: None,
        require_topical_overlap: true,
    };

    let selected = score_and_filter_candidates(&candidates, &query_tokens, &policy);

    assert_eq!(
        selected[0].candidate.url,
        "https://docs.rs/gpui/latest/gpui/"
    );
}
