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
        .replace("{artifact_id}", "art_report_missing")
        .replace("{upload_id}", "upl_missing")
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
        "/v1/uploads",
        "/v1/uploads/{upload_id}",
        "/v1/uploads/{upload_id}/content",
        "/v1/uploads/{upload_id}/complete",
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

// ── Migrated from the deleted `handlers/rest.rs` shadow router (M3 review) ──
//
// `handlers/rest.rs` and `rest_tests.rs` re-implemented a second `/v1/*`
// router purely so a test suite could exercise scope-guard middleware — the
// live router built in `routing.rs` was never mounted with it. Worse: its
// `sync_post::v1_sources` called `axon_services::index_source` directly with
// no per-source authorization boundary, unlike the live
// `handlers::sources::index_source` (see that module's doc comment), so the
// dead module was a live SSRF/local-filesystem/tool-execution ingress that
// merely happened not to be wired into any router. The tests below carry the
// genuinely valuable assertions from `rest_tests.rs` over to the live router
// via `spawn_full_test_server`, adapted where the live handlers' error codes
// differ from the shadow router's bespoke `require_field`/`rest_error`
// helpers (`handlers::sources::index_source` reports empty `source` as
// `route.validation.missing_field`; every other handler here funnels through
// `HttpError::bad_request` → `route.validation.invalid_field`). All bodies
// touching `/v1/sources` or `/v1/extract` use `AuthPolicy::Mounted` with a
// bearer token rather than `LoopbackDev`, because the live router's
// `block_loopback_destructive_request` guard (`routing_loopback_guard.rs`)
// blocks those two routes outright in loopback-dev-without-auth mode — a
// stricter behavior the shadow router never had (it had no destructive-route
// guard at all).

/// Restored projection routes are mounted while retired cleanup aliases remain
/// absent, and the canonical source route remains available.
#[tokio::test]
#[serial]
async fn legacy_indexing_routes_are_absent_and_sources_present_on_live_router() {
    let _env = EnvGuard::set(Some("secret"));
    let (base, shutdown, handle) =
        spawn_full_test_server(AuthPolicy::Mounted { auth_state: None }).await;
    let client = reqwest::Client::new();

    for path in [
        "/v1/scrape",
        "/v1/crawl",
        "/v1/embed",
        "/v1/ingest",
        "/v1/code-search",
    ] {
        let response = client
            .post(format!("{base}{path}"))
            .header("authorization", "Bearer secret")
            .json(&serde_json::json!({}))
            .send()
            .await
            .unwrap_or_else(|e| panic!("post {path}: {e}"));
        assert_ne!(
            response.status(),
            StatusCode::NOT_FOUND,
            "restored route {path}"
        );
        assert_ne!(
            response.status(),
            StatusCode::METHOD_NOT_ALLOWED,
            "restored route {path}"
        );
    }

    for path in ["/v1/purge", "/v1/dedupe"] {
        let response = client
            .post(format!("{base}{path}"))
            .json(&serde_json::json!({}))
            .send()
            .await
            .unwrap_or_else(|e| panic!("post {path}: {e}"));
        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "removed route {path} should 404"
        );
    }

    let response = client
        .post(format!("{base}/v1/sources"))
        .header("authorization", "Bearer secret")
        .json(&serde_json::json!({ "source": "" }))
        .send()
        .await
        .expect("sources request");
    let status = response.status();
    let body: serde_json::Value = response.json().await.expect("json body");
    stop(shutdown, handle).await;
    assert_ne!(
        status,
        StatusCode::NOT_FOUND,
        "POST /v1/sources should be mounted"
    );
    assert_ne!(
        status,
        StatusCode::METHOD_NOT_ALLOWED,
        "POST /v1/sources should be mounted"
    );
    assert_eq!(status, StatusCode::BAD_REQUEST, "empty source is a 400");
    assert_eq!(body["error"]["code"], "route.validation.missing_field");
}

