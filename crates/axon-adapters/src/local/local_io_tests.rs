use super::*;

#[cfg(unix)]
#[test]
fn nested_component_symlink_is_rejected() {
    let root = tempfile::tempdir().expect("root");
    let outside = tempfile::tempdir().expect("outside");
    fs::write(outside.path().join("secret.txt"), "secret").expect("outside file");
    std::os::unix::fs::symlink(outside.path(), root.path().join("nested")).expect("nested symlink");

    let held = LocalRootHandle::open(root.path()).expect("hold root");
    let error = held
        .open_file("nested/secret.txt")
        .expect_err("nested symlink traversal must be denied");

    assert_eq!(error.code.0, "adapter.local.item_key.escape");
}

#[cfg(unix)]
#[test]
fn held_root_descriptor_keeps_original_directory_after_path_swap() {
    let parent = tempfile::tempdir().expect("parent");
    let root = parent.path().join("source");
    let moved = parent.path().join("source-held");
    let replacement = parent.path().join("replacement");
    fs::create_dir(&root).expect("root");
    fs::create_dir(&replacement).expect("replacement");
    fs::write(root.join("document.txt"), "original").expect("original file");
    fs::write(replacement.join("document.txt"), "replacement").expect("replacement file");

    let held = LocalRootHandle::open(&root).expect("hold root");
    fs::rename(&root, &moved).expect("move held root");
    std::os::unix::fs::symlink(&replacement, &root).expect("swap visible path");

    let mut file = held
        .open_file("document.txt")
        .expect("open through held root");
    let mut text = String::new();
    file.read_to_string(&mut text).expect("read held file");
    assert_eq!(text, "original");
}
