use super::*;
use crate::core::config::Config;

#[test]
fn vector_dispatch_failure_redacts_qdrant_url_userinfo_and_query() {
    let mut cfg = Config::test_default();
    cfg.qdrant_url = "https://user:secret@example.com:6333/path?token=secret#frag".to_string();

    let err = std::io::Error::other("boom");
    let service_error = ServiceError::vector_dispatch_failure(
        "query_vector_search_dispatch",
        &cfg,
        12,
        json!({"command": "query"}),
        &err,
    );

    let diag = service_error.diagnostics().expect("diagnostics");
    assert_eq!(diag["qdrant_url"], "https://redacted@example.com:6333/path");
    assert!(!diag.to_string().contains("secret"));
}
