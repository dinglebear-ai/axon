use super::*;

#[test]
fn ignores_removed_dynamic_job_templates() {
    assert_eq!(
        normalize_android_route("/v1/${kind.path}/${encodePathSegment(id)}/cancel"),
        Vec::<String>::new()
    );
    assert_eq!(
        normalize_android_route("/v1/{kind}/{id}"),
        Vec::<String>::new()
    );
}

#[test]
fn route_sources_include_generated_adapter() {
    assert!(
        ANDROID_ROUTE_SOURCES
            .contains(&"apps/android/app/src/main/java/com/axon/app/core/api/GeneratedAxonApi.kt")
    );
}

#[test]
fn strips_comment_only_routes_without_losing_real_routes() {
    let content = r#"
        // openApiRoute("GET", "/v1/comment-only")
        val route = openApiRoute("GET", "/v1/real-route")
        /*
         * openApiRoute("POST", "/v1/block-comment")
         */
    "#;

    let stripped = strip_kotlin_comments_preserving_offsets(content);

    assert!(!stripped.contains("/v1/comment-only"));
    assert!(!stripped.contains("/v1/block-comment"));
    assert!(stripped.contains("/v1/real-route"));
    assert_eq!(stripped.len(), content.len());
}

#[test]
fn parses_only_explicit_openapi_route_markers() {
    let content = r#"
        post("/v1/unmarked", request)
        openApiRoute("GET", "/v1/sources", "/v1/sources?limit=25")
        openApiRoute("POST", "/v1/{kind}/{id}/cancel", "/v1/${kind.path}/${encodePathSegment(id)}/cancel")
    "#;

    let routes = android_routes_from_content(content).expect("routes");

    assert!(!routes.contains(&Route {
        method: "POST".to_string(),
        path: "/v1/unmarked".to_string(),
    }));
    assert!(routes.contains(&Route {
        method: "GET".to_string(),
        path: "/v1/sources".to_string(),
    }));
    assert_eq!(routes.len(), 1);
}

#[test]
fn normalizes_query_and_path_segments() {
    assert_eq!(
        normalize_android_route("/v1/mobile/sessions/${encodePathSegment(session.id)}?x=1"),
        vec!["/v1/mobile/sessions/{id}"]
    );
}

#[test]
fn collections_route_is_not_public() {
    let openapi_operations = BTreeMap::from([(
        Route {
            method: "GET".to_string(),
            path: "/v1/collections".to_string(),
        },
        serde_json::json!({ "security": [{ "bearerAuth": [] }] }),
    )]);
    let android_routes = BTreeSet::from([Route {
        method: "GET".to_string(),
        path: "/v1/collections".to_string(),
    }]);
    assert!(check_routes(&openapi_operations, &android_routes).is_ok());
}

#[test]
fn route_check_requires_matching_method() {
    let openapi_operations = BTreeMap::from([(
        Route {
            method: "GET".to_string(),
            path: "/v1/collections".to_string(),
        },
        serde_json::json!({ "security": [{ "bearerAuth": [] }] }),
    )]);
    let android_routes = BTreeSet::from([Route {
        method: "POST".to_string(),
        path: "/v1/collections".to_string(),
    }]);
    assert!(check_routes(&openapi_operations, &android_routes).is_err());
}

#[test]
fn collections_route_requires_security_metadata() {
    let secure = serde_json::json!({
        "paths": {
            "/v1/collections": {
                "get": {
                    "security": [{ "bearerAuth": [] }]
                }
            }
        }
    });
    let public = serde_json::json!({
        "paths": {
            "/v1/collections": {
                "get": {}
            }
        }
    });
    let android_routes = BTreeSet::from([Route {
        method: "GET".to_string(),
        path: "/v1/collections".to_string(),
    }]);

    let secure_routes = openapi_routes_from_value(&secure).expect("secure routes");
    assert!(check_route_security(&secure_routes.operations, &android_routes).is_ok());

    let public_routes = openapi_routes_from_value(&public).expect("public routes");
    assert!(check_route_security(&public_routes.operations, &android_routes).is_err());
}
