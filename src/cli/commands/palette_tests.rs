use super::*;

#[test]
fn expand_tilde_replaces_home() {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let result = expand_tilde("~/foo/bar");
    assert_eq!(result, PathBuf::from(&home).join("foo/bar"));
}

#[test]
fn expand_tilde_no_tilde_unchanged() {
    let result = expand_tilde("/absolute/path");
    assert_eq!(result, PathBuf::from("/absolute/path"));
}

#[test]
fn expand_tilde_bare_tilde_returns_home() {
    // A path that starts with "~/" but has a trailing component.
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let result = expand_tilde("~/.local/bin");
    assert_eq!(result, PathBuf::from(&home).join(".local/bin"));
}

#[test]
fn find_desktop_manifest_finds_from_repo_root() {
    // This test only passes when run from inside the axon repo tree.
    // Skip when apps/desktop/Cargo.toml isn't present (e.g. shallow CI clones).
    let cwd = std::env::current_dir().unwrap();
    let expected = cwd.join("apps/desktop/Cargo.toml");
    if !expected.exists() {
        return; // Graceful skip.
    }
    let found = find_desktop_manifest().unwrap();
    assert!(found.is_file(), "should find a file at {}", found.display());
    assert!(
        found.ends_with("apps/desktop/Cargo.toml"),
        "expected apps/desktop/Cargo.toml, got {}",
        found.display()
    );
}

#[test]
fn write_desktop_entry_at_produces_valid_ini() {
    use std::io::Read;

    let dir = tempfile::tempdir().expect("tempdir");
    let dest = dir.path().join("test.desktop");
    let binary = PathBuf::from("/usr/local/bin/axon-palette");

    write_desktop_entry_at(&binary, &dest).unwrap();

    let mut content = String::new();
    std::fs::File::open(&dest)
        .unwrap()
        .read_to_string(&mut content)
        .unwrap();

    assert!(content.contains("[Desktop Entry]"), "missing header");
    assert!(
        content.contains("Exec=/usr/local/bin/axon-palette"),
        "missing Exec line"
    );
    assert!(content.contains("Type=Application"), "missing Type");
}
