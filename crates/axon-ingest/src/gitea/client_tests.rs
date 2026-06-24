//! T-H1: SSRF and redirect-policy tests for the Gitea/Forgejo API client.

use super::*;
use axon_core::config::Config;
use axon_core::http::validate_url;

// ── client construction ─────────────────────────────────────────────────────

#[test]
fn build_client_succeeds_without_token() {
    let mut cfg = Config::default_minimal();
    cfg.gitea_token = None;
    build_client(&cfg).expect("client build should succeed without token");
}

#[test]
fn build_client_succeeds_with_empty_token() {
    let mut cfg = Config::default_minimal();
    cfg.gitea_token = Some(String::new());
    build_client(&cfg).expect("client build should succeed with empty token");
}

#[test]
fn build_client_succeeds_with_valid_token() {
    let mut cfg = Config::default_minimal();
    cfg.gitea_token = Some("mysecrettoken".to_string());
    build_client(&cfg).expect("client build should succeed with token");
}

// ── size constant guard ─────────────────────────────────────────────────────

#[test]
fn size_limit_constants_are_correct() {
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

// ── SSRF guard via validate_url ─────────────────────────────────────────────

#[test]
fn private_ip_blocked() {
    assert!(
        validate_url("http://192.168.0.1/api/v1/repos/o/r").is_err(),
        "RFC-1918 192.168.x must be blocked"
    );
    assert!(
        validate_url("http://172.16.0.1/api/v1").is_err(),
        "RFC-1918 172.16.x must be blocked"
    );
    assert!(
        validate_url("http://169.254.169.254/latest/meta-data/").is_err(),
        "link-local (169.254.x) must be blocked"
    );
}

#[test]
fn public_codeberg_url_allowed() {
    assert!(
        validate_url("https://codeberg.org/api/v1/repos/owner/repo").is_ok(),
        "codeberg.org API URL must pass SSRF guard"
    );
}
