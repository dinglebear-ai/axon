use anyhow::{Context, Result, bail};
use regex::Regex;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

const ANDROID_ROUTE_SOURCES: &[&str] = &[
    "apps/android/app/src/main/java/com/axon/app/data/remote/AxonClient.kt",
    "apps/android/app/src/main/java/com/axon/app/data/remote/GeneratedAxonApi.kt",
    "apps/android/app/src/main/java/com/axon/app/ui/operations/OperationMode.kt",
];

const JOB_KINDS: &[&str] = &["crawl", "embed", "extract", "ingest"];

pub fn check(root: &Path) -> Result<()> {
    let openapi_routes = openapi_routes(root)?;
    let android_routes = android_routes(root)?;
    check_routes(&openapi_routes.paths, &android_routes)?;
    check_route_security(&openapi_routes.operations, &android_routes)
}

pub fn check_against_openapi(root: &Path) -> Result<()> {
    check(root)
}

struct OpenApiRoutes {
    paths: BTreeSet<String>,
    operations: BTreeMap<String, Vec<Value>>,
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
    let mut route_paths = BTreeSet::new();
    let mut operations = BTreeMap::new();

    for (path, item) in paths {
        route_paths.insert(path.clone());
        let item = item
            .as_object()
            .with_context(|| format!("OpenAPI path item for {path} is not an object"))?;
        let route_operations = item
            .iter()
            .filter(|(method, _)| is_openapi_method(method))
            .map(|(_, operation)| operation.clone())
            .collect::<Vec<_>>();
        operations.insert(path.clone(), route_operations);
    }

    Ok(OpenApiRoutes {
        paths: route_paths,
        operations,
    })
}

fn is_openapi_method(method: &str) -> bool {
    matches!(
        method,
        "get" | "put" | "post" | "delete" | "options" | "head" | "patch" | "trace"
    )
}

fn android_routes(root: &Path) -> Result<BTreeSet<String>> {
    let route_pattern = Regex::new(r#"/v1[^"'\s]*"#).context("valid Android route regex")?;
    let mut routes = BTreeSet::new();

    for relative in ANDROID_ROUTE_SOURCES {
        let path = root.join(relative);
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        for found in route_pattern.find_iter(&content) {
            for route in normalize_android_route(found.as_str()) {
                routes.insert(route);
            }
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
    if path.contains("{kind}") {
        return Vec::new();
    }
    let path = path
        .replace("${encodePathSegment(jobId)}", "{id}")
        .replace("${encodePathSegment(id)}", "{id}")
        .replace("${encodePathSegment(session.id)}", "{id}");

    if path.contains("${kind.path}") {
        return JOB_KINDS
            .iter()
            .map(|kind| path.replace("${kind.path}", kind))
            .collect();
    }

    vec![path]
}

fn check_routes(openapi_paths: &BTreeSet<String>, android_routes: &BTreeSet<String>) -> Result<()> {
    let missing = android_routes
        .iter()
        .filter(|route| !openapi_paths.contains(*route))
        .cloned()
        .collect::<Vec<_>>();

    if missing.is_empty() {
        println!("OK: Android /v1 client routes are covered by OpenAPI.");
        return Ok(());
    }

    eprintln!("ERROR: Android calls /v1 route(s) missing from OpenAPI:");
    for route in &missing {
        eprintln!("  {route}");
    }
    bail!("Android API route contract drift");
}

fn check_route_security(
    openapi_operations: &BTreeMap<String, Vec<Value>>,
    android_routes: &BTreeSet<String>,
) -> Result<()> {
    let missing_security = android_routes
        .iter()
        .filter(|route| route.starts_with("/v1/"))
        .filter(|route| route.as_str() != "/healthz" && route.as_str() != "/readyz")
        .filter(|route| {
            openapi_operations
                .get(*route)
                .map(|operations| {
                    operations.is_empty()
                        || operations
                            .iter()
                            .any(|operation| operation.get("security").is_none())
                })
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
        eprintln!("  {route}");
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
    }

    #[test]
    fn route_sources_include_generated_adapter() {
        assert!(ANDROID_ROUTE_SOURCES.contains(
            &"apps/android/app/src/main/java/com/axon/app/data/remote/GeneratedAxonApi.kt"
        ));
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
        let openapi_paths = BTreeSet::from(["/v1/collections".to_string()]);
        let android_routes = BTreeSet::from(["/v1/collections".to_string()]);
        assert!(check_routes(&openapi_paths, &android_routes).is_ok());
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
        let android_routes = BTreeSet::from(["/v1/collections".to_string()]);

        let secure_routes = openapi_routes_from_value(&secure).expect("secure routes");
        assert!(check_route_security(&secure_routes.operations, &android_routes).is_ok());

        let public_routes = openapi_routes_from_value(&public).expect("public routes");
        assert!(check_route_security(&public_routes.operations, &android_routes).is_err());
    }
}