/// Extract lifecycle/status/control routes moved under `/v1/jobs`; the
/// family-scoped `/v1/extract/*` routes must stay absent from the live
/// router. Ported from `rest_tests.rs::extract_lifecycle_routes_are_removed`.
#[tokio::test]
#[serial]
async fn extract_lifecycle_routes_are_removed_on_live_router() {
    let _env = EnvGuard::set(Some("secret"));
    let (base, shutdown, handle) =
        spawn_full_test_server(AuthPolicy::Mounted { auth_state: None }).await;
    let client = reqwest::Client::new();
    let unknown = Uuid::nil().to_string();

    for (method, path, expected) in [
        (
            "GET",
            "/v1/extract".to_string(),
            StatusCode::METHOD_NOT_ALLOWED,
        ),
        (
            "DELETE",
            "/v1/extract".to_string(),
            StatusCode::METHOD_NOT_ALLOWED,
        ),
        (
            "POST",
            "/v1/extract/cleanup".to_string(),
            StatusCode::NOT_FOUND,
        ),
        (
            "POST",
            "/v1/extract/recover".to_string(),
            StatusCode::NOT_FOUND,
        ),
        (
            "GET",
            format!("/v1/extract/{unknown}"),
            StatusCode::NOT_FOUND,
        ),
        (
            "POST",
            format!("/v1/extract/{unknown}/cancel"),
            StatusCode::NOT_FOUND,
        ),
    ] {
        let url = format!("{base}{path}");
        let response = match method {
            "GET" => {
                client
                    .get(&url)
                    .header("authorization", "Bearer secret")
                    .send()
                    .await
            }
            "POST" => {
                client
                    .post(&url)
                    .header("authorization", "Bearer secret")
                    .send()
                    .await
            }
            "DELETE" => {
                client
                    .delete(&url)
                    .header("authorization", "Bearer secret")
                    .send()
                    .await
            }
            _ => unreachable!(),
        }
        .unwrap_or_else(|e| panic!("{method} {path}: {e}"));
        assert_eq!(response.status(), expected, "{method} {path}");
    }

    stop(shutdown, handle).await;
}

/// The retired family-scoped watch routes (`/v1/watch`, singular) must stay
/// absent; their canonical replacement is `/v1/watches`. Ported from
/// `rest_tests.rs::retired_watch_routes_are_absent`.
#[tokio::test]
#[serial]
async fn retired_watch_routes_are_absent_on_live_router() {
    let _env = EnvGuard::set(None);
    let (base, shutdown, handle) = spawn_full_test_server(AuthPolicy::LoopbackDev).await;
    let client = reqwest::Client::new();
    let unknown = Uuid::nil().to_string();

    for (method, path) in [
        ("GET", "/v1/watch".to_string()),
        ("POST", "/v1/watch".to_string()),
        ("GET", format!("/v1/watch/{unknown}")),
        ("POST", format!("/v1/watch/{unknown}/run")),
    ] {
        let request = match method {
            "GET" => client.get(format!("{base}{path}")),
            "POST" => client.post(format!("{base}{path}")),
            _ => unreachable!(),
        };
        let response = request.send().await.expect("retired watch route request");
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{method} {path}");
    }

    stop(shutdown, handle).await;
}

/// F2 sync POST routes that are not loopback-destructive-gated
/// (`/v1/query`, `/v1/retrieve`, `/v1/map`, `/v1/search`, `/v1/research`)
/// return 400 `route.validation.invalid_field` when the required string
/// field is empty or whitespace-only. Ported from
/// `rest_tests.rs::sync_post_routes_reject_empty_required_fields`, minus the
/// `/v1/sources` case (covered separately above, and destructive-gated in
/// loopback dev so it needs a bearer token instead).
#[tokio::test]
#[serial]
async fn sync_post_routes_reject_empty_required_fields_on_live_router() {
    let _env = EnvGuard::set(None);
    let (base, shutdown, handle) = spawn_full_test_server(AuthPolicy::LoopbackDev).await;
    let client = reqwest::Client::new();

    let cases = [
        ("/v1/query", serde_json::json!({ "query": "" })),
        ("/v1/retrieve", serde_json::json!({ "url": "" })),
        ("/v1/map", serde_json::json!({ "url": "" })),
        ("/v1/search", serde_json::json!({ "query": "  " })),
        ("/v1/research", serde_json::json!({ "query": "" })),
    ];

    for (path, body) in cases {
        let response = client
            .post(format!("{base}{path}"))
            .json(&body)
            .send()
            .await
            .unwrap_or_else(|e| panic!("request {path}: {e}"));
        let status = response.status();
        let body: serde_json::Value = response.json().await.expect("json body");
        assert_eq!(status, StatusCode::BAD_REQUEST, "{path} expected 400");
        assert_eq!(
            body["error"]["code"], "route.validation.invalid_field",
            "{path} code"
        );
    }

    stop(shutdown, handle).await;
}

