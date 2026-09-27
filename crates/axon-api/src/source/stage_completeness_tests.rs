use super::*;

#[test]
fn completeness_round_trips_without_inventing_historical_evidence() {
    let mut manifest = SourceManifest {
        source_id: SourceId::from("source"),
        generation: SourceGenerationId::from("generation"),
        adapter: AdapterRef {
            name: "test".into(),
            version: "1".into(),
        },
        scope: SourceScope::File,
        items: Vec::new(),
        created_at: Timestamp("2026-09-27T00:00:00Z".into()),
        metadata: MetadataMap::new(),
    };
    assert_eq!(
        manifest.inventory_completeness(),
        InventoryCompleteness::Unknown
    );
    for invalid in [
        serde_json::json!(true),
        serde_json::json!("Complete"),
        serde_json::json!("future"),
    ] {
        manifest
            .metadata
            .insert(INVENTORY_COMPLETENESS_METADATA_KEY.into(), invalid);
        assert_eq!(
            manifest.inventory_completeness(),
            InventoryCompleteness::Unknown
        );
    }
    for completeness in [
        InventoryCompleteness::Complete,
        InventoryCompleteness::Partial,
        InventoryCompleteness::Unknown,
    ] {
        manifest.set_inventory_completeness(completeness);
        let decoded: SourceManifest =
            serde_json::from_value(serde_json::to_value(&manifest).unwrap()).unwrap();
        assert_eq!(decoded.inventory_completeness(), completeness);
    }
}
