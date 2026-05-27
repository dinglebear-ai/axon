use std::path::Path;

use super::*;

#[test]
fn merge_settings_uses_default_collection_when_persisted_collection_missing() {
    let defaults = default_settings(&[("AXON_COLLECTION".to_string(), "docs".to_string())]);

    let merged = merge_settings(PartialPaletteSettings::default(), defaults);

    assert_eq!(merged.collection, "docs");
    assert!(merged.hide_on_blur);
}

#[test]
fn merge_settings_keeps_persisted_collection_over_default() {
    let defaults = default_settings(&[("AXON_COLLECTION".to_string(), "docs".to_string())]);
    let persisted = PartialPaletteSettings {
        collection: Some("saved".to_string()),
        ..PartialPaletteSettings::default()
    };

    let merged = merge_settings(persisted, defaults);

    assert_eq!(merged.collection, "saved");
}

#[test]
fn parse_settings_json_reports_path_on_malformed_settings() {
    let path = Path::new("/tmp/axon-palette/settings.json");
    let err = parse_settings_json("{not json", path).expect_err("malformed settings fail");

    assert!(err.contains("/tmp/axon-palette/settings.json"));
    assert!(err.contains("failed to parse palette settings"));
}
