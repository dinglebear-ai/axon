use super::legacy::authorize_mcp_http_request_with_token;
use super::*;
use axum::{Extension, Router, middleware, routing::get};
use lab_auth::AuthContext;
use tokio::sync::oneshot;

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

async fn scoped_handler(Extension(auth): Extension<AuthContext>) -> String {
    if auth.scopes.iter().any(|scope| scope == "axon:write") {
        "write".to_string()
    } else {
        "missing-write".to_string()
    }
}

async fn run_test_server_with_token(
    token: Option<&str>,
) -> (String, oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
    let configured_token = token.map(ToOwned::to_owned);
    let app = Router::new()
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

async fn run_production_auth_server(
    token: Option<&str>,
) -> (String, oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
    let mut app = Router::new().route("/mcp", get(scoped_handler));
    let policy = AuthPolicy::Mounted { auth_state: None };
    if let Some(layer) =
        build_auth_layer(&policy, token.map(Arc::from), oauth_resource_url(&policy))
    {
        app = app
            .layer(layer)
            .layer(middleware::from_fn(normalize_api_key_header));
    }
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

#[tokio::test]
async fn production_auth_layer_rejects_missing_and_invalid_static_tokens() {
    let (url, shutdown, handle) = run_production_auth_server(Some("secret")).await;

    let missing = get_status(&url, None).await;
    let invalid = get_status(&url, Some(("authorization", "Bearer wrong"))).await;

    let _ = shutdown.send(());
    handle.await.expect("server task");
    assert_eq!(missing, StatusCode::UNAUTHORIZED);
    assert_eq!(invalid, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn production_auth_layer_accepts_bearer_and_x_api_key_with_static_scopes() {
    let (url, shutdown, handle) = run_production_auth_server(Some("secret")).await;
    let client = reqwest::Client::new();

    let bearer = client
        .get(&url)
        .header("authorization", "Bearer secret")
        .send()
        .await
        .expect("bearer request");
    let bearer_status = bearer.status();
    let bearer_body = bearer.text().await.expect("bearer body");
    let api_key = client
        .get(&url)
        .header("x-api-key", "secret")
        .send()
        .await
        .expect("x-api-key request");
    let api_key_status = api_key.status();
    let api_key_body = api_key.text().await.expect("x-api-key body");

    let _ = shutdown.send(());
    handle.await.expect("server task");
    assert_eq!(bearer_status, StatusCode::OK);
    assert_eq!(bearer_body, "write");
    assert_eq!(api_key_status, StatusCode::OK);
    assert_eq!(api_key_body, "write");
}

#[test]
fn auth_policy_loopbackdev_debug_does_not_contain_secrets() {
    let policy = AuthPolicy::LoopbackDev;
    let debug = format!("{policy:?}");
    assert!(debug.contains("LoopbackDev"));
    assert!(!debug.contains("AuthState"));
}

#[test]
fn oauth_resource_url_only_emits_for_oauth_policy() {
    assert_eq!(
        oauth_resource_url_from_parts(false, Some("https://axon.example.com".to_string())),
        None
    );
    assert_eq!(oauth_resource_url_from_parts(true, None), None);
    assert_eq!(
        oauth_resource_url_from_parts(true, Some("https://axon.example.com/".to_string()))
            .as_deref(),
        Some("https://axon.example.com")
    );
}

#[test]
fn oauth_metadata_base_keeps_mcp_as_canonical_resource_audience() {
    let resource_metadata_base =
        oauth_resource_url_from_parts(true, Some("https://axon.example.com/".to_string()))
            .expect("metadata base");
    let auth_config = lab_auth::config::AuthConfigBuilder::new()
        .env_prefix("AXON_MCP")
        .scopes_supported(vec!["axon:read".into(), "axon:write".into()])
        .resource_path("/mcp")
        .default_scope("axon:read axon:write")
        .static_token_scopes(vec!["axon:read".into(), "axon:write".into()])
        .build_from_sources([
            ("AXON_MCP_AUTH_MODE".to_string(), "oauth".to_string()),
            (
                "AXON_MCP_PUBLIC_URL".to_string(),
                "https://axon.example.com".to_string(),
            ),
            (
                "AXON_MCP_GOOGLE_CLIENT_ID".to_string(),
                "client-id".to_string(),
            ),
            (
                "AXON_MCP_GOOGLE_CLIENT_SECRET".to_string(),
                "client-secret".to_string(),
            ),
            (
                "AXON_MCP_AUTH_ADMIN_EMAIL".to_string(),
                "admin@example.com".to_string(),
            ),
        ])
        .expect("auth config");

    assert_eq!(
        lab_auth::auth_context::www_authenticate_value(&resource_metadata_base),
        "Bearer resource_metadata=\"https://axon.example.com/.well-known/oauth-protected-resource\""
    );
    assert_eq!(
        auth_config.public_url.as_ref().map(url::Url::as_str),
        Some("https://axon.example.com/")
    );
    assert_eq!(auth_config.resource_path, "/mcp");
    let canonical_resource = format!(
        "{}{}",
        auth_config
            .public_url
            .as_ref()
            .expect("public url")
            .as_str()
            .trim_end_matches('/'),
        auth_config.resource_path
    );
    assert_eq!(canonical_resource, "https://axon.example.com/mcp");
}

#[test]
fn auth_policy_mounted_bearer_only_debug_is_informative() {
    let policy = AuthPolicy::Mounted { auth_state: None };
    let debug = format!("{policy:?}");
    assert!(debug.contains("bearer-only"));
}
