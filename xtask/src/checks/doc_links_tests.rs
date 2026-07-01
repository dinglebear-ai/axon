use super::*;

use std::fs;

#[test]
fn extracts_relative_targets_and_skips_external_and_anchors() {
    let md = "See [a](./a.md) and [b](sub/b.md#frag) and [ext](https://x.y) \
              and [anchor](#top) and ![img](img/p.png) and [mail](mailto:x@y.z).";
    let targets = extract_relative_link_targets(md);
    assert_eq!(targets, vec!["./a.md", "sub/b.md#frag", "img/p.png"]);
}

#[test]
fn strips_fragment_and_query_before_resolving() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.md"), "x").unwrap();
    assert!(link_target_exists(dir.path(), "a.md#section"));
    assert!(link_target_exists(dir.path(), "a.md?v=1"));
    assert!(!link_target_exists(dir.path(), "missing.md#section"));
}

#[test]
fn check_passes_when_all_links_resolve() {
    let root = tempfile::tempdir().unwrap();
    let ref_dir = root.path().join("docs/reference/sub");
    fs::create_dir_all(&ref_dir).unwrap();
    fs::write(ref_dir.join("target.md"), "# target").unwrap();
    fs::write(
        root.path().join("docs/reference/index.md"),
        "[ok](sub/target.md) [ext](https://ok) [anchor](#x)",
    )
    .unwrap();
    check(root.path()).expect("all links resolve");
}

#[test]
fn check_fails_on_broken_relative_link() {
    let root = tempfile::tempdir().unwrap();
    let ref_dir = root.path().join("docs/reference");
    fs::create_dir_all(&ref_dir).unwrap();
    fs::write(ref_dir.join("index.md"), "[bad](does/not/exist.md)").unwrap();
    let err = check(root.path()).expect_err("broken link must fail");
    assert!(err.to_string().contains("does/not/exist.md"));
}

#[test]
fn check_skips_when_reference_dir_absent() {
    let root = tempfile::tempdir().unwrap();
    check(root.path()).expect("absent docs/reference is not a failure");
}

#[test]
fn ignores_title_suffix_in_link() {
    let targets = extract_relative_link_targets("[a](./a.md \"the title\")");
    assert_eq!(targets, vec!["./a.md"]);
}
