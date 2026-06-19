use anyhow::{Context, Result, bail};
use regex::Regex;
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::Path;

const ANDROID_ROUTE_SOURCES: &[&str] = &[
    "apps/android/app/src/main/java/com/axon/app/data/remote/AxonClient.kt",
    "apps/android/app/src/main/java/com/axon/app/ui/operations/OperationMode.kt",
];

const JOB_KINDS: &[&str] = &["crawl", "embed", "extract", "ingest"];

pub fn check(root: &Path) -> Result<()> {
    let openapi_paths = openapi_paths(root)?;
    let android_routes = android_routes(root)?;
    check_routes(&openapi_paths, &android_routes)
}

pub fn check_against_openapi(root: &Path) -> Result<()> {
    check(root)
}

fn openapi_paths(root: &Path) -> Result<BTreeSet<String>> {
    let path = root.join("apps/web/openapi/axon.json");
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let parsed: Value = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    let paths = parsed
        .get("paths")
        .and_then(Value::as_object)
        .context("apps/web/openapi/axon.json is missing object field `paths`")?;
    Ok(paths.keys().cloned().collect())
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
    fn normalizes_query_and_path_segments() {
        assert_eq!(
            normalize_android_route("/v1/mobile/sessions/${encodePathSegment(session.id)}?x=1"),
            vec!["/v1/mobile/sessions/{id}"]
        );
    }
}
