use super::*;

#[test]
fn projection_execute_fingerprint_is_stable_and_semantic() {
    let value = serde_json::json!({"operation":"crawl","source":"https://example.test"});
    assert_eq!(digest_json(&value).unwrap(), digest_json(&value).unwrap());
    assert_ne!(
        digest_json(&value).unwrap(),
        digest_json(&serde_json::json!({"operation":"scrape","source":"https://example.test"}))
            .unwrap()
    );
}

#[test]
fn projection_execute_principal_is_opaque() {
    let mut auth = AuthSnapshot::default();
    auth.caller_id = Some("user@example.test".to_string());
    let digest = principal_digest(Some(&auth));
    assert_eq!(digest.len(), 64);
    assert!(!digest.contains("user"));
}
