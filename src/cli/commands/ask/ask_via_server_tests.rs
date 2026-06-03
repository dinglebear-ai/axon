//! Unit tests for `ask_via_server` HTTP path and the cleartext-token guard.
//!
//! Network calls are isolated via `httpmock` (binds 127.0.0.1, which our
//! loopback detection treats as safe). Tests that mutate environment
//! variables (`AXON_MCP_HTTP_TOKEN`, `AXON_SERVER_INSECURE`) use `serial_test`
//! to avoid cross-test races and snapshot+restore the previous value
//! inside each test.
//!
//! `unsafe_code` is denied workspace-wide; the env mutation in `EnvGuard`
//! requires `set_var` / `remove_var` which became `unsafe` in Rust 2024.
//! This is the only place in the file that touches process-global env
//! state, and it's gated behind `#[serial]` to prevent races.
#![allow(unsafe_code)]

use super::{ask_via_server, hint_for_ask_error};
use crate::cli::client::check_cleartext_token_allowed;
use crate::core::config::Config;
use crate::core::http::LoopbackGuard;
use httpmock::prelude::*;
use serde_json::json;
use serial_test::serial;
use std::net::TcpListener;

const TOKEN_ENV: &str = "AXON_MCP_HTTP_TOKEN";
const INSECURE_ENV: &str = "AXON_SERVER_INSECURE";

/// Snapshots an env var on construction and restores it on drop.
/// The value `None` means "unset on restore".
struct EnvGuard {
    key: &'static str,
    prev: Option<String>,
}

impl EnvGuard {
    fn set(key: &'static str, value: Option<&str>) -> Self {
        let prev = std::env::var(key).ok();
        match value {
            Some(v) => unsafe { std::env::set_var(key, v) },
            None => unsafe { std::env::remove_var(key) },
        }
        Self { key, prev }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match self.prev.take() {
            Some(v) => unsafe { std::env::set_var(self.key, v) },
            None => unsafe { std::env::remove_var(self.key) },
        }
    }
}

fn test_config() -> Config {
    Config {
        collection: "test_col".into(),
        ask_diagnostics: false,
        hybrid_search_enabled: true,
        ..Config::default()
    }
}

fn valid_ask_result_json() -> serde_json::Value {
    json!({
        "query": "test-query",
        "answer": "hello",
        "diagnostics": null,
        "timing_ms": {
            "retrieval": 1,
            "context_build": 2,
            "llm": 3,
            "total": 6
        }
    })
}

// ---------------------------------------------------------------------------
// hint_for_ask_error — pure-function coverage of the conditional-hint mapper
// ---------------------------------------------------------------------------

#[test]
fn hint_for_ask_error_routes_each_class_correctly() {
    assert!(
        hint_for_ask_error("connect to http://x/v1/ask: dns")
            .unwrap()
            .contains("axon serve")
    );
    assert!(
        hint_for_ask_error("server returned 401 Unauthorized: ...")
            .unwrap()
            .contains("AXON_MCP_HTTP_TOKEN")
    );
    assert!(
        hint_for_ask_error("server returned 403 Forbidden: ...")
            .unwrap()
            .contains("AXON_MCP_HTTP_TOKEN")
    );
    // Other 4xx — no hint, likely client error.
    assert!(hint_for_ask_error("server returned 404 Not Found: x").is_none());
    assert!(hint_for_ask_error("server returned 422 Unprocessable: x").is_none());
    assert!(
        hint_for_ask_error("decode AskResult from http://x/v1/ask: bad")
            .unwrap()
            .contains("schema")
    );
    assert!(
        hint_for_ask_error("refusing to send AXON_MCP_HTTP_TOKEN over plaintext HTTP ...")
            .unwrap()
            .contains("AXON_SERVER_INSECURE=1")
    );
    assert!(hint_for_ask_error("server returned 500 ...").is_none());
    assert!(hint_for_ask_error("totally unrelated").is_none());
}

// ---------------------------------------------------------------------------
// check_cleartext_token_allowed — gate behavior in isolation (no network)
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn cleartext_gate_allows_https_anywhere() {
    let _g = EnvGuard::set(INSECURE_ENV, None);
    let url = reqwest::Url::parse("https://example.com:8001/").unwrap();
    assert!(check_cleartext_token_allowed(&url).is_ok());
}

