use super::*;

#[test]
fn changelog_paths_are_recognized() {
    assert!(is_changelog_path("CHANGELOG.md"));
    assert!(is_changelog_path("apps/android/CHANGELOG.md"));
    assert!(is_changelog_path("apps/palette-tauri/CHANGELOG.md"));
    assert!(!is_changelog_path("src/lib.rs"));
    assert!(!is_changelog_path("docs/CHANGELOG.md.bak"));
    assert!(!is_changelog_path("apps/android/app/build.gradle.kts"));
}
