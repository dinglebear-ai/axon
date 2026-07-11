use super::*;
use axon_core::config::Config;
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
    // The base for the manifest's `relative_path` (which already includes the
    // `markdown/` segment) is the output dir itself — NOT `<output_dir>/markdown`,
    // which would double the segment when the adapter joins.
    assert_eq!(markdown, PathBuf::from("/data/domains/example.com/sync"));
}

/// `SourceRequest.limits.max_pages` (source-pipeline.md `SourceRequest` table)
/// must reach the crawl config's page cap unchanged.
#[test]
fn max_pages_limit_overrides_crawl_config_page_cap() {
    let cfg = Config::test_default();
    let crawl_cfg = effective_crawl_config_for_source(&cfg, Some(7));
    assert_eq!(crawl_cfg.max_pages, 7);
    assert!(
        !crawl_cfg.embed,
        "web source acquisition must always disable the crawl's own embed pass"
    );
}

/// No `limits.max_pages` in the request keeps the crawl config's own default
/// page cap untouched.
#[test]
fn missing_max_pages_limit_keeps_default_page_cap() {
    let cfg = Config::test_default();
    let crawl_cfg = effective_crawl_config_for_source(&cfg, None);
    assert_eq!(crawl_cfg.max_pages, cfg.max_pages);
}
