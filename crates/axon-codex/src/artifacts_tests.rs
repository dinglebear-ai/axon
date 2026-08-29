use super::*;
use std::fs;

#[test]
fn rejects_traversal_links_devices_and_case_collisions() {
    let entry = |path: &str, kind| ArtifactEntry {
        path: path.into(),
        kind,
        expanded_bytes: 1,
    };
    assert!(validate_entries(&[entry("../escape", ArtifactEntryKind::File)]).is_err());
    assert!(validate_entries(&[entry("skill", ArtifactEntryKind::Symlink)]).is_err());
    assert!(validate_entries(&[entry("skill", ArtifactEntryKind::Device)]).is_err());
    assert!(
        validate_entries(&[
            entry("Skill/a", ArtifactEntryKind::File),
            entry("skill/A", ArtifactEntryKind::File)
        ])
        .is_err()
    );
}

#[test]
fn activates_valid_staged_tree_with_single_rename() {
    let root = tempfile::tempdir().unwrap();
    let staged = root.path().join("quarantine");
    fs::create_dir(&staged).unwrap();
    fs::write(staged.join("SKILL.md"), "safe").unwrap();
    let destination = root.path().join("live/skill");
    activate_directory(&staged, &destination).unwrap();
    assert_eq!(
        fs::read_to_string(destination.join("SKILL.md")).unwrap(),
        "safe"
    );
    assert!(!staged.exists());
}