/// F2 `/v1/search` `time_range` parsing rejects invalid values. Ported from
/// `rest_tests.rs::sync_post_search_rejects_invalid_time_range`.
#[tokio::test]
#[serial]
async fn sync_post_search_rejects_invalid_time_range_on_live_router() {
    let _env = EnvGuard::set(None);
    let (base, shutdown, handle) = spawn_full_test_server(AuthPolicy::LoopbackDev).await;
    let client = reqwest::Client::new();

    let response = client
        .post(format!("{base}/v1/search"))
        .json(&serde_json::json!({ "query": "test", "time_range": "decade" }))
        .send()
        .await
        .expect("request");
    let status = response.status();
    let body: serde_json::Value = response.json().await.expect("json body");

    stop(shutdown, handle).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["ok"], false);
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap_or("")
            .contains("time_range"),
        "expected time_range error, got {body}"
    );
}

/// F3 `/v1/extract` rejects an empty `urls` list and an SSRF-blocked private
/// URL before enqueue, both as 400 `route.validation.invalid_field`. Ported
/// from `rest_tests.rs::async_submit_routes_reject_empty_required_fields`
/// and `::async_submit_routes_reject_private_urls_before_enqueue`, merged
/// since both exercise the same live `/v1/extract` validation chain
/// (`handlers::async_jobs::start_extract`) and both need a bearer token
/// because `/v1/extract` is loopback-destructive-gated.
#[tokio::test]
#[serial]
async fn async_submit_routes_reject_invalid_urls_on_live_router() {
    let _env = EnvGuard::set(Some("secret"));
    let (base, shutdown, handle) =
        spawn_full_test_server(AuthPolicy::Mounted { auth_state: None }).await;
    let client = reqwest::Client::new();

    for body in [
        serde_json::json!({ "urls": [] }),
        serde_json::json!({ "urls": ["http://127.0.0.1/admin"] }),
    ] {
        let response = client
            .post(format!("{base}/v1/extract"))
            .header("authorization", "Bearer secret")
            .json(&body)
            .send()
            .await
            .unwrap_or_else(|e| panic!("request /v1/extract with {body}: {e}"));
        let status = response.status();
        let response_body: serde_json::Value = response.json().await.expect("json body");
        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "/v1/extract with {body} expected 400"
        );
        assert_eq!(
            response_body["error"]["code"], "route.validation.invalid_field",
            "/v1/extract with {body} code"
        );
    }

    stop(shutdown, handle).await;
}

/// Every body struct with `#[serde(deny_unknown_fields)]` rejects an unknown
/// field on the live router. Ported from
/// `rest_tests.rs::sync_post_rejects_unknown_fields` and
/// `::all_submit_routes_reject_unknown_fields`, merged into one
/// parametrized case list. All requests carry a bearer token so
/// `/v1/sources` and `/v1/extract` (loopback-destructive-gated) are
/// reachable alongside the non-gated routes.
#[tokio::test]
#[serial]
async fn all_submit_routes_reject_unknown_fields_on_live_router() {
    let _env = EnvGuard::set(Some("secret"));
    let (base, shutdown, handle) =
        spawn_full_test_server(AuthPolicy::Mounted { auth_state: None }).await;
    let client = reqwest::Client::new();

    let cases: &[(&str, serde_json::Value)] = &[
        ("/v1/query", serde_json::json!({ "query": "test", "_x": 1 })),
        (
            "/v1/retrieve",
            serde_json::json!({ "url": "https://example.com", "_x": 1 }),
        ),
        (
            "/v1/map",
            serde_json::json!({ "url": "https://example.com", "_x": 1 }),
        ),
        ("/v1/suggest", serde_json::json!({ "_x": 1 })),
        (
            "/v1/search",
            serde_json::json!({ "query": "test", "_x": 1 }),
        ),
        (
            "/v1/research",
            serde_json::json!({ "query": "test", "_x": 1 }),
        ),
        (
            "/v1/sources",
            serde_json::json!({ "source": "https://example.com", "_x": 1 }),
        ),
        (
            "/v1/extract",
            serde_json::json!({ "urls": ["https://example.com"], "_x": 1 }),
        ),
    ];

    for (path, body) in cases {
        let response = client
            .post(format!("{base}{path}"))
            .header("authorization", "Bearer secret")
            .json(body)
            .send()
            .await
            .unwrap_or_else(|e| panic!("request {path}: {e}"));
        let status = response.status();
        assert!(
            status.is_client_error(),
            "{path} with unknown field should return 4xx, got {status}"
        );
        assert_ne!(status, StatusCode::NOT_FOUND, "{path} should be mounted");
    }

    stop(shutdown, handle).await;
}

