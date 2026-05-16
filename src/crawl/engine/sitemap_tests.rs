use super::*;
use crate::core::config::Config;

/// Unit test for `sitemap_loc_in_scope` using real domain names.
/// The integration test uses a loopback mock server (IP address) where
/// IP addresses have no subdomain relationship — this test exercises
/// the actual subdomain branching logic directly with real hostnames.
#[test]
fn sitemap_loc_in_scope_subdomain_branching() {
    let cfg_no_sub = Config {
        include_subdomains: false,
        ..Config::default()
    };
    let cfg_with_sub = Config {
        include_subdomains: true,
        ..Config::default()
    };

    // Same host: included regardless of include_subdomains setting.
    assert!(
        sitemap_loc_in_scope(
            &cfg_no_sub,
            "https://docs.example.com/page",
            "docs.example.com",
            "/",
            true
        )
        .is_some(),
        "same host should always be in scope"
    );

    // Subdomain with include_subdomains=false: excluded.
    assert!(
        sitemap_loc_in_scope(
            &cfg_no_sub,
            "https://api.example.com/page",
            "example.com",
            "/",
            true
        )
        .is_none(),
        "subdomain should be excluded when include_subdomains=false"
    );

    // Subdomain with include_subdomains=true: included.
    assert!(
        sitemap_loc_in_scope(
            &cfg_with_sub,
            "https://api.example.com/page",
            "example.com",
            "/",
            true
        )
        .is_some(),
        "subdomain should be included when include_subdomains=true"
    );

    // Completely different domain: excluded with both settings.
    assert!(
        sitemap_loc_in_scope(
            &cfg_with_sub,
            "https://other.com/page",
            "example.com",
            "/",
            true
        )
        .is_none(),
        "unrelated domain should never be in scope"
    );
}

#[test]
fn request_timeout_secs_rounds_up_with_minimum_one_second() {
    let cfg = Config {
        request_timeout_ms: Some(1),
        ..Config::default()
    };
    assert_eq!(request_timeout_secs(&cfg), 1);

    let cfg = Config {
        request_timeout_ms: Some(999),
        ..Config::default()
    };
    assert_eq!(request_timeout_secs(&cfg), 1);

    let cfg = Config {
        request_timeout_ms: Some(1_001),
        ..Config::default()
    };
    assert_eq!(request_timeout_secs(&cfg), 2);
}
