use super::*;

#[test]
fn cache_path_is_deterministic_per_target() {
    let a = reddit_cache_path("r/rust");
    let b = reddit_cache_path("r/rust");
    assert_eq!(a, b, "same target must map to the same cache path");

    // Whitespace around the target does not change the derived path (trimmed).
    assert_eq!(reddit_cache_path("  r/rust  "), a);
}

#[test]
fn cache_path_differs_per_target() {
    assert_ne!(reddit_cache_path("r/rust"), reddit_cache_path("r/golang"));
}

#[test]
fn cache_path_lives_under_axon_reddit_dir() {
    let path = reddit_cache_path("r/rust");
    assert_eq!(
        path.parent().and_then(|p| p.file_name()),
        Some(std::ffi::OsStr::new("axon-reddit")),
        "dump path must live under <tmp>/axon-reddit/"
    );
    assert_eq!(
        path.extension(),
        Some(std::ffi::OsStr::new("json")),
        "dump path must be a .json file"
    );
    assert!(path.starts_with(std::env::temp_dir()));
}

#[test]
fn credentials_guard_requires_both_id_and_secret() {
    // Pure credential resolution — no process-env mutation (this crate denies
    // `unsafe`). Both present => ok.
    let ok = resolve_reddit_credentials(Some("id".into()), Some("secret".into()))
        .expect("both credentials present should resolve");
    assert_eq!(ok, ("id".to_string(), "secret".to_string()));

    // Any missing side => actionable error naming both env vars.
    for (id, secret) in [
        (None, Some("secret".to_string())),
        (Some("id".to_string()), None),
        (None, None),
    ] {
        let err = resolve_reddit_credentials(id, secret).expect_err("missing credential must fail");
        let msg = err.to_string();
        assert!(
            msg.contains("REDDIT_CLIENT_ID") && msg.contains("REDDIT_CLIENT_SECRET"),
            "missing-credentials error must name both env vars, got: {msg}"
        );
    }
}

#[tokio::test]
async fn fetch_reddit_dump_rejects_invalid_target_before_any_fetch() {
    // An invalid target fails at parse time — before credentials are read and
    // before any network call. No env manipulation needed, and no dump file is
    // written.
    let err = fetch_reddit_dump("not a valid target!!")
        .await
        .expect_err("invalid target must fail");
    assert!(
        err.to_string().contains("invalid reddit target"),
        "expected invalid-target error, got: {err}"
    );
    assert!(
        !reddit_cache_path("not a valid target!!").exists(),
        "no dump should be written for an invalid target"
    );
}