#[test]
#[serial]
fn cleartext_gate_allows_http_loopback_ipv4() {
    let _g = EnvGuard::set(INSECURE_ENV, None);
    let url = reqwest::Url::parse("http://127.0.0.1:8001/").unwrap();
    assert!(check_cleartext_token_allowed(&url).is_ok());
}

#[test]
#[serial]
fn cleartext_gate_allows_http_loopback_ipv6() {
    let _g = EnvGuard::set(INSECURE_ENV, None);
    let url = reqwest::Url::parse("http://[::1]:8001/").unwrap();
    assert!(check_cleartext_token_allowed(&url).is_ok());
}

#[test]
#[serial]
fn cleartext_gate_allows_http_localhost() {
    let _g = EnvGuard::set(INSECURE_ENV, None);
    let url = reqwest::Url::parse("http://localhost:8001/").unwrap();
    assert!(check_cleartext_token_allowed(&url).is_ok());
}

#[test]
#[serial]
fn cleartext_gate_refuses_http_non_loopback() {
    let _g = EnvGuard::set(INSECURE_ENV, None);
    let url = reqwest::Url::parse("http://example.com:8001/").unwrap();
    let err = check_cleartext_token_allowed(&url).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("refusing to send AXON_MCP_HTTP_TOKEN"));
    assert!(msg.contains("example.com"));
    assert!(msg.contains("AXON_SERVER_INSECURE=1"));
}

#[test]
#[serial]
fn cleartext_gate_opt_in_via_env() {
    let _g = EnvGuard::set(INSECURE_ENV, Some("1"));
    let url = reqwest::Url::parse("http://example.com:8001/").unwrap();
    assert!(check_cleartext_token_allowed(&url).is_ok());
}

// ---------------------------------------------------------------------------
// ask_via_server — happy path / status-code branches / payload shape
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn ask_via_server_returns_parsed_result_on_200() {
    let _lo = LoopbackGuard::allow();
    let _t = EnvGuard::set(TOKEN_ENV, None);
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(POST).path("/v1/ask");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(valid_ask_result_json());
    });

    let cfg = test_config();
    let url = reqwest::Url::parse(&server.base_url()).unwrap();
    let result = ask_via_server(&cfg, &url, "what is rust?").await.unwrap();

    mock.assert();
    assert_eq!(result.answer, "hello");
    assert_eq!(result.timing_ms.total, 6);
}

#[tokio::test]
#[serial]
async fn ask_via_server_forwards_ask_overrides() {
    let _lo = LoopbackGuard::allow();
    let _t = EnvGuard::set(TOKEN_ENV, None);
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(POST).path("/v1/ask").json_body(json!({
            "query": "what is rust?",
            "collection": "test_col",
            "diagnostics": false,
            "explain": false,
            "hybrid_search": true,
            "ask_chunk_limit": 7,
            "ask_full_docs": 2,
            "ask_max_context_chars": 5000,
            "ask_hybrid_candidates": 42,
            "ask_min_relevance_score": 0.7,
            "ask_doc_chunk_limit": 64,
            "ask_doc_fetch_concurrency": 3,
            "ask_backfill_chunks": 4,
            "ask_candidate_limit": 55,
            "ask_min_citations_nontrivial": 3,
            "ask_authoritative_domains": ["docs.rs"],
            "ask_authoritative_boost": 0.2
        }));
        then.status(200)
            .header("content-type", "application/json")
            .json_body(valid_ask_result_json());
    });

    let mut cfg = test_config();
    cfg.ask_chunk_limit = 7;
    cfg.ask_full_docs = 2;
    cfg.ask_max_context_chars = 5000;
    cfg.ask_hybrid_candidates = 42;
    cfg.ask_min_relevance_score = 0.7;
    cfg.ask_doc_chunk_limit = 64;
    cfg.ask_doc_fetch_concurrency = 3;
    cfg.ask_backfill_chunks = 4;
    cfg.ask_candidate_limit = 55;
    cfg.ask_min_citations_nontrivial = 3;
    cfg.ask_authoritative_domains = vec!["docs.rs".to_string()];
    cfg.ask_authoritative_boost = 0.2;

    let url = reqwest::Url::parse(&server.base_url()).unwrap();
    let result = ask_via_server(&cfg, &url, "what is rust?").await.unwrap();

    mock.assert();
    assert_eq!(result.answer, "hello");
}

