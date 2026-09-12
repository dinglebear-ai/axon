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

#[test]
fn bounded_reader_rejects_bytes_arriving_past_the_metadata_size_check() {
    let reader = std::io::Cursor::new(b"sixteen bytes!!!".to_vec());
    let error = read_bounded(reader, Path::new("growing.txt"), 5)
        .expect_err("bytes beyond the admitted limit must be rejected while reading");
    assert_eq!(error.code.to_string(), "adapter.local.file_too_large");
}

#[test]
fn discovery_spool_rejects_content_past_the_file_budget_and_removes_partial_spool() {
    let input = tempfile::NamedTempFile::new().expect("input");
    fs::write(input.path(), b"0123456789").expect("write input");
    let spool_dir = tempfile::tempdir().expect("spool dir");
    let spool = spool_dir.path().join("growing.content");
    let file = File::open(input.path()).expect("open input");

    let error = content_fingerprint_and_spool_from_file(file, Path::new("growing.txt"), &spool, 5)
        .expect_err("spooling must enforce the byte budget");

    assert_eq!(error.code.to_string(), "adapter.local.file_too_large");
    assert!(!spool.exists(), "partial spool must be removed on overflow");
}
