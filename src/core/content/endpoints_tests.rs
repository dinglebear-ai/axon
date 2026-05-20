use super::*;
use crate::services::types::{EndpointKind, EndpointSourceKind};

#[test]
fn resolves_and_caps_script_sources() {
    let html = r#"
        <script src="/app.js"></script>
        <script src="https://cdn.other.test/lib.js"></script>
        <script src="/app.js"></script>
        <script src="/extra.js"></script>
    "#;

    let (scripts, truncated) = discover_script_sources(html, "https://example.test/docs", 2);

    assert!(truncated);
    assert_eq!(scripts.len(), 2);
    assert_eq!(scripts[0].url, "https://example.test/app.js");
    assert!(scripts[0].first_party);
    assert!(!scripts[1].first_party);
}

#[test]
fn extracts_and_deduplicates_static_endpoints() {
    let html = r#"
        <form action="/api/forms"></form>
        <script>
          const a = "/api/users";
          const b = "/api/users";
          const c = "https://api.example.test/v1/search";
          const d = "wss://stream.example.test/api/socket";
          const e = "/graphql";
        </script>
    "#;
    let report = extract_endpoints(
        html,
        "https://example.test",
        &[],
        &EndpointExtractOptions::default(),
    );

    let values: Vec<_> = report.endpoints.iter().map(|e| e.value.as_str()).collect();
    assert!(values.contains(&"/api/forms"));
    assert!(values.contains(&"/api/users"));
    assert!(values.contains(&"https://api.example.test/v1/search"));
    assert!(values.contains(&"wss://stream.example.test/api/socket"));
    assert!(values.contains(&"/graphql"));
    assert_eq!(
        values
            .iter()
            .filter(|value| **value == "/api/users")
            .count(),
        1
    );
    assert!(report.hosts.contains(&"api.example.test".to_string()));
}

#[test]
fn classifies_sources_kinds_and_hosts() {
    let bundles = vec![PrefetchedBundle {
        url: "https://example.test/assets/app.js".to_string(),
        text: r#"fetch("/v2/items"); new WebSocket("wss://socket.other.test/api")"#.to_string(),
        truncated: false,
    }];
    let report = extract_endpoints(
        "<script src='/assets/app.js'></script>",
        "https://example.test/page",
        &bundles,
        &EndpointExtractOptions::default(),
    );

    let rel = report
        .endpoints
        .iter()
        .find(|endpoint| endpoint.value == "/v2/items")
        .expect("relative endpoint");
    assert_eq!(rel.kind, EndpointKind::RelativePath);
    assert_eq!(rel.source, EndpointSourceKind::ScriptBundle);
    assert!(rel.first_party);
    assert_eq!(
        rel.normalized_url.as_deref(),
        Some("https://example.test/v2/items")
    );

    let ws = report
        .endpoints
        .iter()
        .find(|endpoint| endpoint.kind == EndpointKind::Websocket)
        .expect("websocket endpoint");
    assert!(!ws.first_party);
    assert!(report.hosts.contains(&"socket.other.test".to_string()));
}

#[test]
fn scan_byte_cap_sets_truncated() {
    let html = format!("{}\"/api/late\"", "x".repeat(128));
    let options = EndpointExtractOptions {
        max_scan_bytes: 16,
        ..EndpointExtractOptions::default()
    };

    let report = extract_endpoints(&html, "https://example.test", &[], &options);

    assert!(report.truncated);
    assert!(report.endpoints.is_empty());
}

#[test]
fn malformed_html_still_extracts_best_effort() {
    let html = r#"<html><script>window.api="/api/unclosed"</script><a href="/graphql">"#;
    let report = extract_endpoints(
        html,
        "https://example.test",
        &[],
        &EndpointExtractOptions::default(),
    );

    assert!(report.endpoints.iter().any(|e| e.value == "/api/unclosed"));
    assert!(report.endpoints.iter().any(|e| e.value == "/graphql"));
}
