use super::*;

#[test]
fn provider_summaries_projects_the_doctor_services_map() {
    let payload = serde_json::json!({
        "services": {
            "qdrant": {"ok": true, "configured_url": "http://axon-qdrant:6333"},
            "tei": {"ok": false, "detail": "connection refused"},
        }
    });
    let providers = provider_summaries(&payload);
    assert_eq!(providers.len(), 2);
    let qdrant = providers.iter().find(|p| p.id == "qdrant").unwrap();
    assert!(qdrant.ok);
    let tei = providers.iter().find(|p| p.id == "tei").unwrap();
    assert!(!tei.ok);
}

#[test]
fn provider_summaries_is_empty_without_a_services_object() {
    let payload = serde_json::json!({});
    assert!(provider_summaries(&payload).is_empty());
}

#[test]
fn provider_summaries_defaults_missing_ok_field_to_false() {
    let payload = serde_json::json!({"services": {"chrome": {}}});
    let providers = provider_summaries(&payload);
    assert_eq!(providers.len(), 1);
    assert!(!providers[0].ok);
}
