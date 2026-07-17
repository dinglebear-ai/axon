use super::{
    build_map_request, parse_map_result, source_result_map_failure, unsupported_map_result,
};
use crate::source::classify::SourceInputKind;
use crate::source::result_map::{adapter_ref, degraded_no_data_plane};
use crate::source::routing::resolve_source_route;
use crate::types::MapOptions;
use axon_api::source::{SourceIntent, SourceKind, SourceScope};
use serde_json::json;

// ── parse_map_result ──────────────────────────────────────────────────────

#[test]
fn parse_map_result_valid_full() {
    let v = json!({
        "url": "https://example.com",
        "mapped_urls": 3u64,
        "total": 3u64,
        "sitemap_urls": 3usize,
        "pages_seen": 2u32,
        "thin_pages": 0u32,
        "elapsed_ms": 100u64,
        "map_source": "sitemap",
        "warning": null,
        "urls": [
            "https://example.com/a",
            "https://example.com/b",
            "https://example.com/c"
        ]
    });
    let result = parse_map_result(v).unwrap();
    assert_eq!(result.url, "https://example.com");
    assert_eq!(result.returned_url_count, 3);
    assert_eq!(result.sitemap_urls, 3);
    assert_eq!(result.pages_seen, 2);
    assert_eq!(result.thin_pages, 0);
    assert_eq!(result.elapsed_ms, 100);
    assert_eq!(result.map_source, "sitemap");
    assert!(result.warning.is_none());
    assert_eq!(result.urls.len(), 3);
    assert_eq!(result.urls[0], "https://example.com/a");
}

#[test]
fn parse_map_result_with_warning() {
    let v = json!({
        "url": "https://example.com",
        "mapped_urls": 1u64,
        "total": 1u64,
        "sitemap_urls": 0usize,
        "pages_seen": 0u32,
        "thin_pages": 0u32,
        "elapsed_ms": 50u64,
        "map_source": "bounded-structure",
        "warning": "too few urls found",
        "urls": ["https://example.com/"]
    });
    let result = parse_map_result(v).unwrap();
    assert_eq!(result.warning.as_deref(), Some("too few urls found"));
    assert_eq!(result.map_source, "bounded-structure");
}

#[test]
fn parse_map_result_missing_url() {
    let v = json!({
        "mapped_urls": 0u64,
        "total": 0u64,
        "sitemap_urls": 0usize,
        "pages_seen": 0u32,
        "thin_pages": 0u32,
        "elapsed_ms": 0u64,
        "map_source": "sitemap",
        "warning": null,
        "urls": []
    });
    let err = parse_map_result(v).unwrap_err();
    assert!(
        err.to_string().contains("url") || err.to_string().contains("missing field"),
        "error must mention missing field, got: {err}"
    );
}

#[test]
fn parse_map_result_missing_mapped_urls() {
    let v = json!({
        "url": "https://example.com",
        "total": 0u64,
        "sitemap_urls": 0usize,
        "pages_seen": 0u32,
        "thin_pages": 0u32,
        "elapsed_ms": 0u64,
        "map_source": "sitemap",
        "warning": null,
        "urls": []
    });
    let err = parse_map_result(v).unwrap_err();
    assert!(
        err.to_string().contains("mapped_urls") || err.to_string().contains("missing field"),
        "error must mention missing field, got: {err}"
    );
}

#[test]
fn parse_map_result_missing_urls_array() {
    let v = json!({
        "url": "https://example.com",
        "mapped_urls": 0u64,
        "total": 0u64,
        "sitemap_urls": 0usize,
        "pages_seen": 0u32,
        "thin_pages": 0u32,
        "elapsed_ms": 0u64,
        "map_source": "sitemap",
        "warning": null
    });
    let err = parse_map_result(v).unwrap_err();
    assert!(
        err.to_string().contains("urls") || err.to_string().contains("missing field"),
        "error must mention missing field, got: {err}"
    );
}

#[test]
fn parse_map_result_empty_urls_array() {
    let v = json!({
        "url": "https://example.com",
        "mapped_urls": 0u64,
        "total": 0u64,
        "sitemap_urls": 0usize,
        "pages_seen": 0u32,
        "thin_pages": 0u32,
        "elapsed_ms": 0u64,
        "map_source": "bounded-structure",
        "warning": null,
        "urls": []
    });
    let result = parse_map_result(v).unwrap();
    assert!(result.urls.is_empty());
    assert_eq!(result.returned_url_count, 0);
}

