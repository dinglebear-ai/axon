use super::client_contract::{
    ClientCrawlRequest, ClientExtractMode, ClientExtractRequest, ClientRoutePreference,
};
use crate::core::config::RenderMode;

#[test]
fn extract_request_defaults_to_auto_mode() {
    let req = ClientExtractRequest {
        urls: vec!["https://example.com/docs".to_string()],
        prompt: Some("extract title".to_string()),
        mode: None,
        max_pages: Some(1),
        render_mode: Some(RenderMode::Http),
        embed: Some(false),
        headers: vec![],
        route_preference: ClientRoutePreference::Default,
    };

    assert_eq!(req.effective_mode(), ClientExtractMode::Auto);
}

#[test]
fn crawl_request_serializes_all_routing_knobs() {
    let req = ClientCrawlRequest {
        urls: vec!["https://example.com".to_string()],
        max_pages: Some(10),
        max_depth: Some(2),
        render_mode: Some(RenderMode::Http),
        include_subdomains: Some(false),
        respect_robots: Some(true),
        discover_sitemaps: Some(true),
        max_sitemaps: Some(32),
        sitemap_since_days: Some(7),
        delay_ms: Some(25),
        headers: vec![("x-test".to_string(), "1".to_string())],
        route_preference: ClientRoutePreference::ServerRequired,
    };

    let json = serde_json::to_value(&req).expect("serialize crawl request");
    assert_eq!(json["max_pages"], 10);
    assert_eq!(json["max_depth"], 2);
    assert_eq!(json["render_mode"], "http");
    assert_eq!(json["route_preference"], "server_required");
}