#[tokio::test]
#[serial]
async fn ask_via_server_forwards_explain_and_implies_diagnostics() {
    let _lo = LoopbackGuard::allow();
    let _t = EnvGuard::set(TOKEN_ENV, None);
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(POST).path("/v1/ask").is_true(|req| {
            let body: serde_json::Value = serde_json::from_slice(req.body().as_ref()).unwrap();
            body["explain"] == true && body["diagnostics"] == true
        });
        then.status(200)
            .header("content-type", "application/json")
            .json_body(valid_ask_result_json());
    });

    let mut cfg = test_config();
    cfg.ask_explain = true;

    let url = reqwest::Url::parse(&server.base_url()).unwrap();
    let result = ask_via_server(&cfg, &url, "what is rust?").await.unwrap();

    mock.assert();
    assert_eq!(result.answer, "hello");
}

#[tokio::test]
#[serial]
async fn ask_via_server_401_yields_token_mismatch_hint() {
    let _lo = LoopbackGuard::allow();
    let _t = EnvGuard::set(TOKEN_ENV, None);
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(POST).path("/v1/ask");
        then.status(401).body("unauthorized");
    });

    let cfg = test_config();
    let url = reqwest::Url::parse(&server.base_url()).unwrap();
    let err = ask_via_server(&cfg, &url, "x").await.unwrap_err();
    let msg = err.to_string();
    assert!(msg.starts_with("server returned 401"), "got: {msg}");
    let hint = hint_for_ask_error(&msg).expect("401 should have a hint");
    assert!(hint.contains("AXON_MCP_HTTP_TOKEN"));
}

#[tokio::test]
#[serial]
async fn ask_via_server_500_includes_body_in_error() {
    let _lo = LoopbackGuard::allow();
    let _t = EnvGuard::set(TOKEN_ENV, None);
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(POST).path("/v1/ask");
        then.status(500).body("kaboom-internal");
    });

    let cfg = test_config();
    let url = reqwest::Url::parse(&server.base_url()).unwrap();
    let err = ask_via_server(&cfg, &url, "x").await.unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("server returned 500"));
    assert!(msg.contains("kaboom-internal"));
}

#[tokio::test]
#[serial]
async fn ask_via_server_malformed_json_yields_decode_error() {
    let _lo = LoopbackGuard::allow();
    let _t = EnvGuard::set(TOKEN_ENV, None);
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(POST).path("/v1/ask");
        then.status(200)
            .header("content-type", "application/json")
            .body("{not-json");
    });

    let cfg = test_config();
    let url = reqwest::Url::parse(&server.base_url()).unwrap();
    let err = ask_via_server(&cfg, &url, "x").await.unwrap_err();
    let msg = err.to_string();
    assert!(msg.starts_with("decode AskResult"), "got: {msg}");
    assert!(hint_for_ask_error(&msg).is_some());
}

#[tokio::test]
#[serial]
async fn ask_via_server_connection_refused_uses_connect_prefix() {
    let _lo = LoopbackGuard::allow();
    let _t = EnvGuard::set(TOKEN_ENV, None);
    let addr = {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap()
    };
    let dead_url = format!("http://{addr}");

    let cfg = test_config();
    let url = reqwest::Url::parse(&dead_url).unwrap();
    let err = ask_via_server(&cfg, &url, "x").await.unwrap_err();
    let msg = err.to_string();
    assert!(msg.starts_with("connect to "), "got: {msg}");
    assert!(msg.contains(&dead_url) || msg.contains("/v1/ask"));
    assert!(hint_for_ask_error(&msg).unwrap().contains("axon serve"));
}

// ---------------------------------------------------------------------------
// ask_via_server — token attachment + cleartext-bearer guard
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn ask_via_server_refuses_token_over_http_to_non_loopback() {
    let _t = EnvGuard::set(TOKEN_ENV, Some("secret-abc"));
    let _i = EnvGuard::set(INSECURE_ENV, None);

    let cfg = test_config();
    // example.com:1 — request must never leave the gate, so port is irrelevant.
    let url = reqwest::Url::parse("http://example.com:1/").unwrap();
    let err = ask_via_server(&cfg, &url, "x").await.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("refusing to send AXON_MCP_HTTP_TOKEN"),
        "got: {msg}"
    );
    assert!(msg.contains("example.com"));
    assert!(
        hint_for_ask_error(&msg)
            .unwrap()
            .contains("AXON_SERVER_INSECURE=1")
    );
}

