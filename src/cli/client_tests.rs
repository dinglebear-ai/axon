#![allow(unsafe_code)]

use super::{
    SERVER_POLL_TIMEOUT_SECS, ServerClient, ServerClientErrorKind, check_cleartext_token_allowed,
};
use crate::core::http::LoopbackGuard;
use httpmock::prelude::*;
use serde_json::json;
use serial_test::serial;

const TOKEN_ENV: &str = "AXON_MCP_HTTP_TOKEN";
const INSECURE_ENV: &str = "AXON_SERVER_INSECURE";

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

#[test]
fn exposes_polling_timeout_constant() {
    assert_eq!(SERVER_POLL_TIMEOUT_SECS, 30);
}

#[test]
#[serial]
fn cleartext_guard_refuses_non_loopback_http() {
    let _i = EnvGuard::set(INSECURE_ENV, None);
    let url = reqwest::Url::parse("http://example.com:8001/").unwrap();
    let err = check_cleartext_token_allowed(&url).unwrap_err();
    assert_eq!(err.kind(), ServerClientErrorKind::CleartextBearer);
    assert!(err.to_string().contains("AXON_SERVER_INSECURE=1"));
}

#[test]
#[serial]
fn cleartext_guard_allows_generic_override() {
    let _i = EnvGuard::set(INSECURE_ENV, Some("1"));
    let url = reqwest::Url::parse("http://example.com:8001/").unwrap();
    assert!(check_cleartext_token_allowed(&url).is_ok());
}

#[tokio::test]
#[serial]
async fn post_json_attaches_bearer_token() {
    let _lo = LoopbackGuard::allow();
    let _t = EnvGuard::set(TOKEN_ENV, Some("client-token"));
    let _i = EnvGuard::set(INSECURE_ENV, None);
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/scrape")
            .header("authorization", "Bearer client-token")
            .json_body(json!({"url": "https://example.com"}));
        then.status(200)
            .header("content-type", "application/json")
            .json_body(json!({"ok": true}));
    });

    let client = ServerClient::new(reqwest::Url::parse(&server.base_url()).unwrap()).unwrap();
    let got: serde_json::Value = client
        .post_json(
            "/v1/scrape",
            &json!({"url": "https://example.com"}),
            "scrape response",
        )
        .await
        .unwrap();

    mock.assert();
    assert_eq!(got, json!({"ok": true}));
}

#[tokio::test]
#[serial]
async fn get_json_preserves_query_string() {
    let _lo = LoopbackGuard::allow();
    let _t = EnvGuard::set(TOKEN_ENV, None);
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(GET)
            .path("/v1/sources")
            .query_param("limit", "3")
            .query_param("domain", "example.com");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(json!({"ok": true}));
    });

    let client = ServerClient::new(reqwest::Url::parse(&server.base_url()).unwrap()).unwrap();
    let got: serde_json::Value = client
        .get_json("/v1/sources?limit=3&domain=example.com", "sources response")
        .await
        .unwrap();

    mock.assert();
    assert_eq!(got, json!({"ok": true}));
}

#[tokio::test]
#[serial]
async fn post_json_classifies_auth_status() {
    let _lo = LoopbackGuard::allow();
    let _t = EnvGuard::set(TOKEN_ENV, None);
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(POST).path("/v1/scrape");
        then.status(401).body("unauthorized");
    });

    let client = ServerClient::new(reqwest::Url::parse(&server.base_url()).unwrap()).unwrap();
    let err = client
        .post_json::<_, serde_json::Value>(
            "/v1/scrape",
            &json!({"url": "https://example.com"}),
            "scrape response",
        )
        .await
        .unwrap_err();

    assert_eq!(err.kind(), ServerClientErrorKind::Auth);
    assert!(err.to_string().starts_with("server returned 401"));
}

#[tokio::test]
#[serial]
async fn post_json_classifies_schema_version_mismatch() {
    let _lo = LoopbackGuard::allow();
    let _t = EnvGuard::set(TOKEN_ENV, None);
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(POST).path("/v1/scrape");
        then.status(426).body("schema version mismatch");
    });

    let client = ServerClient::new(reqwest::Url::parse(&server.base_url()).unwrap()).unwrap();
    let err = client
        .post_json::<_, serde_json::Value>(
            "/v1/scrape",
            &json!({"url": "https://example.com"}),
            "scrape response",
        )
        .await
        .unwrap_err();

    assert_eq!(err.kind(), ServerClientErrorKind::VersionMismatch);
}
