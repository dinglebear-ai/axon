use super::*;

fn tempdir() -> PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push(format!("axon-palette-files-test-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).expect("create tempdir");
    fs::canonicalize(&dir).expect("canonicalize tempdir")
}

#[test]
fn resolve_within_root_accepts_relative_child_path() {
    let root = tempdir();
    fs::write(root.join("a.txt"), b"hello").unwrap();

    let resolved = resolve_within_root(&root, "a.txt").expect("should resolve");
    assert_eq!(resolved, root.join("a.txt"));

    fs::remove_dir_all(&root).ok();
}

#[test]
fn resolve_within_root_accepts_absolute_child_path() {
    let root = tempdir();
    fs::write(root.join("a.txt"), b"hello").unwrap();
    let absolute = root.join("a.txt").to_string_lossy().into_owned();

    let resolved = resolve_within_root(&root, &absolute).expect("should resolve");
    assert_eq!(resolved, root.join("a.txt"));

    fs::remove_dir_all(&root).ok();
}

#[test]
fn resolve_within_root_rejects_dot_dot_traversal() {
    let root = tempdir();
    fs::create_dir_all(root.join("sub")).unwrap();

    let err = resolve_within_root(&root, "sub/../../etc/passwd").unwrap_err();
    assert!(
        err.contains(".."),
        "expected traversal rejection, got: {err}"
    );

    fs::remove_dir_all(&root).ok();
}

#[test]
fn resolve_within_root_rejects_absolute_path_outside_root() {
    let root = tempdir();

    let err = resolve_within_root(&root, "/etc/passwd").unwrap_err();
    assert!(
        err.contains("escapes") || err.contains("failed to resolve"),
        "expected escape rejection, got: {err}"
    );

    fs::remove_dir_all(&root).ok();
}

#[test]
fn resolve_within_root_rejects_nul_byte() {
    let root = tempdir();
    let err = resolve_within_root(&root, "a\0b").unwrap_err();
    assert!(err.contains("NUL"));
    fs::remove_dir_all(&root).ok();
}

#[cfg(unix)]
#[test]
fn resolve_within_root_rejects_symlink_escaping_root() {
    use std::os::unix::fs::symlink;

    let root = tempdir();
    let outside = tempdir();
    fs::write(outside.join("secret.txt"), b"top secret").unwrap();

    let link_path = root.join("escape_link");
    symlink(&outside, &link_path).expect("create symlink");

    // Following the symlink resolves outside the root — must be rejected even
    // though the symlink itself lives inside the root.
    let err = resolve_within_root(&root, "escape_link/secret.txt").unwrap_err();
    assert!(
        err.contains("escapes"),
        "expected escape rejection, got: {err}"
    );

    fs::remove_dir_all(&root).ok();
    fs::remove_dir_all(&outside).ok();
}

#[cfg(unix)]
#[test]
fn to_entry_skips_symlinked_children() {
    use std::os::unix::fs::symlink;

    let root = tempdir();
    let outside = tempdir();
    fs::write(outside.join("secret.txt"), b"top secret").unwrap();
    symlink(outside.join("secret.txt"), root.join("linked.txt")).expect("create symlink");
    fs::write(root.join("real.txt"), b"real content").unwrap();

    let mut names = Vec::new();
    for entry in fs::read_dir(&root).unwrap() {
        let entry = entry.unwrap();
        if let Some(file_entry) = to_entry(&root, &entry) {
            names.push(file_entry.name);
        }
    }

    assert_eq!(names, vec!["real.txt".to_string()]);

    fs::remove_dir_all(&root).ok();
    fs::remove_dir_all(&outside).ok();
}

#[test]
fn resolve_new_within_root_allows_new_file_in_existing_dir() {
    let root = tempdir();

    let resolved = resolve_new_within_root(&root, "new-file.txt").expect("should resolve");
    assert_eq!(resolved, root.join("new-file.txt"));

    fs::remove_dir_all(&root).ok();
}

#[test]
fn resolve_new_within_root_rejects_dot_dot_traversal() {
    let root = tempdir();

    let err = resolve_new_within_root(&root, "../escape.txt").unwrap_err();
    assert!(err.contains(".."));

    fs::remove_dir_all(&root).ok();
}

#[test]
fn resolve_new_within_root_rejects_parent_outside_root() {
    let root = tempdir();
    let outside = tempdir();

    let absolute = outside.join("new.txt").to_string_lossy().into_owned();
    let err = resolve_new_within_root(&root, &absolute).unwrap_err();
    assert!(err.contains("escapes"));

    fs::remove_dir_all(&root).ok();
    fs::remove_dir_all(&outside).ok();
}

#[cfg(unix)]
#[test]
fn resolve_new_within_root_rejects_symlinked_parent_escaping_root() {
    use std::os::unix::fs::symlink;

    let root = tempdir();
    let outside = tempdir();
    symlink(&outside, root.join("escape_dir")).expect("create symlink");

    let err = resolve_new_within_root(&root, "escape_dir/new.txt").unwrap_err();
    assert!(err.contains("escapes"));

    fs::remove_dir_all(&root).ok();
    fs::remove_dir_all(&outside).ok();
}

#[test]
fn resolve_new_within_root_revalidates_existing_target() {
    let root = tempdir();
    fs::write(root.join("existing.txt"), b"data").unwrap();

    let resolved = resolve_new_within_root(&root, "existing.txt").expect("should resolve");
    assert_eq!(resolved, root.join("existing.txt"));

    fs::remove_dir_all(&root).ok();
}

#[test]
fn files_list_write_read_roundtrip_via_pure_helpers() {
    let root = tempdir();
    fs::create_dir_all(root.join("nested")).unwrap();
    fs::write(root.join("top.txt"), b"top level").unwrap();
    fs::write(root.join("nested/child.txt"), b"nested file").unwrap();

    let mut top_level = Vec::new();
    for entry in fs::read_dir(&root).unwrap() {
        let entry = entry.unwrap();
        if let Some(file_entry) = to_entry(&root, &entry) {
            top_level.push(file_entry);
        }
    }
    top_level.sort_by(|a, b| a.name.cmp(&b.name));
    assert_eq!(top_level.len(), 2);
    assert!(top_level.iter().any(|e| e.name == "top.txt" && !e.is_dir));
    assert!(top_level.iter().any(|e| e.name == "nested" && e.is_dir));

    let resolved = resolve_within_root(&root, "nested/child.txt").expect("resolves");
    let content = fs::read_to_string(&resolved).unwrap();
    assert_eq!(content, "nested file");

    fs::remove_dir_all(&root).ok();
}
