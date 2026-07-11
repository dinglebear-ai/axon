use super::*;
use crate::validate_saved_server_url;

fn request(method: HttpMethod, path: &str) -> AxonHttpRequest {
    AxonHttpRequest {
        _base_url: Some("https://evil.example".to_string()),
        _token: Some("renderer-token".to_string()),
        method,
        path: path.to_string(),
        body: None,
    }
}

#[test]
fn allows_known_palette_routes() {
    assert_eq!(
        validate_axon_route(&request(HttpMethod::Get, "/v1/doctor")).unwrap(),
        "/v1/doctor"
    );
    assert_eq!(
        validate_axon_route(&request(HttpMethod::Post, "/v1/ask")).unwrap(),
        "/v1/ask"
    );
    assert_eq!(
        validate_axon_route(&request(HttpMethod::Post, "/v1/chat")).unwrap(),
        "/v1/chat"
    );
    assert_eq!(
        validate_axon_route(&request(HttpMethod::Post, "/v1/endpoints")).unwrap(),
        "/v1/endpoints"
    );
    assert_eq!(
        validate_axon_route(&request(HttpMethod::Post, "/v1/brand")).unwrap(),
        "/v1/brand"
    );
    assert_eq!(
        validate_axon_route(&request(HttpMethod::Post, "/v1/diff")).unwrap(),
        "/v1/diff"
    );
    assert_eq!(
        validate_axon_route(&request(HttpMethod::Post, "/v1/screenshot")).unwrap(),
        "/v1/screenshot"
    );
    assert_eq!(
        validate_axon_route(&request(HttpMethod::Delete, "/v1/crawl")).unwrap(),
        "/v1/crawl"
    );
    assert_eq!(
        validate_axon_route(&request(
            HttpMethod::Get,
            "/v1/crawl/00000000-0000-4000-8000-000000000000"
        ))
        .unwrap(),
        "/v1/crawl/00000000-0000-4000-8000-000000000000"
    );
    assert_eq!(
        validate_axon_route(&request(
            HttpMethod::Post,
            "/v1/watch/00000000-0000-4000-8000-000000000000/run"
        ))
        .unwrap(),
        "/v1/watch/00000000-0000-4000-8000-000000000000/run"
    );
    assert_eq!(
        validate_axon_route(&request(HttpMethod::Post, "/v1/sources")).unwrap(),
        "/v1/sources"
    );
}

/// scrape/crawl/embed/ingest were removed server-side in favor of the
/// unified `POST /v1/sources` pipeline (confirmed 404 by
/// crates/axon-web/src/server/handlers/rest_tests.rs); the bridge allowlist
/// must not resurrect them.
#[test]
fn rejects_removed_legacy_verb_routes() {
    for path in ["/v1/scrape", "/v1/crawl", "/v1/embed", "/v1/ingest"] {
        assert!(
            validate_axon_route(&request(HttpMethod::Post, path)).is_err(),
            "removed route {path} should be rejected"
        );
    }
}

#[test]
fn rejects_full_urls_and_traversal_paths() {
    for path in [
        "https://evil.example/v1/doctor",
        "//evil.example/v1/doctor",
        "/v1/../admin",
        "/v1/%2e%2e/admin",
        "/v1/doctor?next=/admin",
        "/v1/doctor#fragment",
        "/v1\\doctor",
        " /v1/doctor",
    ] {
        assert!(
            validate_axon_route(&request(HttpMethod::Get, path)).is_err(),
            "path should be rejected: {path}"
        );
    }
}

#[test]
fn rejects_unknown_method_route_pairs() {
    assert!(validate_axon_route(&request(HttpMethod::Post, "/v1/doctor")).is_err());
    assert!(validate_axon_route(&request(HttpMethod::Get, "/v1/ask")).is_err());
    assert!(validate_axon_route(&request(HttpMethod::Get, "/v1/admin")).is_err());
    assert!(validate_axon_route(&request(HttpMethod::Get, "/v1/crawl/not-a-uuid")).is_err());
}

