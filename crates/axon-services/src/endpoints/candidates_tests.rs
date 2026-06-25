use super::*;

#[test]
fn apex_simple_com() {
    assert_eq!(registrable_apex("foo.com").as_deref(), Some("foo.com"));
    assert_eq!(registrable_apex("docs.foo.com").as_deref(), Some("foo.com"));
}

#[test]
fn apex_multi_part_tld() {
    assert_eq!(
        registrable_apex("docs.foo.co.uk").as_deref(),
        Some("foo.co.uk")
    );
    assert_eq!(
        registrable_apex("a.b.foo.com.au").as_deref(),
        Some("foo.com.au")
    );
}

#[test]
fn apex_rejects_ip_and_unknown() {
    assert_eq!(registrable_apex("127.0.0.1"), None);
    assert_eq!(registrable_apex("[::1]"), None);
    assert_eq!(registrable_apex("localhost"), None);
}

#[test]
fn candidates_same_host_only() {
    let c = mcp_candidate_urls("https://docs.foo.com/bar", false);
    let urls: Vec<&str> = c.iter().map(|x| x.url.as_str()).collect();
    assert_eq!(
        urls,
        vec!["https://docs.foo.com/mcp", "https://docs.foo.com/api/mcp"]
    );
    assert!(c.iter().all(|x| x.host_kind == McpHostKind::SameHost));
}

#[test]
fn candidates_with_subdomain() {
    let c = mcp_candidate_urls("https://docs.foo.com/bar", true);
    let urls: Vec<&str> = c.iter().map(|x| x.url.as_str()).collect();
    assert_eq!(
        urls,
        vec![
            "https://docs.foo.com/mcp",
            "https://docs.foo.com/api/mcp",
            "https://mcp.foo.com/mcp",
            "https://mcp.foo.com/api/mcp",
        ]
    );
}

#[test]
fn candidates_collapse_when_host_is_mcp() {
    // Target host already mcp.* → subdomain set duplicates same-host, so skipped.
    let c = mcp_candidate_urls("https://mcp.foo.com/x", true);
    let urls: Vec<&str> = c.iter().map(|x| x.url.as_str()).collect();
    assert_eq!(
        urls,
        vec!["https://mcp.foo.com/mcp", "https://mcp.foo.com/api/mcp"]
    );
}

#[test]
fn candidates_skip_subdomain_for_ip() {
    let c = mcp_candidate_urls("http://127.0.0.1:9000/x", true);
    // same-host candidates still produced (will be SSRF-blocked later), no subdomain
    assert!(c.iter().all(|x| x.host_kind == McpHostKind::SameHost));
    assert_eq!(c.len(), 2);
}

use crate::types::{EndpointReport, EndpointSourceKind, McpProbeOutcome};
use axon_core::http::LoopbackGuard;
use httpmock::prelude::*;
use serial_test::serial;

fn empty_report(url: &str) -> EndpointReport {
    EndpointReport {
        url: url.to_string(),
        endpoints: Vec::new(),
        hosts: Vec::new(),
        scripts_discovered: 0,
        bundles_fetched: 0,
        bundles_scanned: 0,
        truncated: false,
        warnings: Vec::new(),
        elapsed_ms: 0,
        mcp_candidates: Vec::new(),
    }
}

#[tokio::test]
#[serial]
async fn synthesized_same_host_mcp_confirms() {
    let _loopback = LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(POST).path("/mcp").body_includes("initialize");
            then.status(200).header("content-type", "application/json").json_body(serde_json::json!({
                "jsonrpc": "2.0", "id": 1,
                "result": { "serverInfo": { "name": "demo", "version": "1" }, "capabilities": {} }
            }));
        })
        .await;
    let client = axon_core::http::build_client(3, Some(axon_core::http::axon_ua())).unwrap();
    let mut report = empty_report(&server.url("/x"));

    synthesize_and_probe_mcp(&client, &server.url("/x"), false, &mut report).await;

    // /mcp confirmed, /api/mcp unconfirmed (404).
    assert_eq!(report.mcp_candidates.len(), 2);
    let confirmed: Vec<_> = report
        .mcp_candidates
        .iter()
        .filter(|a| a.outcome == McpProbeOutcome::Confirmed)
        .collect();
    assert_eq!(confirmed.len(), 1);
    assert!(confirmed[0].url.ends_with("/mcp"));
    // Confirmed candidate added to endpoints as synthesized_mcp.
    assert!(
        report
            .endpoints
            .iter()
            .any(|e| e.source == EndpointSourceKind::SynthesizedMcp && e.first_party)
    );
}

#[tokio::test]
#[serial]
async fn synthesized_candidate_blocked_when_loopback_disallowed() {
    // No LoopbackGuard → SSRF guard blocks 127.0.0.1.
    let _loopback = LoopbackGuard::block();
    let client = axon_core::http::build_client(3, Some(axon_core::http::axon_ua())).unwrap();
    let mut report = empty_report("http://127.0.0.1:9/x");

    synthesize_and_probe_mcp(&client, "http://127.0.0.1:9/x", false, &mut report).await;

    assert!(!report.mcp_candidates.is_empty());
    assert!(
        report
            .mcp_candidates
            .iter()
            .all(|a| a.outcome == McpProbeOutcome::Blocked)
    );
    assert!(report.endpoints.is_empty());
}

#[tokio::test]
#[serial]
async fn synthesized_candidate_dedups_against_existing_endpoint() {
    let _loopback = LoopbackGuard::allow();
    let server = MockServer::start_async().await;
    // /api/mcp is a real MCP server; /mcp is pre-seeded as an existing endpoint
    // and must therefore be filtered out (never re-probed).
    server
        .mock_async(|when, then| {
            when.method(POST).path("/api/mcp").body_includes("initialize");
            then.status(200).header("content-type", "application/json").json_body(serde_json::json!({
                "jsonrpc": "2.0", "id": 1,
                "result": { "serverInfo": { "name": "demo", "version": "1" }, "capabilities": {} }
            }));
        })
        .await;
    let client = axon_core::http::build_client(3, Some(axon_core::http::axon_ua())).unwrap();
    let mut report = empty_report(&server.url("/x"));
    report.endpoints.push(DiscoveredEndpoint {
        value: server.url("/mcp"),
        normalized_url: Some(server.url("/mcp")),
        kind: EndpointKind::AbsoluteUrl,
        first_party: true,
        source: EndpointSourceKind::HtmlAttribute,
        source_url: Some(server.url("/x")),
        verified: None,
        rpc_probe: None,
    });

    synthesize_and_probe_mcp(&client, &server.url("/x"), false, &mut report).await;

    // /mcp deduped → only /api/mcp probed.
    assert_eq!(report.mcp_candidates.len(), 1);
    assert!(report.mcp_candidates[0].url.ends_with("/api/mcp"));
}

#[tokio::test]
#[serial]
async fn synthesized_subdomain_attempt_is_recorded() {
    // Real apex (NOT loopback) so example.com resolves normally. The
    // mcp.example.com candidate will be Blocked or Unconfirmed — we only assert
    // the ApexSubdomain attempt was enumerated and recorded, not confirmed.
    let client = axon_core::http::build_client(3, Some(axon_core::http::axon_ua())).unwrap();
    let mut report = empty_report("https://docs.example.com/x");

    synthesize_and_probe_mcp(&client, "https://docs.example.com/x", true, &mut report).await;

    assert!(
        report
            .mcp_candidates
            .iter()
            .any(|a| a.host_kind == McpHostKind::ApexSubdomain),
        "expected an ApexSubdomain attempt; got {:?}",
        report.mcp_candidates
    );
}
