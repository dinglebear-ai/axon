use super::client_contract::{
    ClientCrawlRequest, ClientExtractMode, ClientExtractRequest, ClientRoutePreference,
    RestCrawlRequest, RestExtractRequest, RestIngestRequest,
};
use crate::core::config::RenderMode;
use crate::mcp::schema::IngestRequest;

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
fn rest_extract_request_rejects_unimplemented_modes() {
    let err = serde_json::from_value::<RestExtractRequest>(serde_json::json!({
        "urls": ["https://example.com"],
        "mode": "deterministic"
    }))
    .expect_err("unsupported REST extract mode should not deserialize");

    assert!(err.to_string().contains("unknown variant"));
}

#[test]
fn rest_crawl_request_preserves_legacy_render_mode_aliases() {
    for alias in ["auto", "autoswitch", "auto-switch"] {
        let req: RestCrawlRequest = serde_json::from_value(serde_json::json!({
            "urls": ["https://example.com"],
            "render_mode": alias
        }))
        .unwrap_or_else(|err| panic!("deserialize render mode alias {alias}: {err}"));

        assert_eq!(req.render_mode, Some(RenderMode::AutoSwitch));
    }
}

#[test]
fn rest_ingest_request_carries_sessions_options_to_mcp_request() {
    let req: RestIngestRequest = serde_json::from_value(serde_json::json!({
        "source_type": "sessions",
        "sessions": {
            "claude": true,
            "codex": false,
            "gemini": true,
            "project": "axon_rust"
        }
    }))
    .expect("deserialize REST ingest request");

    let mcp_req = IngestRequest::from(req);
    let sessions = mcp_req.sessions.expect("sessions payload");
    assert_eq!(sessions.claude, Some(true));
    assert_eq!(sessions.codex, Some(false));
    assert_eq!(sessions.gemini, Some(true));
    assert_eq!(sessions.project.as_deref(), Some("axon_rust"));
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
        discover_llms_txt: Some(true),
        max_llms_txt_urls: Some(64),
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

#[test]
fn crawl_request_deserializes_missing_transport_fields_as_defaults() {
    let req: ClientCrawlRequest = serde_json::from_value(serde_json::json!({
        "urls": ["https://example.com"],
        "max_pages": 1
    }))
    .expect("deserialize crawl request");

    assert!(req.headers.is_empty());
    assert_eq!(req.route_preference, ClientRoutePreference::Default);
}
