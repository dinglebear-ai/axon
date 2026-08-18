use std::path::Path;

use super::*;

#[test]
fn merge_settings_uses_internal_default_collection_when_persisted_collection_missing() {
    let merged = merge_settings(PartialPaletteSettings::default(), default_settings());

    assert_eq!(merged.collection, "axon");
    assert!(merged.hide_on_blur);
}

#[test]
fn merge_settings_keeps_legacy_persisted_collection() {
    let persisted = PartialPaletteSettings {
        collection: Some("saved".to_string()),
        ..PartialPaletteSettings::default()
    };

    let merged = merge_settings(persisted, default_settings());

    assert_eq!(merged.collection, "saved");
}

#[test]
fn parse_settings_json_reports_path_on_malformed_settings() {
    let path = Path::new("/tmp/axon-palette/settings.json");
    let err = parse_settings_json("{not json", path).expect_err("malformed settings fail");

    assert!(err.contains("/tmp/axon-palette/settings.json"));
    assert!(err.contains("failed to parse palette settings"));
}

#[test]
fn normalize_shortcut_label_canonicalizes_known_aliases() {
    assert_eq!(normalize_shortcut_label("option+space"), "Alt+Space");
    assert_eq!(normalize_shortcut_label("control+space"), "Ctrl+Space");
    assert_eq!(
        normalize_shortcut_label("command+shift+space"),
        "Cmd+Shift+Space"
    );
}

#[test]
fn normalize_shortcut_label_falls_back_to_default_for_unknown() {
    assert_eq!(normalize_shortcut_label("not-a-shortcut"), DEFAULT_SHORTCUT);
}