#[test]
fn parse_map_result_round_trips_via_serde() {
    let original = crate::types::MapResult {
        url: "https://example.com".to_string(),
        returned_url_count: 2,
        total: 10,
        sitemap_urls: 5,
        pages_seen: 1,
        thin_pages: 0,
        elapsed_ms: 300,
        map_source: "bounded-structure".to_string(),
        warning: Some("low coverage".to_string()),
        urls: vec![
            "https://example.com/a".to_string(),
            "https://example.com/b".to_string(),
        ],
    };
    let v = serde_json::to_value(&original).unwrap();
    let parsed = parse_map_result(v).unwrap();
    assert_eq!(original, parsed);
}

// ── source-pipeline routing (source-pipeline.md SourceRequest.intent=map) ──

#[test]
fn build_map_request_sets_map_intent_no_embed_map_scope() {
    let request = build_map_request("https://example.com/docs");

    assert_eq!(request.source, "https://example.com/docs");
    assert_eq!(request.intent, SourceIntent::Map);
    assert!(!request.embed, "map must never write vectors");
    assert_eq!(request.scope, Some(SourceScope::Map));
    assert_eq!(
        request.adapter, None,
        "the resolver must select the adapter"
    );
}

#[test]
fn plain_web_url_routes_through_resolver_and_router_as_web_kind() {
    let request = build_map_request("https://example.com/docs");
    let routed = resolve_source_route(&request).expect("plain web url routes");

    assert_eq!(routed.kind, SourceInputKind::Web);
    assert_eq!(routed.route.scope, SourceScope::Map);
}

#[test]
fn git_url_map_scope_is_rejected_by_the_router_not_silently_treated_as_web() {
    // A GitHub repo URL is http(s) but must classify as `Git`, so the router
    // rejects the map-scope request (the git adapter has no map scope)
    // instead of silently falling through to the web catch-all the way the
    // pre-pipeline map path did. This is exactly the routing failure
    // `discover` degrades to `unsupported_map_result` for.
    let request = build_map_request("https://github.com/jmagar/axon");
    let err = resolve_source_route(&request).expect_err("git adapter has no map scope");

    assert_eq!(err.code.0, "source.scope.unsupported");
}

#[test]
fn unsupported_map_result_is_degraded_with_zero_urls_and_no_vectors() {
    let result = unsupported_map_result(
        "https://github.com/jmagar/axon",
        "map route error: adapter does not support requested source scope",
    );

    assert_eq!(result.url, "https://github.com/jmagar/axon");
    assert_eq!(result.map_source, "unsupported");
    assert_eq!(result.returned_url_count, 0);
    assert_eq!(result.total, 0);
    assert!(result.urls.is_empty());
    assert!(result.warning.is_some());
}

#[test]
fn failed_source_result_becomes_degraded_map_result_without_manifest_projection() {
    let source_result = degraded_no_data_plane(
        "https://example.com/docs",
        SourceKind::Web,
        adapter_ref("web"),
        SourceScope::Map,
    );

    let result = source_result_map_failure("https://example.com/docs", &source_result);

    assert_eq!(result.url, "https://example.com/docs");
    assert_eq!(result.map_source, "unsupported");
    assert_eq!(result.returned_url_count, 0);
    assert_eq!(result.total, 0);
    assert!(result.urls.is_empty());
    assert!(
        result
            .warning
            .as_deref()
            .is_some_and(|warning| warning.contains("data plane")),
        "warning should preserve source pipeline failure reason: {:?}",
        result.warning
    );
}

#[tokio::test]
async fn discover_degrades_non_web_git_source_to_unsupported_result_without_crawling() {
    // Exercises the same branch `discover` takes internally, without paying
    // for the outer live-DNS `validate_url_with_dns` gate that a github.com
    // hostname would otherwise require network access for in this sandbox:
    // route the request directly and confirm the router-rejection path feeds
    // `unsupported_map_result` rather than ever reaching web URL discovery.
    let url = "https://github.com/jmagar/axon";
    let request = build_map_request(url);
    let err = resolve_source_route(&request).expect_err("git adapter has no map scope");

    let result = unsupported_map_result(url, format!("map route error: {err}"));
    assert_eq!(result.map_source, "unsupported");
    assert_eq!(result.returned_url_count, 0);
    assert!(result.urls.is_empty());
}
