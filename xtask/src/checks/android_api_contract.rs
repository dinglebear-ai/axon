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

pub fn check(root: &Path) -> Result<()> {
    let openapi_routes = openapi_routes(root)?;
    let android_routes = android_routes(root)?;
    check_routes(&openapi_routes.operations, &android_routes)?;
    check_route_security(&openapi_routes.operations, &android_routes)
}

#[cfg(feature = "openapi-drift")]
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
        return Vec::new();
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
#[path = "android_api_contract_tests.rs"]
mod tests;
