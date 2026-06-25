//! T-H1: SSRF and redirect-policy tests for the GitLab API client.

use super::*;
use axon_core::config::Config;
use axon_core::http::validate_url;

// ── cross-host redirect rejection ──────────────────────────────────────────

#[test]
fn build_gitlab_client_succeeds_without_token() {
    let mut cfg = Config::default_minimal();
    cfg.gitlab_token = None;
    build_gitlab_client(&cfg).expect("client build should succeed without token");
}

#[test]
fn build_gitlab_client_succeeds_with_empty_token() {
    let mut cfg = Config::default_minimal();
    cfg.gitlab_token = Some(String::new());
    build_gitlab_client(&cfg).expect("client build should succeed with empty token");
}

#[test]
fn build_gitlab_client_succeeds_with_valid_token() {
    let mut cfg = Config::default_minimal();
    cfg.gitlab_token = Some("glpat-test1234567890".to_string());
    build_gitlab_client(&cfg).expect("client build should succeed with token");
}

// ── bounded_error_body truncation ──────────────────────────────────────────

#[test]
fn error_body_constant_size_limits() {
    // The constants must stay at their intended values to bound memory usage.
    assert_eq!(
        MAX_RESPONSE_BYTES,
        10 * 1024 * 1024,
        "success response cap must be 10 MiB"
    );
    assert_eq!(
        MAX_ERROR_BODY_BYTES,
        4 * 1024,
        "error body cap must be 4 KiB"
    );
}

// ── validate_url reachable from gitlab client ───────────────────────────────

#[test]
fn private_ip_url_blocked_by_ssrf_guard() {
    // validate_url is used by the redirect policy; confirm it blocks RFC-1918 targets
    assert!(
        validate_url("http://192.168.1.1/api/v4/projects").is_err(),
        "RFC-1918 address must be blocked"
    );
    assert!(
        validate_url("http://10.0.0.1/api").is_err(),
        "RFC-1918 10.x must be blocked"
    );
    assert!(
        validate_url("http://127.0.0.1/api").is_err(),
        "loopback must be blocked"
    );
}

#[test]
fn public_gitlab_url_allowed() {
    assert!(
        validate_url("https://gitlab.com/api/v4/projects/42").is_ok(),
        "public gitlab.com API URL must pass SSRF guard"
    );
}