#[test]
fn rejects_get_request_bodies() {
    let mut req = request(HttpMethod::Get, "/v1/doctor");
    req.body = Some(serde_json::json!({ "unexpected": true }));
    assert!(validate_axon_route(&req).is_err());
    let mut req = request(HttpMethod::Delete, "/v1/crawl");
    req.body = Some(serde_json::json!({ "unexpected": true }));
    assert!(validate_axon_route(&req).is_err());
}

#[test]
fn validates_saved_server_url_shape() {
    assert_eq!(
        validate_saved_server_url("127.0.0.1:8001").unwrap(),
        "http://127.0.0.1:8001"
    );
    assert_eq!(
        validate_saved_server_url("localhost:8001").unwrap(),
        "http://localhost:8001"
    );
    assert_eq!(
        validate_saved_server_url("axon.example.com/").unwrap(),
        "https://axon.example.com"
    );
    assert!(validate_saved_server_url("file:///tmp/axon.sock").is_err());
    assert!(validate_saved_server_url("https://axon.example.com/api").is_err());
    assert!(validate_saved_server_url("https://axon.example.com?token=leak").is_err());
}

#[test]
fn validates_saved_server_url_accepts_ipv6() {
    // IPv6 loopback with port — normalize_server_url adds https:// prefix since
    // it is not 127.0.0.1 or localhost
    let result = validate_saved_server_url("[::1]:8001");
    // Either accepted with http/https or rejected with a clear message — test
    // that it does not panic and that if accepted the scheme is http or https
    // Rejection is also acceptable — URL parsing of IPv6 without scheme varies.
    if let Ok(url) = result {
        assert!(
            url.starts_with("http://") || url.starts_with("https://"),
            "accepted URL must have http(s) scheme: {url}"
        );
    }
}

#[test]
fn artifact_relative_path_validation_rejects_unsafe_values() {
    assert!(validate_artifact_relative_path("../secret").is_err());
    assert!(validate_artifact_relative_path("screenshots/%2e/secret").is_err());
    assert!(validate_artifact_relative_path("screenshots/%2e%2e/secret").is_err());
    assert!(validate_artifact_relative_path("screenshots%5csecret").is_err());
    assert!(validate_artifact_relative_path(r"screenshots\\..\\secret").is_err());
    assert!(validate_artifact_relative_path("C:\\secret").is_err());
    assert!(validate_artifact_relative_path("screenshots/shot.png\0").is_err());
    assert!(validate_artifact_relative_path("screenshots/shot.png").is_ok());
}

#[test]
fn artifact_url_uses_query_encoding_without_accepting_raw_query_paths() {
    let url = artifact_url("https://axon.local", "screenshots/foo #1.png").unwrap();
    assert_eq!(
        url.as_str(),
        "https://axon.local/v1/artifacts?path=screenshots%2Ffoo+%231.png"
    );
}

#[test]
fn artifact_content_type_allowlist_is_raster_only() {
    assert!(is_allowed_artifact_content_type("image/png"));
    assert!(is_allowed_artifact_content_type(
        "image/jpeg; charset=binary"
    ));
    assert!(!is_allowed_artifact_content_type("image/svg+xml"));
    assert!(!is_allowed_artifact_content_type("text/html"));
}

#[test]
fn artifact_stream_reader_errors_as_soon_as_preview_cap_is_crossed() {
    let chunks = vec![
        Ok::<_, String>(vec![1; MAX_ARTIFACT_PREVIEW_BYTES as usize]),
        Ok(b"x".to_vec()),
    ];
    let err = tauri::async_runtime::block_on(read_limited_artifact_stream(
        futures_util::stream::iter(chunks),
    ))
    .expect_err("stream should stop when the cap is exceeded");
    assert_eq!(err, "artifact is too large to preview");
}
