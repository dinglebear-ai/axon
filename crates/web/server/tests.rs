//! Tests for `crates/web/server.rs` auth + ask classification helpers.
//!
//! Env-var reads are not isolated, so every test that touches
//! `AXON_MCP_HTTP_TOKEN` is marked `#[serial]`.

#![allow(unsafe_code)]

use super::{ask_authorized, classify_ask_error};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use serial_test::serial;
#[cfg(unix)]
use std::os::unix::ffi::OsStringExt;

const ENV_KEY: &str = "AXON_MCP_HTTP_TOKEN";

struct EnvGuard {
    prev: Option<String>,
}

impl EnvGuard {
    fn set(value: Option<&str>) -> Self {
        let prev = std::env::var(ENV_KEY).ok();
        match value {
            Some(v) => unsafe { std::env::set_var(ENV_KEY, v) },
            None => unsafe { std::env::remove_var(ENV_KEY) },
        }
        Self { prev }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match self.prev.take() {
            Some(v) => unsafe { std::env::set_var(ENV_KEY, v) },
            None => unsafe { std::env::remove_var(ENV_KEY) },
        }
    }
}

#[cfg(unix)]
struct OsEnvGuard {
    prev: Option<std::ffi::OsString>,
}

#[cfg(unix)]
impl OsEnvGuard {
    fn set_non_utf8(bytes: Vec<u8>) -> Self {
        let prev = std::env::var_os(ENV_KEY);
        unsafe {
            std::env::set_var(ENV_KEY, std::ffi::OsString::from_vec(bytes));
        }
        Self { prev }
    }
}

#[cfg(unix)]
impl Drop for OsEnvGuard {
    fn drop(&mut self) {
        unsafe {
            match self.prev.take() {
                Some(v) => std::env::set_var(ENV_KEY, v),
                None => std::env::remove_var(ENV_KEY),
            }
        }
    }
}

fn h(pairs: &[(&'static str, &'static str)]) -> HeaderMap {
    let mut m = HeaderMap::new();
    for (k, v) in pairs {
        m.insert(
            HeaderName::from_static(k),
            HeaderValue::from_str(v).expect("test header value"),
        );
    }
    m
}

// ---- ask_authorized ----

#[test]
#[serial]
fn ask_authorized_unset_no_headers_allows() {
    let _guard = EnvGuard::set(None);
    assert!(ask_authorized(&HeaderMap::new()));
}

#[test]
#[serial]
fn ask_authorized_unset_with_headers_still_allows() {
    let _guard = EnvGuard::set(None);
    let headers = h(&[("authorization", "Bearer whatever")]);
    assert!(ask_authorized(&headers));
}

#[test]
#[serial]
fn ask_authorized_set_no_headers_denies() {
    let _guard = EnvGuard::set(Some("secret"));
    assert!(!ask_authorized(&HeaderMap::new()));
}

#[test]
#[serial]
fn ask_authorized_set_correct_bearer_allows() {
    let _guard = EnvGuard::set(Some("secret"));
    let headers = h(&[("authorization", "Bearer secret")]);
    assert!(ask_authorized(&headers));
}

#[test]
#[serial]
fn ask_authorized_set_correct_bearer_case_insensitive_allows() {
    let _guard = EnvGuard::set(Some("secret"));
    let headers = h(&[("authorization", "bearer secret")]);
    assert!(ask_authorized(&headers));
}

#[test]
#[serial]
fn ask_authorized_set_correct_api_key_allows() {
    let _guard = EnvGuard::set(Some("secret"));
    let headers = h(&[("x-api-key", "secret")]);
    assert!(ask_authorized(&headers));
}

#[test]
#[serial]
fn ask_authorized_wrong_bearer_correct_api_key_allows() {
    // Either header alone may carry the correct token.
    let _guard = EnvGuard::set(Some("secret"));
    let headers = h(&[("authorization", "Bearer wrong"), ("x-api-key", "secret")]);
    assert!(ask_authorized(&headers));
}

#[test]
#[serial]
fn ask_authorized_wrong_bearer_alone_denies() {
    let _guard = EnvGuard::set(Some("secret"));
    let headers = h(&[("authorization", "Bearer wrong")]);
    assert!(!ask_authorized(&headers));
}

#[test]
#[serial]
fn ask_authorized_malformed_bearer_denies() {
    let _guard = EnvGuard::set(Some("secret"));
    // Missing space between scheme and token: "Bearersecret"
    let headers = h(&[("authorization", "Bearersecret")]);
    assert!(!ask_authorized(&headers));
}

#[test]
#[serial]
fn ask_authorized_set_to_whitespace_fails_closed() {
    // Operator clearly intended to enable auth — the env is set — but the
    // value is whitespace. Refuse all requests rather than fail open.
    let _guard = EnvGuard::set(Some("   "));
    assert!(!ask_authorized(&HeaderMap::new()));
    let headers = h(&[("authorization", "Bearer "), ("x-api-key", "")]);
    assert!(!ask_authorized(&headers));
}

#[test]
#[serial]
fn ask_authorized_set_to_empty_string_fails_closed() {
    let _guard = EnvGuard::set(Some(""));
    assert!(!ask_authorized(&HeaderMap::new()));
}

#[test]
#[serial]
#[cfg(unix)]
fn ask_authorized_non_utf8_token_fails_closed() {
    let _guard = OsEnvGuard::set_non_utf8(vec![0xff, b'x']);
    assert!(!ask_authorized(&HeaderMap::new()));
}

// ---- classify_ask_error ----

#[derive(Debug)]
struct Boom(String);
impl std::fmt::Display for Boom {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Boom {}

#[test]
fn classify_bad_request() {
    let e = Boom("invalid query: empty".to_string());
    let (status, kind) = classify_ask_error(&e);
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(kind, "bad_request");
}

#[test]
fn classify_upstream() {
    let e = Boom("qdrant: connection refused".to_string());
    let (status, kind) = classify_ask_error(&e);
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(kind, "upstream");
}

#[test]
fn classify_upstream_timeout() {
    let e = Boom("TEI request timed out".to_string());
    let (status, kind) = classify_ask_error(&e);
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(kind, "upstream");
}

#[test]
fn classify_internal_default() {
    let e = Boom("something went sideways".to_string());
    let (status, kind) = classify_ask_error(&e);
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(kind, "internal");
}
