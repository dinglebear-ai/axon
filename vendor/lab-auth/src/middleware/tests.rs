use super::*;
use axum::Router;
use axum::http::{Request as HttpRequest, StatusCode};
use axum::routing::get;
use tower::ServiceExt;

use crate::authorize::tests::{test_auth_config, test_auth_state, test_auth_state_with_config};

fn echo_app(layer: AuthLayer) -> Router {
    Router::new()
        .route("/probe", get(|| async { "ok" }))
        .route_layer(layer)
}

#[test]
fn parse_bearer_token_accepts_valid_header() {
    assert_eq!(
        parse_bearer_token("Bearer abc.def").as_deref(),
        Some("abc.def")
    );
    assert_eq!(
        parse_bearer_token("bearer abc.def").as_deref(),
        Some("abc.def")
    );
}

#[test]
fn parse_bearer_token_rejects_malformed() {
    assert_eq!(parse_bearer_token("Basic abc.def"), None);
    assert_eq!(parse_bearer_token("Bearer"), None);
    assert_eq!(parse_bearer_token("Bearer one two"), None);
    assert_eq!(parse_bearer_token(""), None);
}

#[test]
fn tokens_equal_distinguishes_unequal_strings() {
    assert!(tokens_equal("abc", "abc"));
    assert!(!tokens_equal("abc", "abd"));
    assert!(!tokens_equal("abc", "abcd"));
}

