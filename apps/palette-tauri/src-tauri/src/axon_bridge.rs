use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::{merged_settings, normalize_server_url};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AxonHttpRequest {
    #[serde(default, rename = "baseUrl")]
    _base_url: Option<String>,
    #[serde(default, rename = "token")]
    _token: Option<String>,
    method: HttpMethod,
    path: String,
    body: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
enum HttpMethod {
    Get,
    Post,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AxonHttpResult {
    ok: bool,
    status: u16,
    path: String,
    method: HttpMethod,
    payload: serde_json::Value,
}

#[tauri::command]
pub(crate) async fn axon_http_request(
    app: AppHandle,
    request: AxonHttpRequest,
) -> Result<AxonHttpResult, String> {
    let path = validate_axon_route(&request)?.to_string();
    let method = request.method;
    let settings = merged_settings(&app)?;
    let base_url = validate_saved_server_url(&settings.server_url)?;
    let url = format!("{}{}", base_url.trim_end_matches('/'), path);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(300))
        .user_agent("Axon Palette/4.5")
        .build()
        .map_err(|err| err.to_string())?;

    let mut builder = match method {
        HttpMethod::Get => client.get(&url),
        HttpMethod::Post => client.post(&url),
    }
    .header(
        reqwest::header::ACCEPT,
        "application/json, text/plain;q=0.9, */*;q=0.5",
    );

    if let Some(token) = settings
        .token
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
    {
        builder = builder.bearer_auth(token).header("x-api-key", token);
    }
    if let Some(body) = request.body {
        builder = builder.json(&body);
    }

    let response = builder.send().await.map_err(|err| err.to_string())?;
    let status = response.status();
    let text = response.text().await.map_err(|err| err.to_string())?;
    let payload = if text.trim().is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text))
    };

    Ok(AxonHttpResult {
        ok: status.is_success(),
        status: status.as_u16(),
        path,
        method,
        payload,
    })
}

fn validate_axon_route(request: &AxonHttpRequest) -> Result<&str, String> {
    let path = request.path.trim();
    if path != request.path || !path.starts_with("/v1/") {
        return Err("request path is not an allowed Axon API route".to_string());
    }
    if path.contains("://")
        || path.starts_with("//")
        || path.contains(['\\', '?', '#'])
        || path
            .split('/')
            .any(|segment| segment == "." || segment == "..")
    {
        return Err("request path must be a canonical /v1 route path".to_string());
    }
    if request.method == HttpMethod::Get && request.body.is_some() {
        return Err("GET requests cannot include a body".to_string());
    }
    is_allowed_route(request.method, path)
        .then_some(path)
        .ok_or_else(|| "request method and path are not allowed".to_string())
}

fn is_allowed_route(method: HttpMethod, path: &str) -> bool {
    matches!(
        (method, path),
        (
            HttpMethod::Get,
            "/v1/doctor" | "/v1/status" | "/v1/sources" | "/v1/domains" | "/v1/stats"
        ) | (
            HttpMethod::Post,
            "/v1/scrape"
                | "/v1/crawl"
                | "/v1/map"
                | "/v1/summarize"
                | "/v1/ask"
                | "/v1/query"
                | "/v1/retrieve"
                | "/v1/suggest"
                | "/v1/evaluate"
                | "/v1/search"
                | "/v1/research"
                | "/v1/embed"
                | "/v1/extract"
                | "/v1/ingest"
        )
    )
}

fn validate_saved_server_url(server_url: &str) -> Result<String, String> {
    let server_url = normalize_server_url(server_url);
    let parsed =
        reqwest::Url::parse(&server_url).map_err(|_| "saved Axon server URL is invalid")?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("saved Axon server URL must use http or https".to_string());
    }
    if parsed.host_str().is_none()
        || !matches!(parsed.path(), "" | "/")
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err("saved Axon server URL must be an origin URL".to_string());
    }
    Ok(server_url.trim_end_matches('/').to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

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
    }

    #[test]
    fn rejects_get_request_bodies() {
        let mut req = request(HttpMethod::Get, "/v1/doctor");
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
            validate_saved_server_url("axon.example.com/").unwrap(),
            "https://axon.example.com"
        );
        assert!(validate_saved_server_url("file:///tmp/axon.sock").is_err());
        assert!(validate_saved_server_url("https://axon.example.com/api").is_err());
        assert!(validate_saved_server_url("https://axon.example.com?token=leak").is_err());
    }
}
