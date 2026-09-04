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
    assert_eq!(normalize_shortcut_label("command+space"), "Cmd+Space");
    assert_eq!(normalize_shortcut_label("super+space"), "Cmd+Space");
    assert_eq!(
        normalize_shortcut_label("command+shift+space"),
        "Cmd+Shift+Space"
    );
    assert_eq!(
        normalize_shortcut_label("control+shift+space"),
        "Ctrl+Shift+Space"
    );
}

#[test]
fn normalize_shortcut_label_falls_back_to_default_for_unknown() {
    assert_eq!(normalize_shortcut_label("not-a-shortcut"), DEFAULT_SHORTCUT);
}

#[test]
fn unchanged_shortcut_does_not_need_duplicate_registration() {
    assert!(!shortcut_needs_registration(
        Some("Cmd+Shift+Space"),
        "Cmd+Shift+Space"
    ));
    assert!(shortcut_needs_registration(
        Some("Ctrl+Space"),
        "Cmd+Shift+Space"
    ));
}

#[test]
fn center_position_uses_final_logical_size_and_monitor_scale() {
    assert_eq!(
        center_position((-3840, 0), (3840, 2160), 2.0, (720.0, 92.0)),
        (-2640, 988)
    );
}
