use axon_core::config::{Config, FreshnessCommand, FreshnessRequest};
use serde_json::json;

use crate::freshness::{
    FreshnessRequestPayload, FreshnessRequestV1, freshness_identity_hash, safe_replay_snapshot,
    validate_freshness_payload_for_dispatch,
};

#[test]
fn safe_replay_snapshot_does_not_persist_secret_headers() {
    let mut cfg = Config::test_default();
    cfg.custom_headers = vec![
        "Authorization: Bearer sk-secret".to_string(),
        "Cookie: sid=secret".to_string(),
        "X-Docs-Version: latest".to_string(),
    ];
    let err = safe_replay_snapshot(&cfg).unwrap_err();
    assert!(
        err.to_string()
            .contains("secret-bearing headers cannot be stored in freshness schedules")
    );
}

#[test]
fn safe_replay_snapshot_strips_freshness_intent() {
    let mut cfg = Config::test_default();
    cfg.freshness = Some(FreshnessRequest {
        command: FreshnessCommand::Scrape,
        every_seconds: 86_400,
    });
    let snapshot = safe_replay_snapshot(&cfg).unwrap();
    assert!(snapshot.freshness_is_stripped);
}

#[test]
fn identity_hash_distinguishes_collection_and_render_mode() {
    let a = freshness_identity_hash(
        "scrape",
        "https://example.com",
        86_400,
        &json!({"url":"https://example.com"}),
        &json!({"collection":"a","render_mode":"http"}),
    );
    let b = freshness_identity_hash(
        "scrape",
        "https://example.com",
        86_400,
        &json!({"url":"https://example.com"}),
        &json!({"collection":"b","render_mode":"http"}),
    );
    assert_ne!(a, b);
}

#[cfg(unix)]
#[test]
fn dispatch_validation_rejects_local_embed_replaced_by_symlink_escape() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let allowed = tmp.path().join("allowed");
    let outside = tmp.path().join("outside");
    std::fs::create_dir_all(&allowed).expect("allowed dir");
    std::fs::create_dir_all(&outside).expect("outside dir");
    let input = allowed.join("doc.md");
    std::fs::write(&input, "# safe").expect("safe file");

    let mut cfg = Config::test_default();
    cfg.mcp_embed_allowed_roots = vec![allowed.clone()];
    let payload = FreshnessRequestPayload::V1(FreshnessRequestV1::Embed {
        input: input.to_string_lossy().to_string(),
    });
    validate_freshness_payload_for_dispatch(&payload, &cfg).expect("initial file is valid");

    std::fs::remove_file(&input).expect("remove file");
    let secret = outside.join("secret.md");
    std::fs::write(&secret, "# secret").expect("outside file");
    std::os::unix::fs::symlink(&secret, &input).expect("symlink");

    let err = validate_freshness_payload_for_dispatch(&payload, &cfg)
        .expect_err("symlink escape must fail at dispatch time");
    assert!(
        err.to_string().contains("must be under one of")
            || err.to_string().contains("must not be a symlink"),
        "unexpected error: {err}"
    );
}
