use super::*;

fn manifest_with_config(config: &str) -> SourceManifest {
    let mut metadata = MetadataMap::new();
    metadata.insert(
        super::super::PUBLICATION_CONFIG_KEY.to_string(),
        serde_json::json!(config),
    );
    SourceManifest {
        source_id: SourceId::new("src-config-test"),
        generation: SourceGenerationId::new("gen-config-test"),
        adapter: AdapterRef {
            name: "feed".to_string(),
            version: "test".to_string(),
        },
        scope: SourceScope::Api,
        items: Vec::new(),
        created_at: Timestamp("2026-07-16T00:00:00Z".to_string()),
        metadata,
    }
}

#[test]
fn unchanged_fast_path_requires_the_same_publication_configuration() {
    let manifest = manifest_with_config("cfg-original");

    assert!(publication_config_matches(
        &manifest,
        &ConfigSnapshotId::new("cfg-original")
    ));
    assert!(!publication_config_matches(
        &manifest,
        &ConfigSnapshotId::new("cfg-changed")
    ));
}

#[test]
fn legacy_manifest_without_publication_configuration_is_not_reused() {
    let mut manifest = manifest_with_config("cfg-original");
    manifest.metadata.clear();

    assert!(!publication_config_matches(
        &manifest,
        &ConfigSnapshotId::new("cfg-original")
    ));
}
