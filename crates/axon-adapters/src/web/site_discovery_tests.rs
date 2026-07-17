use super::*;

#[test]
fn discovery_config_has_no_disk_output_contract() {
    let plan = crate::web_tests::web_plan("https://example.com/docs", SourceScope::Docs);

    let cfg = build_discovery_config(&plan);

    assert!(cfg.output_dir.as_os_str().is_empty());
    assert!(!cfg.cache);
}

#[test]
fn map_strategy_has_no_crawl_or_disk_handoff() {
    let strategy = include_str!("../web_engine/engine/map/strategy.rs");

    for forbidden in [
        "configure_website",
        ".crawl()",
        ".crawl_raw()",
        "output_dir",
        "manifest.jsonl",
        concat!("map_with_", "sitemap"),
    ] {
        assert!(
            !strategy.contains(forbidden),
            "bounded map strategy must not contain {forbidden}"
        );
    }
}

#[test]
fn manifest_limit_applies_to_map_items_after_sort_and_dedup() {
    let plan = crate::web_tests::web_plan("https://example.com/docs", SourceScope::Map);
    let item = |url: &str| {
        let web = WebUrlParts::parse(url).unwrap();
        web_manifest_item(&plan, &web, None, None, None)
    };

    let items = finalize_items(
        vec![
            item("https://example.com/docs/z"),
            item("https://example.com/docs/a"),
            item("https://example.com/docs/a"),
            item("https://example.com/docs/m"),
        ],
        2,
    );

    assert_eq!(items.len(), 2);
    assert_eq!(
        items[0].canonical_uri.as_str(),
        "https://example.com/docs/a"
    );
    assert_eq!(
        items[1].canonical_uri.as_str(),
        "https://example.com/docs/m"
    );
}
