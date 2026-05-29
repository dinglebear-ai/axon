use super::*;
use crate::services::types::{EndpointKind, EndpointSourceKind};

#[test]
fn resolves_and_caps_script_sources() {
    let html = r#"
        <script src="/app.js"></script>
        <script src="https://cdn.other.test/lib.js"></script>
        <script src=/unquoted.js></script>
        <script src="/app.js"></script>
        <script src="/extra.js"></script>
    "#;

    let (scripts, truncated) = discover_script_sources(html, "https://example.test/docs", 3);

    assert!(truncated);
    assert_eq!(scripts.len(), 3);
    assert_eq!(scripts[0].url, "https://example.test/app.js");
    assert!(scripts[0].first_party);
    assert!(!scripts[1].first_party);
    assert_eq!(scripts[2].url, "https://example.test/unquoted.js");
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

#[test]
fn html_attribute_scan_respects_byte_cap() {
    let html = format!("{}<a href=\"/api/late\">", "x".repeat(128));
    let options = EndpointExtractOptions {
        max_scan_bytes: 16,
        ..EndpointExtractOptions::default()
    };

    let report = extract_endpoints(&html, "https://example.test", &[], &options);

    assert!(report.truncated);
    assert!(report.endpoints.is_empty());
}

#[test]
fn matches_graphql_attributes_case_insensitively() {
    let html = r#"<a href="/GraphQL"></a>"#;

    let report = extract_endpoints(
        html,
        "https://example.test",
        &[],
        &EndpointExtractOptions::default(),
    );

    let endpoint = report
        .endpoints
        .iter()
        .find(|endpoint| endpoint.value == "/GraphQL")
        .expect("GraphQL endpoint");
    assert_eq!(endpoint.kind, EndpointKind::Graphql);
}

#[test]
fn matches_graphql_absolute_schemes_case_insensitively() {
    let html = r#"<script>const endpoint = "HTTPS://api.example.test/GraphQL"</script>"#;

    let report = extract_endpoints(
        html,
        "https://example.test",
        &[],
        &EndpointExtractOptions::default(),
    );

    let endpoint = report
        .endpoints
        .iter()
        .find(|endpoint| endpoint.value == "HTTPS://api.example.test/GraphQL")
        .expect("uppercase scheme GraphQL endpoint");
    assert_eq!(endpoint.kind, EndpointKind::Graphql);
    assert_eq!(
        endpoint.normalized_url.as_deref(),
        Some("https://api.example.test/GraphQL")
    );
}

#[test]
fn protocol_relative_urls_are_not_first_party_by_path_prefix() {
    let html = r#"<script>fetch("//api.other.test/graphql")</script>"#;

    let report = extract_endpoints(
        html,
        "https://example.test",
        &[],
        &EndpointExtractOptions::default(),
    );

    let endpoint = report
        .endpoints
        .iter()
        .find(|endpoint| endpoint.value == "//api.other.test/graphql")
        .expect("protocol-relative endpoint");
    assert!(!endpoint.first_party);
    assert_eq!(
        endpoint.normalized_url.as_deref(),
        Some("https://api.other.test/graphql")
    );
}

#[test]
fn registrable_domain_two_label_tld() {
    assert_eq!(registrable_domain("api.example.com"), "example.com");
    assert_eq!(registrable_domain("example.com"), "example.com");
    assert_eq!(registrable_domain("a.b.example.com"), "example.com");
}

#[test]
fn registrable_domain_multi_label_tld() {
    assert_eq!(
        registrable_domain("api.ticketmaster.co.uk"),
        "ticketmaster.co.uk"
    );
    assert_eq!(registrable_domain("www.shop.com.au"), "shop.com.au");
    assert_eq!(
        registrable_domain("ticketmaster.co.uk"),
        "ticketmaster.co.uk"
    );
}

#[test]
fn first_party_multi_label_tld_subdomain() {
    // api.example.co.uk and www.example.co.uk share registrable domain
    assert!(host_is_first_party(
        Some("api.example.co.uk"),
        "www.example.co.uk"
    ));
    assert!(host_is_first_party(
        Some("example.co.uk"),
        "www.example.co.uk"
    ));
    // different registrable domains must be third-party
    assert!(!host_is_first_party(
        Some("api.other.co.uk"),
        "www.example.co.uk"
    ));
}

#[test]
fn valid_absolute_host_rejects_minifier_garbage() {
    // single-label hosts
    assert!(!is_valid_absolute_host("http://n/path"));
    assert!(!is_valid_absolute_host("http://f"));
    // single-char TLD
    assert!(!is_valid_absolute_host("http://foo.b/path"));
    // valid hosts pass
    assert!(is_valid_absolute_host("https://api.example.com/v1"));
    assert!(is_valid_absolute_host("https://example.co.uk/api"));
    // relative paths pass through (caller decides)
    assert!(is_valid_absolute_host("/api/v1/search"));
}

#[test]
fn noise_hosts_filtered() {
    let html = r#"<script>
        fetch("https://schema.org/action");
        fetch("https://w3.org/ns/activitystreams");
        fetch("https://example.com/api/test");
        fetch("https://example.net/rest/v1");
        fetch("https://api.real.com/v1/users");
    </script>"#;
    let report = extract_endpoints(
        html,
        "https://real.com",
        &[],
        &EndpointExtractOptions::default(),
    );
    let values: Vec<_> = report.endpoints.iter().map(|e| e.value.as_str()).collect();
    assert!(!values.iter().any(|v| v.contains("schema.org")));
    assert!(!values.iter().any(|v| v.contains("w3.org")));
    assert!(!values.iter().any(|v| v.contains("example.com")));
    assert!(!values.iter().any(|v| v.contains("example.net")));
    assert!(values.iter().any(|v| v.contains("api.real.com")));
}

#[test]
fn static_asset_extensions_filtered() {
    let html = r#"<script>
        const a = "https://cdn.real.test/v1/bundle.js";
        const b = "https://cdn.real.test/images/logo.png";
        const c = "https://cdn.real.test/fonts/inter.woff2";
        const d = "https://api.real.test/v1/upload";
    </script>"#;
    let report = extract_endpoints(
        html,
        "https://real.test",
        &[],
        &EndpointExtractOptions::default(),
    );
    let values: Vec<_> = report.endpoints.iter().map(|e| e.value.as_str()).collect();
    assert!(!values.iter().any(|v| v.ends_with(".js")));
    assert!(!values.iter().any(|v| v.ends_with(".png")));
    assert!(!values.iter().any(|v| v.ends_with(".woff2")));
    assert!(values.iter().any(|v| v.contains("/v1/upload")));
}
