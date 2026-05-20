use super::route_meta::{FallbackOutcome, RouteKind, RouteMetadata};

#[test]
fn fallback_equivalent_serializes_as_stable_json() {
    let meta = RouteMetadata {
        route: RouteKind::FallbackLocal,
        fallback: true,
        fallback_outcome: FallbackOutcome::CompletedEquivalent,
        capability_tier: "tier_1_crawl_retrieve".to_string(),
        server_url: Some("http://127.0.0.1:8001".to_string()),
        local_data_dir: Some("/home/user/.axon".to_string()),
        effective_endpoints: serde_json::json!({
            "qdrant": "http://127.0.0.1:53333",
            "embedding": "http://127.0.0.1:52000"
        }),
        warnings: vec!["server unavailable; completed locally".to_string()],
    };

    let json = serde_json::to_value(&meta).expect("serialize route metadata");
    assert_eq!(json["route"], "fallback_local");
    assert_eq!(json["fallback"], true);
    assert_eq!(json["fallback_outcome"], "completed_equivalent");
    assert_eq!(json["warnings"][0], "server unavailable; completed locally");
}
