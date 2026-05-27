//! Tests for `crates/web/server.rs` ask classification and ask route contracts.

#![allow(unsafe_code)]

use super::HttpError;
use super::test_support::{EnvGuard, spawn_ask_test_server, spawn_full_test_server, stop};
use crate::mcp::auth::AuthPolicy;
use axum::http::StatusCode;
use serial_test::serial;
use std::error::Error;
use uuid::Uuid;

#[derive(Debug)]
struct Boom(String);
impl std::fmt::Display for Boom {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl Error for Boom {}

#[test]
fn classify_bad_request() {
    let e = Boom("invalid query: empty".to_string());
    let err = HttpError::from_error(&e);
    assert_eq!(err.status(), StatusCode::BAD_REQUEST);
    assert_eq!(err.kind(), "bad_request");
}

#[test]
fn classify_upstream() {
    let e = Boom("qdrant: connection refused".to_string());
    let err = HttpError::from_error(&e);
    assert_eq!(err.status(), StatusCode::BAD_GATEWAY);
    assert_eq!(err.kind(), "upstream_unavailable");
}

#[test]
fn classify_upstream_timeout() {
    let e = Boom("TEI request timed out".to_string());
    let err = HttpError::from_error(&e);
    assert_eq!(err.status(), StatusCode::GATEWAY_TIMEOUT);
    assert_eq!(err.kind(), "timeout");
}

#[test]
fn classify_rate_limit_uses_sanitized_message() {
    let e = Boom("upstream 429: account specific limit details".to_string());
    let err = HttpError::from_error(&e);
    assert_eq!(err.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(err.kind(), "rate_limited");
    assert_eq!(err.message(), "rate limited");
}

#[test]
fn classify_internal_default() {
    let e = Boom("something went sideways".to_string());
    let err = HttpError::from_error(&e);
    assert_eq!(err.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(err.kind(), "internal");
}

#[tokio::test]
#[serial]
async fn v1_ask_auth_layer_rejects_missing_and_wrong_tokens() {
    let _env = EnvGuard::set(Some("secret"));
    let (base, shutdown, handle) =
        spawn_ask_test_server(AuthPolicy::Mounted { auth_state: None }).await;
    let client = reqwest::Client::new();
    let body = serde_json::json!({ "query": "" });

    let missing = client
        .post(format!("{base}/v1/ask"))
        .json(&body)
        .send()
        .await
        .expect("missing auth request");
    let wrong = client
        .post(format!("{base}/v1/ask"))
        .header("authorization", "Bearer wrong")
        .json(&body)
        .send()
        .await
        .expect("wrong auth request");

    stop(shutdown, handle).await;
    assert_eq!(missing.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
#[serial]
async fn all_v1_rest_routes_reject_missing_auth_when_auth_is_configured() {
    let _env = EnvGuard::set(Some("secret"));
    let (base, shutdown, handle) =
        spawn_full_test_server(AuthPolicy::Mounted { auth_state: None }).await;
    let client = reqwest::Client::new();
    let job_id = Uuid::nil();
    let crawl_job = format!("/v1/crawl/{job_id}");
    let crawl_cancel = format!("/v1/crawl/{job_id}/cancel");
    let embed_job = format!("/v1/embed/{job_id}");
    let embed_cancel = format!("/v1/embed/{job_id}/cancel");
    let extract_job = format!("/v1/extract/{job_id}");
    let extract_cancel = format!("/v1/extract/{job_id}/cancel");
    let ingest_job = format!("/v1/ingest/{job_id}");
    let ingest_cancel = format!("/v1/ingest/{job_id}/cancel");
    let watch_run = format!("/v1/watch/{job_id}/run");
    let routes = [
        ("GET", "/v1/capabilities"),
        ("GET", "/v1/sources"),
        ("GET", "/v1/domains"),
        ("GET", "/v1/stats"),
        ("GET", "/v1/status"),
        ("GET", "/v1/doctor"),
        ("POST", "/v1/ask"),
        ("POST", "/v1/ask/stream"),
        ("POST", "/v1/query"),
        ("POST", "/v1/retrieve"),
        ("POST", "/v1/evaluate"),
        ("POST", "/v1/suggest"),
        ("POST", "/v1/scrape"),
        ("POST", "/v1/map"),
        ("POST", "/v1/search"),
        ("POST", "/v1/research"),
        ("POST", "/v1/crawl"),
        ("GET", crawl_job.as_str()),
        ("POST", crawl_cancel.as_str()),
        ("POST", "/v1/crawl/cleanup"),
        ("DELETE", "/v1/crawl"),
        ("POST", "/v1/crawl/recover"),
        ("POST", "/v1/embed"),
        ("GET", embed_job.as_str()),
        ("POST", embed_cancel.as_str()),
        ("POST", "/v1/embed/cleanup"),
        ("DELETE", "/v1/embed"),
        ("POST", "/v1/embed/recover"),
        ("POST", "/v1/extract"),
        ("GET", extract_job.as_str()),
        ("POST", extract_cancel.as_str()),
        ("POST", "/v1/extract/cleanup"),
        ("DELETE", "/v1/extract"),
        ("POST", "/v1/extract/recover"),
        ("POST", "/v1/ingest"),
        ("GET", ingest_job.as_str()),
        ("POST", ingest_cancel.as_str()),
        ("POST", "/v1/ingest/cleanup"),
        ("DELETE", "/v1/ingest"),
        ("POST", "/v1/ingest/recover"),
        ("POST", "/v1/dedupe"),
        ("GET", "/v1/watch"),
        ("POST", "/v1/watch"),
        ("POST", watch_run.as_str()),
    ];

    for (method, path) in routes {
        let response = match method {
            "DELETE" => client.delete(format!("{base}{path}")).send().await,
            "GET" => client.get(format!("{base}{path}")).send().await,
            "POST" => {
                client
                    .post(format!("{base}{path}"))
                    .json(&serde_json::json!({}))
                    .send()
                    .await
            }
            _ => unreachable!("unexpected test method"),
        }
        .unwrap_or_else(|err| panic!("{method} {path} failed: {err}"));
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{method} {path} should reject missing auth"
        );
    }

    stop(shutdown, handle).await;
}

#[tokio::test]
#[serial]
async fn v1_actions_is_not_mounted_after_rest_cutover() {
    let _env = EnvGuard::set(None);
    let (base, shutdown, handle) = spawn_full_test_server(AuthPolicy::LoopbackDev).await;
    let response = reqwest::Client::new()
        .post(format!("{base}/v1/actions"))
        .send()
        .await
        .expect("v1 actions request");

    stop(shutdown, handle).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
#[serial]
async fn v1_migrate_is_not_mounted_after_rest_cutover() {
    let _env = EnvGuard::set(Some("secret"));
    let (base, shutdown, handle) =
        spawn_full_test_server(AuthPolicy::Mounted { auth_state: None }).await;
    let response = reqwest::Client::new()
        .post(format!("{base}/v1/migrate"))
        .header("authorization", "Bearer secret")
        .json(&serde_json::json!({ "from": "src", "to": "dst" }))
        .send()
        .await
        .expect("v1 migrate request");

    stop(shutdown, handle).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
#[serial]
async fn openapi_docs_are_public_and_list_rest_routes() {
    let _env = EnvGuard::set(Some("secret"));
    let (base, shutdown, handle) =
        spawn_full_test_server(AuthPolicy::Mounted { auth_state: None }).await;
    let client = reqwest::Client::new();

    let spec = client
        .get(format!("{base}/api-docs/openapi.json"))
        .send()
        .await
        .expect("openapi spec request");
    let ui = client
        .get(format!("{base}/docs"))
        .send()
        .await
        .expect("swagger ui request");

    assert_eq!(spec.status(), StatusCode::OK);
    assert_eq!(ui.status(), StatusCode::OK);

    let spec_json: serde_json::Value = spec.json().await.expect("openapi json");
    let paths = spec_json["paths"].as_object().expect("openapi paths");
    for path in [
        "/v1/query",
        "/v1/ask",
        "/v1/ask/stream",
        "/v1/crawl",
        "/v1/crawl/{id}",
        "/v1/embed",
        "/v1/extract",
        "/v1/ingest",
        "/v1/watch",
        "/v1/watch/{id}/run",
    ] {
        assert!(
            paths.contains_key(path),
            "OpenAPI spec should include {path}"
        );
    }

    stop(shutdown, handle).await;
}

#[tokio::test]
#[serial]
async fn loopback_dev_blocks_destructive_rest_routes_without_auth() {
    let _env = EnvGuard::set(None);
    let (base, shutdown, handle) = spawn_full_test_server(AuthPolicy::LoopbackDev).await;
    let client = reqwest::Client::new();
    let job_id = Uuid::nil();
    let watch_run = format!("/v1/watch/{job_id}/run");
    let crawl_cancel = format!("/v1/crawl/{job_id}/cancel");
    let embed_cancel = format!("/v1/embed/{job_id}/cancel");
    let extract_cancel = format!("/v1/extract/{job_id}/cancel");
    let ingest_cancel = format!("/v1/ingest/{job_id}/cancel");
    let routes = [
        ("POST", "/v1/dedupe"),
        ("POST", "/v1/watch"),
        ("POST", watch_run.as_str()),
        ("POST", "/v1/crawl"),
        ("POST", crawl_cancel.as_str()),
        ("POST", "/v1/crawl/cleanup"),
        ("DELETE", "/v1/crawl"),
        ("POST", "/v1/crawl/recover"),
        ("POST", "/v1/embed"),
        ("POST", embed_cancel.as_str()),
        ("POST", "/v1/embed/cleanup"),
        ("DELETE", "/v1/embed"),
        ("POST", "/v1/embed/recover"),
        ("POST", "/v1/extract"),
        ("POST", extract_cancel.as_str()),
        ("POST", "/v1/extract/cleanup"),
        ("DELETE", "/v1/extract"),
        ("POST", "/v1/extract/recover"),
        ("POST", "/v1/ingest"),
        ("POST", ingest_cancel.as_str()),
        ("POST", "/v1/ingest/cleanup"),
        ("DELETE", "/v1/ingest"),
        ("POST", "/v1/ingest/recover"),
    ];

    for (method, path) in routes {
        let response = match method {
            "DELETE" => client.delete(format!("{base}{path}")).send().await,
            "POST" => {
                client
                    .post(format!("{base}{path}"))
                    .json(&serde_json::json!({}))
                    .send()
                    .await
            }
            _ => unreachable!("unexpected test method"),
        }
        .unwrap_or_else(|err| panic!("{method} {path} failed: {err}"));
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{method} {path} should reject missing auth in loopback dev"
        );
    }

    stop(shutdown, handle).await;
}

#[tokio::test]
#[serial]
async fn loopback_dev_allows_non_destructive_write_routes_without_auth() {
    let _env = EnvGuard::set(None);
    let (base, shutdown, handle) = spawn_full_test_server(AuthPolicy::LoopbackDev).await;
    let response = reqwest::Client::new()
        .post(format!("{base}/v1/ask"))
        .json(&serde_json::json!({ "query": "" }))
        .send()
        .await
        .expect("ask request");

    stop(shutdown, handle).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
#[serial]
async fn v1_ask_auth_layer_accepts_bearer_and_x_api_key() {
    let _env = EnvGuard::set(Some("secret"));
    let (base, shutdown, handle) =
        spawn_ask_test_server(AuthPolicy::Mounted { auth_state: None }).await;
    let client = reqwest::Client::new();
    let body = serde_json::json!({ "query": "" });

    let bearer = client
        .post(format!("{base}/v1/ask"))
        .header("authorization", "Bearer secret")
        .json(&body)
        .send()
        .await
        .expect("bearer auth request");
    let api_key = client
        .post(format!("{base}/v1/ask"))
        .header("x-api-key", "secret")
        .json(&body)
        .send()
        .await
        .expect("x-api-key auth request");

    stop(shutdown, handle).await;
    assert_eq!(bearer.status(), StatusCode::BAD_REQUEST);
    assert_eq!(api_key.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
#[serial]
async fn v1_ask_rejects_removed_graph_field() {
    let _env = EnvGuard::set(None);
    let (base, shutdown, handle) = spawn_ask_test_server(AuthPolicy::LoopbackDev).await;
    let client = reqwest::Client::new();

    let response = client
        .post(format!("{base}/v1/ask"))
        .json(&serde_json::json!({ "query": "test", "graph": false }))
        .send()
        .await
        .expect("graph request");

    stop(shutdown, handle).await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}
