use super::*;
use std::path::PathBuf;

#[test]
fn output_dir_reroots_under_domain_sync() {
    let base = PathBuf::from("/tmp/out");
    let dir = crawl_sync_output_dir(&base, "https://example.com/docs/guide");
    assert_eq!(
        dir,
        PathBuf::from("/tmp/out/domains/example.com/sync"),
        "output dir must mirror crawl_sync's domains/<domain>/sync re-rooting"
    );
}

#[test]
fn output_dir_uses_host_without_scheme_or_path() {
    let base = PathBuf::from("/data");
    let dir = crawl_sync_output_dir(&base, "https://docs.rs/serde/latest/serde/");
    assert_eq!(dir, PathBuf::from("/data/domains/docs.rs/sync"));
}

#[test]
fn manifest_and_markdown_paths_derive_from_output_dir() {
    let dir = PathBuf::from("/data/domains/example.com/sync");
    let (manifest, markdown) = crawl_output_manifest_and_markdown(&dir);
    assert_eq!(
        manifest,
        PathBuf::from("/data/domains/example.com/sync/manifest.jsonl")
    );
    assert_eq!(
        markdown,
        PathBuf::from("/data/domains/example.com/sync/markdown")
    );
}