#[tokio::test(flavor = "current_thread")]
async fn missing_bearer_token_returns_401_with_www_authenticate() {
    let layer =
        AuthLayer::new().with_resource_url(Some(Arc::<str>::from("https://lab.example.com")));
    let app = echo_app(layer);
    let response = app
        .oneshot(
            HttpRequest::builder()
                .uri("/probe")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let www = response
        .headers()
        .get(header::WWW_AUTHENTICATE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    assert!(
        www.contains("resource_metadata="),
        "missing resource_metadata in WWW-Authenticate: `{www}`"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn static_bearer_match_grants_configured_scopes() {
    let token: Arc<str> = Arc::<str>::from("super-secret");
    let layer = AuthLayer::new()
        .with_static_token(Some(token.clone()))
        .with_static_token_scopes(vec!["syslog:read".to_string(), "syslog:admin".to_string()]);
    let app =
        Router::new()
            .route(
                "/probe",
                get(
                    |axum::Extension(ctx): axum::Extension<AuthContext>| async move {
                        ctx.scopes.join(",")
                    },
                ),
            )
            .route_layer(layer);

    let response = app
        .oneshot(
            HttpRequest::builder()
                .uri("/probe")
                .header(header::AUTHORIZATION, "Bearer super-secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    assert_eq!(&body[..], b"syslog:read,syslog:admin");
}

#[tokio::test(flavor = "current_thread")]
async fn wrong_static_bearer_rejected() {
    let layer = AuthLayer::new().with_static_token(Some(Arc::<str>::from("super-secret")));
    let app = echo_app(layer);

    let response = app
        .oneshot(
            HttpRequest::builder()
                .uri("/probe")
                .header(header::AUTHORIZATION, "Bearer wrong")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test(flavor = "current_thread")]
async fn jwt_validation_path_accepts_signed_token_and_writes_context() {
    let state = Arc::new(test_auth_state().await);
    let aud = canonical_resource_url(&state);
    let iss = state
        .config
        .public_url
        .as_ref()
        .map(|url| url.as_str().trim_end_matches('/').to_string())
        .unwrap();
    let claims = crate::jwt::AccessClaims {
        iss: iss.clone(),
        sub: "user@example.com".to_string(),
        aud: aud.clone(),
        exp: (crate::util::now_unix() + 60) as usize,
        iat: crate::util::now_unix() as usize,
        jti: "j-1".to_string(),
        scope: "syslog:read syslog:admin".to_string(),
        azp: String::new(),
    };
    let token = state.signing_keys.issue_access_token(&claims).unwrap();
    let layer = AuthLayer::from_state(state);
    let app = Router::new()
        .route(
            "/probe",
            get(
                |axum::Extension(ctx): axum::Extension<AuthContext>| async move {
                    format!("{}|{}", ctx.sub, ctx.scopes.join(","))
                },
            ),
        )
        .route_layer(layer);

    let response = app
        .oneshot(
            HttpRequest::builder()
                .uri("/probe")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .unwrap();
    assert_eq!(&body[..], b"user@example.com|syslog:read,syslog:admin");
}

#[tokio::test(flavor = "current_thread")]
async fn jwt_with_wrong_issuer_rejected() {
    let state = Arc::new(test_auth_state().await);
    let aud = canonical_resource_url(&state);
    let claims = crate::jwt::AccessClaims {
        iss: "https://attacker.example.com".to_string(),
        sub: "user@example.com".to_string(),
        aud,
        exp: (crate::util::now_unix() + 60) as usize,
        iat: crate::util::now_unix() as usize,
        jti: "j-1".to_string(),
        scope: "syslog:read".to_string(),
        azp: String::new(),
    };
    let token = state.signing_keys.issue_access_token(&claims).unwrap();
    let layer = AuthLayer::from_state(state)
        .with_resource_url(Some(Arc::<str>::from("https://lab.example.com")));
    let app = echo_app(layer);

    let response = app
        .oneshot(
            HttpRequest::builder()
                .uri("/probe")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test(flavor = "current_thread")]
async fn jwt_with_wrong_audience_rejected() {
    let state = Arc::new(test_auth_state().await);
    let iss = state
        .config
        .public_url
        .as_ref()
        .map(|url| url.as_str().trim_end_matches('/').to_string())
        .unwrap();
    let claims = crate::jwt::AccessClaims {
        iss,
        sub: "user@example.com".to_string(),
        aud: "https://other.example.com/mcp".to_string(),
        exp: (crate::util::now_unix() + 60) as usize,
        iat: crate::util::now_unix() as usize,
        jti: "j-1".to_string(),
        scope: "syslog:read".to_string(),
        azp: String::new(),
    };
    let token = state.signing_keys.issue_access_token(&claims).unwrap();
    let layer = AuthLayer::from_state(state);
    let app = echo_app(layer);

    let response = app
        .oneshot(
            HttpRequest::builder()
                .uri("/probe")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test(flavor = "current_thread")]
async fn html_get_with_session_cookie_enabled_redirects_to_login_path() {
    let state = Arc::new(test_auth_state().await);
    let layer = AuthLayer::from_state(state)
        .with_allow_session_cookie(true)
        .with_login_path("/auth/login");
    let app = echo_app(layer);

    let response = app
        .oneshot(
            HttpRequest::builder()
                .uri("/probe?x=1")
                .header(header::ACCEPT, "text/html")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let location = response
        .headers()
        .get(header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    assert!(
        location.starts_with("/auth/login?return_to="),
        "unexpected redirect Location: `{location}`"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn html_get_uses_configured_login_path_override() {
    let state = Arc::new(test_auth_state().await);
    let layer = AuthLayer::from_state(state)
        .with_allow_session_cookie(true)
        .with_login_path("/syslog/auth/login");
    let app = echo_app(layer);

    let response = app
        .oneshot(
            HttpRequest::builder()
                .uri("/probe")
                .header(header::ACCEPT, "text/html")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let location = response
        .headers()
        .get(header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    assert!(
        location.starts_with("/syslog/auth/login?return_to="),
        "unexpected redirect Location: `{location}`"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn static_bearer_blocked_when_disable_static_token_with_oauth_is_set() {
    // When disable_static_token_with_oauth=true and mode=OAuth, the static
    // token must be rejected even though the token value matches.
    let mut config = test_auth_config();
    config.disable_static_token_with_oauth = true;
    let state = Arc::new(test_auth_state_with_config(config).await);

    let token: Arc<str> = Arc::from("super-secret");
    let layer = AuthLayer::from_state(state).with_static_token(Some(token.clone()));
    let app = echo_app(layer);

    let response = app
        .oneshot(
            HttpRequest::builder()
                .uri("/probe")
                .header(header::AUTHORIZATION, "Bearer super-secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    // Must be 401 — static token blocked because OAuth is active.
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test(flavor = "current_thread")]
async fn explicit_invalid_bearer_never_falls_back_to_browser_redirect() {
    let state = Arc::new(test_auth_state().await);
    let app = echo_app(AuthLayer::from_state(state).with_allow_session_cookie(true));
    let response = app
        .oneshot(
            HttpRequest::builder()
                .uri("/probe")
                .header(header::ACCEPT, "text/html")
                .header(header::AUTHORIZATION, "Bearer wrong")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(response.headers().get(header::LOCATION).is_none());
}
