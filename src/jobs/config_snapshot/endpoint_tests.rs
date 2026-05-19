use super::endpoint_snapshot;

#[test]
fn endpoint_snapshot_falls_back_for_process_local_urls() {
    let mut fallback_fields = Vec::new();

    let snapshot = endpoint_snapshot("tei_url", "http://localhost:80", &mut fallback_fields)
        .expect("valid local endpoint");

    assert_eq!(snapshot, None);
    assert_eq!(fallback_fields, vec!["tei_url".to_string()]);
}

#[test]
fn endpoint_snapshot_rejects_malformed_endpoint_urls() {
    let mut fallback_fields = Vec::new();

    let err = endpoint_snapshot("tei_url", "not a url", &mut fallback_fields)
        .expect_err("malformed endpoint must fail");

    assert!(err.contains("invalid tei_url"), "unexpected error: {err}");
    assert!(fallback_fields.is_empty());
}
