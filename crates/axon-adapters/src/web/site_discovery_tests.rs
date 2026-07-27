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

#[tokio::test]
async fn map_discovery_uses_the_injected_fetch_provider() {
    let _loopback = axon_core::http::LoopbackGuard::allow();
    let server = httpmock::MockServer::start();
    let providers = crate::boundary::FakeAdapterProviders::new()
        .with_fetch_text("<a href=\"/docs/provider-only\">provider result</a>");
    let providers_for_adapter = std::sync::Arc::new(providers.clone());
    let adapter =
        crate::web::WebSourceAdapter::new(providers_for_adapter.clone(), providers_for_adapter);
    let mut plan = crate::web_tests::web_plan(&server.url("/docs"), SourceScope::Map);
    plan.route
        .validated_options
        .values
        .insert("discover_sitemaps".to_string(), serde_json::json!(false));
    plan.route
        .validated_options
        .values
        .insert("discover_llms_txt".to_string(), serde_json::json!(false));

    let manifest = crate::SourceAdapter::discover(&adapter, &plan)
        .await
        .unwrap();

    assert_eq!(manifest.scope, SourceScope::Map);
    assert!(
        manifest
            .items
            .iter()
            .any(|item| item.canonical_uri.ends_with("/provider-only")),
        "Map discovery must consume content returned by the injected FetchProvider"
    );
    assert!(
        providers.calls().await.contains(&"fetch"),
        "Map discovery must use the adapter's FetchProvider rather than a private HTTP client"
    );
}
