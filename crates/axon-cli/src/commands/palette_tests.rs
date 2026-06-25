use super::*;

#[test]
fn expand_home_replaces_home() {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let result = expand_home("~/foo/bar");
    assert_eq!(result, PathBuf::from(&home).join("foo/bar"));
}

#[test]
fn expand_home_no_tilde_unchanged() {
    let result = expand_home("/absolute/path");
    assert_eq!(result, PathBuf::from("/absolute/path"));
}

#[test]
fn expand_home_bare_tilde_returns_home() {
    // A path that starts with "~/" but has a trailing component.
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let result = expand_home("~/.local/bin");
    assert_eq!(result, PathBuf::from(&home).join(".local/bin"));
}

#[test]
fn find_palette_dir_finds_from_repo_root() {
    // This test only passes when run from inside the axon repo tree.
    // Skip when apps/palette-tauri isn't present (e.g. shallow CI clones).
    let cwd = std::env::current_dir().unwrap();
    let expected = cwd.join("apps/palette-tauri/src-tauri/Cargo.toml");
    if !expected.exists() {
        return; // Graceful skip.
    }
    let found = find_palette_dir().unwrap();
    assert!(
        found.join("src-tauri/Cargo.toml").is_file(),
        "should find src-tauri/Cargo.toml under {}",
        found.display()
    );
    assert!(
        found.ends_with("apps/palette-tauri"),
        "expected apps/palette-tauri, got {}",
        found.display()
    );
}

#[test]
fn write_desktop_entry_at_produces_valid_ini() {
    use std::io::Read;

    let dir = tempfile::tempdir().expect("tempdir");
    let dest = dir.path().join("test.desktop");
    let binary = PathBuf::from("/usr/local/bin/axon-palette-tauri");

    write_desktop_entry_at(&binary, &dest).unwrap();

    let mut content = String::new();
    std::fs::File::open(&dest)
        .unwrap()
        .read_to_string(&mut content)
        .unwrap();

    assert!(content.contains("[Desktop Entry]"), "missing header");
    assert!(
        content.contains("Exec=/usr/local/bin/axon-palette-tauri"),
        "missing Exec line"
    );
    assert!(content.contains("Type=Application"), "missing Type");
}
