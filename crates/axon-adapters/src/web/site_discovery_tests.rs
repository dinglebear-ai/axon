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
