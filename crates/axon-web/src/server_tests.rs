//! Tests for `src/web/server.rs` ask classification and ask route contracts.

#![allow(unsafe_code)]

use super::HttpError;
use super::test_support::{EnvGuard, spawn_ask_test_server, spawn_full_test_server, stop};
use axon_authz::http::AuthPolicy;
use axon_services::types::{RestRouteAuth, rest_route_inventory};
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
    let routes = rest_route_inventory()
        .iter()
        .filter(|route| route.auth != RestRouteAuth::Public);

    for route in routes {
        let method = route.method;
        let path = route_to_test_path(route.path);
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
            "PUT" => {
                client
                    .put(format!("{base}{path}"))
                    .json(&serde_json::json!({}))
                    .send()
                    .await
            }
            "PATCH" => {
                client
                    .patch(format!("{base}{path}"))
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
        let body: serde_json::Value = response
            .json()
            .await
            .unwrap_or_else(|err| panic!("{method} {path} returned non-JSON auth error: {err}"));
        assert_eq!(body["ok"], false, "{method} {path}");
        assert_eq!(body["error"]["code"], "auth.missing", "{method} {path}");
    }

    stop(shutdown, handle).await;
}

fn route_to_test_path(path: &str) -> String {
    path.replace("{id}", &Uuid::nil().to_string())
        .replace("{artifact_id}", "artifact_report_missing")
        .replace("{memory_id}", "mem_test")
        .replace("{watch_id}", "watch_test")
        .replace("{path}", "missing.txt")
}

#[test]
fn openapi_document_matches_openapi_route_inventory() {
    let document = crate::server::openapi_document();
    let documented = document
        .paths
        .paths
        .iter()
        .flat_map(|(path, item)| {
            [
                ("GET", item.get.as_ref()),
                ("PUT", item.put.as_ref()),
                ("POST", item.post.as_ref()),
                ("DELETE", item.delete.as_ref()),
                ("OPTIONS", item.options.as_ref()),
                ("HEAD", item.head.as_ref()),
                ("PATCH", item.patch.as_ref()),
                ("TRACE", item.trace.as_ref()),
            ]
            .into_iter()
            .filter_map(move |(method, operation)| {
                operation.map(|_| (method.to_string(), path.as_str().to_string()))
            })
        })
        .collect::<std::collections::BTreeSet<_>>();

    let expected = rest_route_inventory()
        .iter()
        .filter(|route| route.openapi)
        .map(|route| (route.method.to_string(), route.path.to_string()))
        .collect::<std::collections::BTreeSet<_>>();

    assert_eq!(expected, documented);
}

