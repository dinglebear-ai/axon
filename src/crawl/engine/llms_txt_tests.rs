use super::*;
use crate::core::http::LoopbackGuard;

const FIXTURE: &str = "\u{feff}# Example Docs\n\n> A short summary.\n\nSome intro prose with an inline [ignored-in-prose-too](https://example.com/intro.md) link.\n\n## Docs\n\n- [Getting Started](/docs/start.md#h): the basics\n- [Guide](guide.md)\n- [External](https://other.com/x.md)\n- [Email](mailto:hi@example.com)\n- [Anchor](#section)\n\n## Optional\n\n- [Extra](/docs/extra.md)\n";

#[test]
fn extracts_and_resolves_links() {
    let links = extract_llms_txt_links(FIXTURE, "https://example.com/llms.txt");
    // Relative resolved against base; mailto/anchor dropped; external kept (scope happens later).
    // `/docs/start.md#h` proves `set_fragment(None)` strips a fragment from a real link
    // (distinct from the leading-`#` anchor guard).
    assert!(links.contains(&"https://example.com/docs/start.md".to_string()));
    assert!(
        !links.iter().any(|u| u.contains("start.md#h")),
        "fragment must be stripped from /docs/start.md#h via set_fragment(None)"
    );
    assert!(links.contains(&"https://example.com/guide.md".to_string()));
    assert!(links.contains(&"https://other.com/x.md".to_string()));
    assert!(links.contains(&"https://example.com/docs/extra.md".to_string()));
    assert!(!links.iter().any(|u| u.starts_with("mailto:")));
    assert!(!links.iter().any(|u| u.contains("#section")));
    assert!(!links.iter().any(|u| u.contains('#')));
}

#[test]
fn llms_url_brackets_ipv6_authority_with_port() {
    // IPv6 host: host_str() returns the address WITHOUT brackets, so a naive
    // format!("{host}:{port}") produces an invalid authority. join_origin_path must
    // bracket the literal and preserve the port.
    let parsed = Url::parse("https://[::1]:8080/docs").unwrap();
    assert_eq!(
        join_origin_path(&parsed, "/llms.txt").unwrap(),
        "https://[::1]:8080/llms.txt"
    );

    // Sanity: ordinary host with a non-standard port still round-trips.
    let parsed = Url::parse("http://example.com:9000/docs/guide").unwrap();
    assert_eq!(
        join_origin_path(&parsed, "/llms.txt").unwrap(),
        "http://example.com:9000/llms.txt"
    );

    // Userinfo (user:pass@) must be stripped so credentials never reach discovery
    // requests or logs.
    let parsed = Url::parse("https://user:secret@example.com/docs").unwrap();
    assert_eq!(
        join_origin_path(&parsed, "/llms.txt").unwrap(),
        "https://example.com/llms.txt"
    );
}

#[test]
fn rejects_soft_404_html() {
    // text without a leading '# ' H1 is not a valid llms.txt
    assert!(!looks_like_llms_txt(
        "<!DOCTYPE html><html>not found</html>"
    ));
    assert!(looks_like_llms_txt("# Title\n\n> x"));
    // BOM-prefixed still recognized
    assert!(looks_like_llms_txt("\u{feff}# Title"));
}

fn cfg_for(host_include_subdomains: bool, max: usize) -> Config {
    Config {
        include_subdomains: host_include_subdomains,
        max_llms_txt_urls: max,
        ..Config::default()
    }
}

#[test]
fn scope_drops_offhost_and_caps() {
    let cfg = cfg_for(false, 1);
    // Two same-host links + one off-host; cap=1 keeps only one same-host after sort.
    let body = "# T\n\n## S\n- [a](/a.md)\n- [b](/b.md)\n- [ext](https://other.com/c.md)\n";
    // discover_llms_txt_urls needs network; test the pure pieces instead:
    let links = extract_llms_txt_links(body, "https://example.com/llms.txt");
    let scoped: Vec<String> = links
        .into_iter()
        .filter_map(|l| loc_in_scope(&cfg, &l, "example.com", "", true))
        .collect();
    assert!(scoped.iter().all(|u| u.contains("example.com")));
    assert_eq!(scoped.len(), 2, "off-host dropped, two same-host kept");
}

/// `discover_llms_txt_urls` must cap its output at `max_llms_txt_urls` (the per-source
/// fan-out bound) even when the `/llms.txt` lists far more same-host links. This exercises
/// the truncate at llms_txt.rs which is otherwise unexercised end-to-end.
#[tokio::test]
#[serial_test::serial]
async fn discover_llms_txt_urls_caps_at_max() {
    let server = httpmock::MockServer::start();
    let base = server.base_url();
    // 20 same-host links; cap will be 5.
    let mut body = String::from("# Docs\n\n> summary\n\n## Pages\n\n");
    for i in 0..20 {
        body.push_str(&format!("- [p{i}]({base}/page-{i}.md)\n"));
    }
    let m = server.mock(|when, then| {
        when.method(httpmock::Method::GET).path("/llms.txt");
        then.status(200).body(&body);
    });
    let cfg = Config {
        max_llms_txt_urls: 5,
        fetch_retries: 0,
        retry_backoff_ms: 0,
        request_timeout_ms: Some(5_000),
        ..Config::default()
    };
    let _loopback = LoopbackGuard::allow();
    let urls = discover_llms_txt_urls(&cfg, &base).await.expect("discover");
    m.assert();
    assert_eq!(
        urls.len(),
        5,
        "output must be capped at max_llms_txt_urls=5"
    );
}
