use anyhow::{Context, Result, bail};
use regex::Regex;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

const ANDROID_ROUTE_SOURCES: &[&str] = &[
    "apps/android/app/src/main/java/com/axon/app/core/api/AxonClient.kt",
    "apps/android/app/src/main/java/com/axon/app/core/api/AxonClientMemory.kt",
    "apps/android/app/src/main/java/com/axon/app/core/api/AxonClientPanel.kt",
    "apps/android/app/src/main/java/com/axon/app/core/api/AxonClientStreaming.kt",
    "apps/android/app/src/main/java/com/axon/app/core/api/GeneratedAxonApi.kt",
    "apps/android/app/src/main/java/com/axon/app/ui/operations/OperationMode.kt",
];

const JOB_KINDS: &[&str] = &["crawl", "embed", "extract", "ingest"];

pub fn check(root: &Path) -> Result<()> {
    let openapi_routes = openapi_routes(root)?;
    let android_routes = android_routes(root)?;
    check_routes(&openapi_routes.operations, &android_routes)?;
    check_route_security(&openapi_routes.operations, &android_routes)
}

pub fn check_against_openapi(root: &Path) -> Result<()> {
    check(root)
}

struct OpenApiRoutes {
    operations: BTreeMap<Route, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Route {
    method: String,
    path: String,
}

fn openapi_routes(root: &Path) -> Result<OpenApiRoutes> {
    let path = root.join("apps/web/openapi/axon.json");
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let parsed: Value = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    openapi_routes_from_value(&parsed)
}

fn openapi_routes_from_value(parsed: &Value) -> Result<OpenApiRoutes> {
    let paths = parsed
        .get("paths")
        .and_then(Value::as_object)
        .context("apps/web/openapi/axon.json is missing object field `paths`")?;
    let mut operations = BTreeMap::new();

    for (path, item) in paths {
        let item = item
            .as_object()
            .with_context(|| format!("OpenAPI path item for {path} is not an object"))?;
        for (method, operation) in item.iter().filter(|(method, _)| is_openapi_method(method)) {
            operations.insert(
                Route {
                    method: method.to_uppercase(),
                    path: path.clone(),
                },
                operation.clone(),
            );
        }
    }

    Ok(OpenApiRoutes { operations })
}

fn is_openapi_method(method: &str) -> bool {
    matches!(
        method,
        "get" | "put" | "post" | "delete" | "options" | "head" | "patch" | "trace"
    )
}

fn android_routes(root: &Path) -> Result<BTreeSet<Route>> {
    let mut routes = BTreeSet::new();

    for relative in ANDROID_ROUTE_SOURCES {
        let path = root.join(relative);
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        routes.extend(android_routes_from_content(&content)?);
    }

    Ok(routes)
}

fn android_routes_from_content(content: &str) -> Result<BTreeSet<Route>> {
    let route_pattern = Regex::new(
        r#"openApiRoute\(\s*"(?P<method>GET|POST|PUT|DELETE|PATCH)"\s*,\s*"(?P<path>/v1[^"]*)""#,
    )
    .context("valid Android explicit route regex")?;
    let mut routes = BTreeSet::new();
    let content_without_comments = strip_kotlin_comments_preserving_offsets(content);

    for found in route_pattern.captures_iter(&content_without_comments) {
        let method = found
            .name("method")
            .expect("route regex captures method")
            .as_str();
        let path = found
            .name("path")
            .expect("route regex captures path")
            .as_str();
        for path in normalize_android_route(path) {
            routes.insert(Route {
                method: method.to_string(),
                path,
            });
        }
    }

    Ok(routes)
}

fn normalize_android_route(raw: &str) -> Vec<String> {
    let path = raw
        .split('?')
        .next()
        .unwrap_or(raw)
        .trim_end_matches(['.', ',', ';', ':']);
    let path = path
        .replace("${encodePathSegment(jobId)}", "{id}")
        .replace("${encodePathSegment(id)}", "{id}")
        .replace("${encodePathSegment(session.id)}", "{id}");

    if path.contains("${kind.path}") || path.contains("{kind}") {
        return JOB_KINDS
            .iter()
            .map(|kind| path.replace("${kind.path}", kind).replace("{kind}", kind))
            .collect();
    }

    vec![path]
}

fn strip_kotlin_comments_preserving_offsets(content: &str) -> String {
    let bytes = content.as_bytes();
    let mut output = bytes.to_vec();
    let mut index = 0;
    let mut in_string = false;
    let mut escaped = false;

    while index < bytes.len() {
        let current = bytes[index];

        if in_string {
            if escaped {
                escaped = false;
            } else if current == b'\\' {
                escaped = true;
            } else if current == b'"' {
                in_string = false;
            }
            index += 1;
            continue;
        }

        if current == b'"' {
            in_string = true;
            index += 1;
            continue;
        }

        if current == b'/' && bytes.get(index + 1) == Some(&b'/') {
            output[index] = b' ';
            output[index + 1] = b' ';
            index += 2;
            while index < bytes.len() && bytes[index] != b'\n' {
                output[index] = b' ';
                index += 1;
            }
            continue;
        }

        if current == b'/' && bytes.get(index + 1) == Some(&b'*') {
            output[index] = b' ';
            output[index + 1] = b' ';
            index += 2;
            while index < bytes.len() {
                if bytes[index] == b'*' && bytes.get(index + 1) == Some(&b'/') {
                    output[index] = b' ';
                    output[index + 1] = b' ';
                    index += 2;
                    break;
                }
                if bytes[index] != b'\n' {
                    output[index] = b' ';
                }
                index += 1;
            }
            continue;
        }

        index += 1;
    }

    String::from_utf8(output).expect("comment stripping preserves utf-8")
}

fn check_routes(
    openapi_operations: &BTreeMap<Route, Value>,
    android_routes: &BTreeSet<Route>,
) -> Result<()> {
    let missing = android_routes
        .iter()
        .filter(|route| !openapi_operations.contains_key(*route))
        .cloned()
        .collect::<Vec<_>>();

    if missing.is_empty() {
        println!("OK: Android /v1 client routes are covered by OpenAPI.");
        return Ok(());
    }

    eprintln!("ERROR: Android calls /v1 route(s) missing from OpenAPI:");
    for route in &missing {
        eprintln!("  {} {}", route.method, route.path);
    }
    bail!("Android API route contract drift");
}

fn check_route_security(
    openapi_operations: &BTreeMap<Route, Value>,
    android_routes: &BTreeSet<Route>,
) -> Result<()> {
    let missing_security = android_routes
        .iter()
        .filter(|route| route.path.starts_with("/v1/"))
        .filter(|route| {
            openapi_operations
                .get(*route)
                .map(|operation| operation.get("security").is_none())
                .unwrap_or(true)
        })
        .cloned()
        .collect::<Vec<_>>();

    if missing_security.is_empty() {
        println!("OK: Android /v1 client routes require OpenAPI security metadata.");
        return Ok(());
    }

    eprintln!("ERROR: Android calls /v1 route(s) missing OpenAPI security metadata:");
    for route in &missing_security {
        eprintln!("  {} {}", route.method, route.path);
    }
    bail!("Android API security contract drift");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_dynamic_job_routes() {
        assert_eq!(
            normalize_android_route("/v1/${kind.path}/${encodePathSegment(id)}/cancel"),
            vec![
                "/v1/crawl/{id}/cancel",
                "/v1/embed/{id}/cancel",
                "/v1/extract/{id}/cancel",
                "/v1/ingest/{id}/cancel",
            ]
        );
        assert_eq!(
            normalize_android_route("/v1/{kind}/{id}"),
            vec![
                "/v1/crawl/{id}",
                "/v1/embed/{id}",
                "/v1/extract/{id}",
                "/v1/ingest/{id}",
            ]
        );
    }

    #[test]
    fn route_sources_include_generated_adapter() {
        assert!(
            ANDROID_ROUTE_SOURCES.contains(
                &"apps/android/app/src/main/java/com/axon/app/core/api/GeneratedAxonApi.kt"
            )
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
        assert!(routes.contains(&Route {
            method: "POST".to_string(),
            path: "/v1/crawl/{id}/cancel".to_string(),
        }));
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
}