/// Close the one dispatch surface with no compiler check: a `.route("/v1/...")`
/// or `.nest("/v1/...")` added to the central route tree without a matching
/// `rest_route_inventory()` entry. The inventory is locked to the OpenAPI
/// document by `openapi_document_matches_openapi_route_inventory`, so an entry
/// missing from the inventory is also missing from the docs; and every inventory
/// route is exercised against the live router by
/// `all_v1_rest_routes_reject_missing_auth_when_auth_is_configured`. This test
/// adds the missing direction (router → inventory). Sub-routes nested inside the
/// per-job routers are covered transitively: their `/v1/<kind>` nest prefix is
/// checked here and their full inventory sub-paths are probed by the auth test.
#[test]
fn routing_registers_no_v1_route_outside_inventory() {
    // Intentionally mounted but absent from the REST/OpenAPI inventory:
    //   /v1/actions, /v1/migrate — removed-surface stubs that only return 404.
    const ALLOWED_UNLISTED: &[&str] = &["/v1/actions", "/v1/migrate"];

    let source = include_str!("server/routing.rs");
    let inventory: std::collections::BTreeSet<&str> =
        rest_route_inventory().iter().map(|r| r.path).collect();

    let mut registered: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for marker in [".route(", ".nest("] {
        for (idx, _) in source.match_indices(marker) {
            // Tolerate rustfmt wrapping the path literal onto its own line
            // (`.route(\n    "/v1/...",`) — skip whitespace/newlines before the
            // opening quote. The old `.route("` literal match missed every
            // multi-line registration, silently scanning only ~70% of routes.
            let after = source[idx + marker.len()..].trim_start();
            let Some(rest) = after.strip_prefix('"') else {
                continue;
            };
            if let Some(end) = rest.find('"') {
                let path = &rest[..end];
                if path.starts_with("/v1") {
                    registered.insert(path.to_string());
                }
            }
        }
    }

    // Self-test floor: the scanner MUST see the multi-line registrations, not
    // silently regress to seeing nothing (which would turn this whole test into a
    // no-op — the exact failure mode it exists to prevent). Both of these
    // routes exercise the scanner's multi-line route parsing.
    for must_see in ["/v1/extract", "/v1/research/stream"] {
        assert!(
            registered.contains(must_see),
            "route scanner missed `{must_see}` — the .route(/.nest( matcher is broken \
             and this test would pass without inspecting real routes. Found: {registered:?}"
        );
    }

    let missing: Vec<String> = registered
        .into_iter()
        .filter(|path| !ALLOWED_UNLISTED.contains(&path.as_str()))
        .filter(|path| {
            // Covered when the path is an inventory route exactly, or is the
            // prefix of a nested router whose sub-paths the inventory lists.
            let exact = inventory.contains(path.as_str());
            let nest_prefix = inventory
                .iter()
                .any(|inv| inv.starts_with(&format!("{path}/")));
            !(exact || nest_prefix)
        })
        .collect();

    assert!(
        missing.is_empty(),
        "routing.rs registers /v1 route(s) absent from rest_route_inventory() \
         (and therefore from the OpenAPI document): {missing:?}. Add each to \
         REST_ROUTE_INVENTORY (src/services/types/route_inventory.rs) and the \
         #[openapi(paths(...))] list in src/web/server/openapi.rs, or to \
         ALLOWED_UNLISTED above if it is intentionally undocumented."
    );
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
async fn scoped_prune_routes_are_not_mounted_after_cutover() {
    let _env = EnvGuard::set(Some("secret"));
    let (base, shutdown, handle) =
        spawn_full_test_server(AuthPolicy::Mounted { auth_state: None }).await;
    let client = reqwest::Client::new();

    for path in ["/v1/prune/dedupe", "/v1/prune/purge"] {
        let response = client
            .post(format!("{base}{path}"))
            .header("authorization", "Bearer secret")
            .json(&serde_json::json!({}))
            .send()
            .await
            .expect("removed prune route request");
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
    }

    stop(shutdown, handle).await;
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
    assert_eq!(
        ui.headers()
            .get("x-content-type-options")
            .and_then(|value| value.to_str().ok()),
        Some("nosniff")
    );
    assert_eq!(
        ui.headers()
            .get("referrer-policy")
            .and_then(|value| value.to_str().ok()),
        Some("no-referrer")
    );
    assert_eq!(
        ui.headers()
            .get("x-frame-options")
            .and_then(|value| value.to_str().ok()),
        Some("DENY")
    );
    assert!(ui.headers().contains_key("content-security-policy"));
    assert!(ui.headers().contains_key("permissions-policy"));

    let spec_json: serde_json::Value = spec.json().await.expect("openapi json");
    let paths = spec_json["paths"].as_object().expect("openapi paths");
    for path in [
        "/v1/query",
        "/v1/ask",
        "/v1/ask/stream",
        "/v1/sources",
        "/v1/extract",
        "/v1/watches",
        "/v1/watches/{watch_id}/exec",
        "/v1/prune/plan",
        "/v1/prune/exec",
        "/v1/reset/plan",
        "/v1/reset/exec",
        "/v1/memories",
        "/v1/memories/{memory_id}",
        "/v1/memories/import",
        "/v1/memories/export",
        "/v1/mobile/sessions",
        "/v1/mobile/sessions/{id}",
        "/v1/artifacts",
        "/v1/artifacts/{artifact_id}",
        "/v1/artifacts/{artifact_id}/content",
    ] {
        assert!(
            paths.contains_key(path),
            "OpenAPI spec should include {path}"
        );
    }
    for removed in ["/v1/prune/dedupe", "/v1/prune/purge", "/v1/memory"] {
        assert!(
            !paths.contains_key(removed),
            "OpenAPI spec must not include removed route {removed}"
        );
    }
    for removed in [
        "/v1/extract/{id}",
        "/v1/extract/{id}/cancel",
        "/v1/extract/cleanup",
        "/v1/extract/recover",
    ] {
        assert!(
            !paths.contains_key(removed),
            "OpenAPI spec must not include removed extract lifecycle route {removed}"
        );
    }

    stop(shutdown, handle).await;
}

#[tokio::test]
#[serial]
async fn mobile_session_routes_round_trip_and_reject_stale_updates() {
    let _env = EnvGuard::set(Some("secret"));
    let (base, shutdown, handle) =
        spawn_full_test_server(AuthPolicy::Mounted { auth_state: None }).await;
    let client = reqwest::Client::new();
    let id = "session_test";
    let session = serde_json::json!({
        "session": {
            "id": id,
            "title": "Hello",
            "first_message_preview": "Hello",
            "turn_count": 1,
            "injected_op_count": 0,
            "created_at": 1000,
            "updated_at": 2000,
            "items": [
                {
                    "kind": "user",
                    "text": "Hello",
                    "payload": {},
                    "timestamp": 1000
                }
            ]
        }
    });

    let put = client
        .put(format!("{base}/v1/mobile/sessions/{id}"))
        .header("authorization", "Bearer secret")
        .json(&session)
        .send()
        .await
        .expect("put mobile session");
    assert_eq!(put.status(), StatusCode::OK);

    let get = client
        .get(format!("{base}/v1/mobile/sessions/{id}"))
        .header("authorization", "Bearer secret")
        .send()
        .await
        .expect("get mobile session");
    assert_eq!(get.status(), StatusCode::OK);
    let detail: serde_json::Value = get.json().await.expect("detail json");
    assert_eq!(detail["session"]["id"], id);

    let list = client
        .get(format!("{base}/v1/mobile/sessions"))
        .header("authorization", "Bearer secret")
        .send()
        .await
        .expect("list mobile sessions");
    assert_eq!(list.status(), StatusCode::OK);
    let list_body: serde_json::Value = list.json().await.expect("list json");
    assert!(
        list_body["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|session| { session["id"] == id })
    );

    let stale = serde_json::json!({
        "session": {
            "id": id,
            "title": "Stale",
            "first_message_preview": "Stale",
            "turn_count": 1,
            "injected_op_count": 0,
            "created_at": 1000,
            "updated_at": 1500,
            "items": [
                {
                    "kind": "user",
                    "text": "Stale",
                    "payload": {},
                    "timestamp": 1000
                }
            ]
        }
    });
    let stale_response = client
        .put(format!("{base}/v1/mobile/sessions/{id}"))
        .header("authorization", "Bearer secret")
        .json(&stale)
        .send()
        .await
        .expect("stale put mobile session");
    assert_eq!(stale_response.status(), StatusCode::CONFLICT);

    let delete = client
        .delete(format!("{base}/v1/mobile/sessions/{id}"))
        .header("authorization", "Bearer secret")
        .send()
        .await
        .expect("delete mobile session");
    assert_eq!(delete.status(), StatusCode::OK);

    let missing = client
        .get(format!("{base}/v1/mobile/sessions/{id}"))
        .header("authorization", "Bearer secret")
        .send()
        .await
        .expect("get deleted mobile session");
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);

    stop(shutdown, handle).await;
}

#[tokio::test]
#[serial]
async fn loopback_dev_can_read_empty_mobile_session_list_without_auth_extension() {
    let _env = EnvGuard::set(None);
    let (base, shutdown, handle) = spawn_full_test_server(AuthPolicy::LoopbackDev).await;
    let response = reqwest::Client::new()
        .get(format!("{base}/v1/mobile/sessions"))
        .send()
        .await
        .expect("loopback mobile sessions request");

    stop(shutdown, handle).await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
#[serial]
async fn loopback_dev_blocks_destructive_rest_routes_without_auth() {
    let _env = EnvGuard::set(None);
    let (base, shutdown, handle) = spawn_full_test_server(AuthPolicy::LoopbackDev).await;
    let client = reqwest::Client::new();
    let job_id = Uuid::nil();
    let watch_exec = format!("/v1/watches/{job_id}/exec");
    let mobile_session = "/v1/mobile/sessions/test_session";
    let memory_link = "/v1/memories/mem_test/link";
    let memory_supersede = "/v1/memories/mem_test/supersede";
    let memory_reinforce = "/v1/memories/mem_test/reinforce";
    let memory_contradict = "/v1/memories/mem_test/contradict";
    let memory_pin = "/v1/memories/mem_test/pin";
    let memory_archive = "/v1/memories/mem_test/archive";
    let memory_compact_one = "/v1/memories/mem_test/compact";
    let memory_forget = "/v1/memories/mem_test";
    let routes = [
        ("POST", "/v1/prune/plan"),
        ("POST", "/v1/prune/exec"),
        ("POST", "/v1/reset/plan"),
        ("POST", "/v1/reset/exec"),
        ("POST", "/v1/sources"),
        ("POST", "/v1/watches"),
        ("POST", watch_exec.as_str()),
        ("POST", "/v1/extract"),
        ("POST", "/v1/memories"),
        // `/v1/memories/search` and `/v1/memories/context` moved to
        // `axon:read` (U2-20/C6-20, query-shaped surfaces) and are covered by
        // `loopback_dev_allows_non_destructive_write_routes_without_auth`
        // instead — they pass through loopback dev without auth like other
        // read routes.
        ("POST", "/v1/memories/review"),
        ("POST", "/v1/memories/compact"),
        ("POST", memory_link),
        ("POST", memory_supersede),
        ("POST", memory_reinforce),
        ("POST", memory_contradict),
        ("POST", memory_pin),
        ("POST", memory_archive),
        ("POST", memory_compact_one),
        ("DELETE", memory_forget),
        ("POST", "/v1/memories/import"),
        ("POST", "/v1/memories/export"),
        ("PUT", mobile_session),
        ("DELETE", mobile_session),
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
            "PUT" => {
                client
                    .put(format!("{base}{path}"))
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
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{base}/v1/ask"))
        .json(&serde_json::json!({ "query": "" }))
        .send()
        .await
        .expect("ask request");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // U2-20/C6-20: memory search/context default to `axon:read` and pass
    // through loopback dev without auth like other read routes -- neither
    // should ever answer 401 here.
    for path in ["/v1/memories/search", "/v1/memories/context"] {
        let response = client
            .post(format!("{base}{path}"))
            .json(&serde_json::json!({}))
            .send()
            .await
            .unwrap_or_else(|err| panic!("POST {path} failed: {err}"));
        assert_ne!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "POST {path} should not require auth in loopback dev"
        );
    }

    stop(shutdown, handle).await;
}

#[tokio::test]
#[serial]
async fn removed_v1_memory_route_returns_not_found() {
    let _env = EnvGuard::set(Some("secret"));
    let (base, shutdown, handle) =
        spawn_full_test_server(AuthPolicy::Mounted { auth_state: None }).await;

    let response = reqwest::Client::new()
        .post(format!("{base}/v1/memory"))
        .bearer_auth("secret")
        .json(&serde_json::json!({ "subaction": "search" }))
        .send()
        .await
        .expect("memory request");
    let status = response.status();
    stop(shutdown, handle).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
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