// ── New: closes the real coverage gap the shadow router masked ─────────────
//
// The shadow router's `bearer_token_passes_write_scope_guard` only ever
// proved a bearer token satisfies the *broad* `axon:write` router-layer
// scope check. It never exercised `handlers::sources::index_source`'s
// *per-source* fine-grained authorization boundary
// (`authorize_source_request`), because the shadow router's own
// `sync_post::v1_sources` didn't call it at all — it called
// `axon_services::index_source` directly with no auth boundary whatsoever.
// That is the actual live gap: a caller holding only the static bearer
// token (granted `axon:read`+`axon:write`+`axon:admin`, never the
// fine-grained `axon:local`/`axon:execute` scopes — see
// `axon_authz::http::build_auth_layer`) must still be denied when the
// source classifies as `SafetyClass::LocalFilesystem`, and the only way to
// prove that is against the mounted router with a real request.

/// A valid write-scoped bearer token is NOT sufficient to index a
/// local-filesystem source: `authorize_source_request` in the live
/// `handlers::sources::index_source` must independently deny it for lacking
/// `axon:local`. This is the fine-grained scope-discrimination assertion
/// that `rest_tests.rs` explicitly documented as untestable without an
/// OAuth token (see its `bearer_token_passes_write_scope_guard` doc
/// comment) — the static-bearer path can't prove denial for a scope it's
/// never granted, but it CAN prove denial for a scope (`axon:local`) it
/// deliberately never grants. That is exactly the fine-grained boundary
/// this test exercises.
#[tokio::test]
#[serial]
async fn v1_sources_denies_local_path_without_axon_local_scope() {
    let _env = EnvGuard::set(Some("secret"));
    let (base, shutdown, handle) =
        spawn_full_test_server(AuthPolicy::Mounted { auth_state: None }).await;
    let client = reqwest::Client::new();
    let local_dir = tempfile::Builder::new()
        .prefix("axon-web-local-")
        .tempdir()
        .expect("visible tempdir");

    let response = client
        .post(format!("{base}/v1/sources"))
        .header("authorization", "Bearer secret")
        .json(&serde_json::json!({ "source": local_dir.path().to_string_lossy() }))
        .send()
        .await
        .expect("sources request");
    let status = response.status();
    let body: serde_json::Value = response.json().await.expect("json body");

    stop(shutdown, handle).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "expected fine-grained auth denial for a local path, got {status}: {body}"
    );
    assert_eq!(body["ok"], false);
    assert_eq!(body["error"]["code"], "auth.forbidden");
    assert_eq!(body["error"]["details"]["required_scope"], "axon:local");
    assert_eq!(body["error"]["details"]["safety_class"], "local_filesystem");
}

/// Counterpart to the denial above: the same bearer token IS sufficient for
/// a `PublicNetwork`-classified source (no fine-grained scope required), so
/// the scope guard discriminates on safety class rather than blanket-denying
/// every `/v1/sources` call. Never 401/403 proves the request reached the
/// handler past both the router-layer `axon:write` check and the per-source
/// boundary. Ported from `rest_tests.rs::bearer_token_passes_write_scope_guard`.
#[tokio::test]
#[serial]
async fn v1_sources_allows_public_network_source_with_write_scope() {
    let _env = EnvGuard::set(Some("secret"));
    let (base, shutdown, handle) =
        spawn_full_test_server(AuthPolicy::Mounted { auth_state: None }).await;
    let client = reqwest::Client::new();

    let response = client
        .post(format!("{base}/v1/sources"))
        .header("authorization", "Bearer secret")
        .json(&serde_json::json!({ "source": "https://example.invalid/" }))
        .send()
        .await
        .expect("sources request");
    let status = response.status();

    stop(shutdown, handle).await;
    assert_ne!(status, StatusCode::UNAUTHORIZED, "valid bearer rejected");
    assert_ne!(status, StatusCode::FORBIDDEN, "valid bearer rejected");
}
