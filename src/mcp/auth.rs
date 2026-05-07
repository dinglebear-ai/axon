//! HTTP authentication middleware for the MCP HTTP server.
//!
//! Guards `axon mcp --transport http` (and `--transport both`) endpoints.
//! Set `AXON_MCP_HTTP_TOKEN` to enable token-based auth. If unset, loopback
//! MCP HTTP binds are allowed with a warning; non-loopback binds are rejected
//! at startup by the server policy.
//!
//! Token resolution order (first non-empty value wins):
//! 1. `Authorization: Bearer <token>` request header
//! 2. `x-api-key: <token>` request header

use axum::{
    body::Body,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use subtle::ConstantTimeEq;

/// Constant-time byte comparison to prevent timing attacks on API token checks.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.ct_eq(b).into()
}

/// Extract the bearer or x-api-key token from request headers.
fn extract_token(req: &Request<Body>) -> Option<&str> {
    req.headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .and_then(|v| {
            let (scheme, token) = v.split_once(' ')?;
            scheme
                .eq_ignore_ascii_case("Bearer")
                .then_some(token.trim())
                .filter(|s| !s.is_empty())
        })
        .or_else(|| {
            req.headers()
                .get("x-api-key")
                .and_then(|v| v.to_str().ok())
                .map(str::trim)
                .filter(|s| !s.is_empty())
        })
}

fn configured_mcp_http_token() -> Option<String> {
    let configured_token = std::env::var("AXON_MCP_HTTP_TOKEN").ok();
    configured_token
        .map(|value| value.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub(crate) fn mcp_http_token_is_configured() -> bool {
    configured_mcp_http_token().is_some()
}

fn authorize_mcp_http_request_with_token(
    request: &Request<Body>,
    configured_token: Option<&str>,
) -> Result<(), StatusCode> {
    match configured_token.map(str::trim).filter(|s| !s.is_empty()) {
        Some(expected) => {
            let provided = extract_token(request).unwrap_or("").trim();
            if provided.is_empty() {
                return Err(StatusCode::UNAUTHORIZED);
            }
            if !constant_time_eq(provided.as_bytes(), expected.as_bytes()) {
                return Err(StatusCode::UNAUTHORIZED);
            }
            Ok(())
        }
        None => {
            static WARNED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
            WARNED.get_or_init(|| {
                tracing::warn!(
                    context = "mcp_auth",
                    "AXON_MCP_HTTP_TOKEN not set \u{2014} MCP HTTP server is unauthenticated"
                );
            });
            Ok(())
        }
    }
}

fn authorize_mcp_http_request(request: &Request<Body>) -> Result<(), StatusCode> {
    let configured_token = configured_mcp_http_token();
    authorize_mcp_http_request_with_token(request, configured_token.as_deref())
}

/// Axum middleware that enforces `AXON_MCP_HTTP_TOKEN` on all MCP HTTP requests.
///
/// - Token set: 401 for missing or mismatched token.
/// - Token NOT set: warn once and allow through. Server startup policy rejects
///   tokenless non-loopback binds before this middleware can serve requests.
pub async fn mcp_auth_middleware(request: Request<Body>, next: Next) -> Response {
    match authorize_mcp_http_request(&request) {
        Ok(()) => next.run(request).await,
        Err(status) => (status, "unauthorized").into_response(),
    }
}

#[cfg(test)]
async fn mcp_auth_middleware_with_configured_token(
    axum::extract::State(configured_token): axum::extract::State<Option<String>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    match authorize_mcp_http_request_with_token(&request, configured_token.as_deref()) {
        Ok(()) => next.run(request).await,
        Err(status) => (status, "unauthorized").into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, middleware, routing::get};
    use tokio::sync::oneshot;

    fn request_with_header(name: &'static str, value: &'static str) -> Request<Body> {
        Request::builder()
            .uri("/mcp")
            .header(name, value)
            .body(Body::empty())
            .expect("request")
    }

    fn request_without_token() -> Request<Body> {
        Request::builder()
            .uri("/mcp")
            .body(Body::empty())
            .expect("request")
    }

    #[test]
    fn token_middleware_rejects_missing_token_when_configured() {
        let request = request_without_token();

        let result = authorize_mcp_http_request_with_token(&request, Some("secret"));

        assert_eq!(result, Err(StatusCode::UNAUTHORIZED));
    }

    #[test]
    fn token_middleware_rejects_invalid_bearer_token() {
        let request = request_with_header("authorization", "Bearer wrong");

        let result = authorize_mcp_http_request_with_token(&request, Some("secret"));

        assert_eq!(result, Err(StatusCode::UNAUTHORIZED));
    }

    #[test]
    fn token_middleware_accepts_valid_bearer_token() {
        let request = request_with_header("authorization", "Bearer secret");

        let result = authorize_mcp_http_request_with_token(&request, Some("secret"));

        assert_eq!(result, Ok(()));
    }

    #[test]
    fn token_middleware_accepts_valid_x_api_key_token() {
        let request = request_with_header("x-api-key", "secret");

        let result = authorize_mcp_http_request_with_token(&request, Some("secret"));

        assert_eq!(result, Ok(()));
    }

    #[test]
    fn tokenless_requests_are_allowed_when_token_is_unset() {
        let request = request_without_token();

        let result = authorize_mcp_http_request_with_token(&request, None);

        assert_eq!(result, Ok(()));
    }

    async fn ok_handler() -> &'static str {
        "ok"
    }

    async fn run_test_server_with_token(
        token: Option<&str>,
    ) -> (String, oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
        let configured_token = token.map(ToOwned::to_owned);
        let app =
            Router::new()
                .route("/mcp", get(ok_handler))
                .layer(middleware::from_fn_with_state(
                    configured_token,
                    mcp_auth_middleware_with_configured_token,
                ));
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind test listener");
        let addr = listener.local_addr().expect("local addr");
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let handle = tokio::spawn(async move {
            let result = axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = shutdown_rx.await;
                })
                .await;
            result.expect("test server");
        });

        (format!("http://{addr}/mcp"), shutdown_tx, handle)
    }

    async fn get_status(url: &str, header: Option<(&str, &str)>) -> StatusCode {
        let client = reqwest::Client::new();
        let mut request = client.get(url);
        if let Some((name, value)) = header {
            request = request.header(name, value);
        }
        let response = request.send().await.expect("send request");
        response.status()
    }

    #[tokio::test]
    async fn middleware_http_rejects_missing_token_when_configured() {
        let (url, shutdown, handle) = run_test_server_with_token(Some("secret")).await;

        let status = get_status(&url, None).await;

        let _ = shutdown.send(());
        handle.await.expect("server task");
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn middleware_http_rejects_invalid_bearer_token() {
        let (url, shutdown, handle) = run_test_server_with_token(Some("secret")).await;

        let status = get_status(&url, Some(("authorization", "Bearer wrong"))).await;

        let _ = shutdown.send(());
        handle.await.expect("server task");
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn middleware_http_accepts_valid_bearer_token() {
        let (url, shutdown, handle) = run_test_server_with_token(Some("secret")).await;

        let status = get_status(&url, Some(("authorization", "Bearer secret"))).await;

        let _ = shutdown.send(());
        handle.await.expect("server task");
        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn middleware_http_accepts_valid_x_api_key_token() {
        let (url, shutdown, handle) = run_test_server_with_token(Some("secret")).await;

        let status = get_status(&url, Some(("x-api-key", "secret"))).await;

        let _ = shutdown.send(());
        handle.await.expect("server task");
        assert_eq!(status, StatusCode::OK);
    }
}