#[tokio::test]
#[serial]
async fn ask_via_server_attaches_bearer_on_loopback_http() {
    let _lo = LoopbackGuard::allow();
    let _t = EnvGuard::set(TOKEN_ENV, Some("loopback-token"));
    let _i = EnvGuard::set(INSECURE_ENV, None);

    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/ask")
            .header("authorization", "Bearer loopback-token");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(valid_ask_result_json());
    });

    let cfg = test_config();
    let url = reqwest::Url::parse(&server.base_url()).unwrap();
    let res = ask_via_server(&cfg, &url, "x").await.unwrap();
    mock.assert();
    assert_eq!(res.answer, "hello");
}

#[tokio::test]
#[serial]
async fn ask_via_server_omits_bearer_when_token_is_whitespace() {
    let _lo = LoopbackGuard::allow();
    let _t = EnvGuard::set(TOKEN_ENV, Some("   "));
    let _i = EnvGuard::set(INSECURE_ENV, None);

    let server = MockServer::start();
    // No Authorization header expected — match a request with absent header.
    let mock = server.mock(|when, then| {
        when.method(POST).path("/v1/ask").is_true(|req| {
            !req.headers_vec()
                .iter()
                .any(|(k, _)| k.eq_ignore_ascii_case("authorization"))
        });
        then.status(200)
            .header("content-type", "application/json")
            .json_body(valid_ask_result_json());
    });

    let cfg = test_config();
    let url = reqwest::Url::parse(&server.base_url()).unwrap();
    let _ = ask_via_server(&cfg, &url, "x").await.unwrap();
    mock.assert();
}

// ---------------------------------------------------------------------------
// ask_via_server — payload shape (since/before, optional fields)
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn payload_omits_since_before_when_none() {
    let _lo = LoopbackGuard::allow();
    let _t = EnvGuard::set(TOKEN_ENV, None);
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(POST).path("/v1/ask").is_true(|req| {
            let body = req.body_string();
            let v: serde_json::Value =
                serde_json::from_str(&body).unwrap_or(serde_json::Value::Null);
            let obj = match v.as_object() {
                Some(o) => o,
                None => return false,
            };
            !obj.contains_key("since") && !obj.contains_key("before")
        });
        then.status(200)
            .header("content-type", "application/json")
            .json_body(valid_ask_result_json());
    });

    let mut cfg = test_config();
    cfg.since = None;
    cfg.before = None;
    let url = reqwest::Url::parse(&server.base_url()).unwrap();
    let _ = ask_via_server(&cfg, &url, "q").await.unwrap();
    mock.assert();
}

#[tokio::test]
#[serial]
async fn payload_includes_since_before_when_set() {
    let _lo = LoopbackGuard::allow();
    let _t = EnvGuard::set(TOKEN_ENV, None);
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(POST).path("/v1/ask").is_true(|req| {
            let body = req.body_string();
            let v: serde_json::Value =
                serde_json::from_str(&body).unwrap_or(serde_json::Value::Null);
            v.get("since").and_then(|x| x.as_str()) == Some("7d")
                && v.get("before").and_then(|x| x.as_str()) == Some("2026-01-01")
        });
        then.status(200)
            .header("content-type", "application/json")
            .json_body(valid_ask_result_json());
    });

    let mut cfg = test_config();
    cfg.since = Some("7d".into());
    cfg.before = Some("2026-01-01".into());
    let url = reqwest::Url::parse(&server.base_url()).unwrap();
    let _ = ask_via_server(&cfg, &url, "q").await.unwrap();
    mock.assert();
}

#[tokio::test]
#[serial]
async fn endpoint_normalizes_trailing_slash_variants() {
    let _lo = LoopbackGuard::allow();
    let _t = EnvGuard::set(TOKEN_ENV, None);
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(POST).path("/v1/ask");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(valid_ask_result_json());
    });

    let cfg = test_config();
    // Variant A: no trailing slash on base.
    let base_a = server.base_url();
    let url_a = reqwest::Url::parse(&base_a).unwrap();
    ask_via_server(&cfg, &url_a, "x").await.unwrap();

    // Variant B: with trailing slash on base.
    let base_b = format!("{base_a}/");
    let url_b = reqwest::Url::parse(&base_b).unwrap();
    ask_via_server(&cfg, &url_b, "x").await.unwrap();
}
