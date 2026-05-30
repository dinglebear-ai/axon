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
