use super::*;

#[test]
fn document_cache_invalidation_rejects_unknown_variant_fields() {
    for payload in [
        serde_json::json!({
            "kind": "source",
            "source_id": "src",
            "extra": true
        }),
        serde_json::json!({
            "kind": "generation",
            "generation": "gen",
            "extra": true
        }),
        serde_json::json!({
            "kind": "key",
            "key": {
                "source_id": "src",
                "source_item_key": "item"
            },
            "extra": true
        }),
        serde_json::json!({
            "kind": "all",
            "extra": true
        }),
    ] {
        assert!(serde_json::from_value::<DocumentCacheInvalidation>(payload).is_err());
    }
}
