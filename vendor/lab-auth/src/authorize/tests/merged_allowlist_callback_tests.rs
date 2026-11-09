use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use base64::Engine;
use serde_json::json;
use sha2::{Digest, Sha256};
use tower::util::ServiceExt;
use url::Url;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::{
    signed_test_id_token, test_auth_config, test_auth_state_with_config,
    test_auth_state_with_mock_google, test_jwks,
};
use crate::google::GoogleProvider;
use crate::routes::router;
use crate::state::AuthState;
use crate::types::{
    AuthorizationRequestRow, BrowserLoginStateRow, RegisteredClient, TokenResponse,
};
use crate::util::now_unix;

fn pkce_challenge(code_verifier: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(Sha256::digest(code_verifier.as_bytes()))
}

/// Helper that mounts Google mock endpoints on a fresh server and builds
/// an `AuthState` with that mock, reusing an existing base state's store
/// and signing keys (so DB writes made to `base_state.store` are visible).
async fn state_with_mock_google_from(base_state: &AuthState) -> AuthState {
    let server = Box::leak(Box::new(MockServer::start().await));
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "google-access-token",
            "refresh_token": "refresh-token",
            "expires_in": 3600,
            "id_token": signed_test_id_token(),
        })))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/certs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(test_jwks()))
        .mount(server)
        .await;
    let google = GoogleProvider::new(
        "client-id".to_string(),
        "client-secret".to_string(),
        Url::parse("https://lab.example.com/auth/google/callback").unwrap(),
    )
    .unwrap()
    .with_endpoints(
        server.uri().parse::<Url>().unwrap(),
        server.uri().parse::<Url>().unwrap().join("/token").unwrap(),
    )
    .with_jwks_endpoint(server.uri().parse::<Url>().unwrap().join("/certs").unwrap());
    AuthState::for_tests(
        (*base_state.config).clone(),
        base_state.store.clone(),
        (*base_state.signing_keys).clone(),
        google,
    )
}

/// The mock id_token always returns `user@example.com`. When admin is set
/// to a *different* email and that address is added to `allowed_users`, the
/// browser-login callback must succeed (DB row authorises the login).
#[tokio::test]
async fn browser_login_succeeds_for_allowlisted_non_admin_email() {
    let mut config = test_auth_config();
    // Set admin to something other than the id_token email.
    config.admin_email = "admin@example.com".to_string();
    let base_state = test_auth_state_with_config(config).await;

    // Insert id_token email into allowed_users.
    base_state
        .store
        .add_allowed_user("user@example.com", "admin", now_unix())
        .await
        .unwrap();

    let state = state_with_mock_google_from(&base_state).await;

    // Seed the browser-login state row so the callback recognises the flow.
    state
        .store
        .insert_browser_login_state(BrowserLoginStateRow {
            state: "browser-state".to_string(),
            return_to: "/".to_string(),
            provider_code_verifier: "provider-verifier".to_string(),
            created_at: now_unix(),
            expires_at: now_unix() + 300,
        })
        .await
        .unwrap();

    let app = router(state);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/auth/google/callback?state=browser-state&code=upstream-code")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Successful browser login → redirect with a Set-Cookie header (session).
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert!(response.headers().contains_key(header::SET_COOKIE));
}

/// Admin email is always authorised even when the `allowed_users` table is
/// empty (browser-login branch).
#[tokio::test]
async fn browser_login_succeeds_for_admin_when_allowed_users_is_empty() {
    // Default test config sets admin_email = "user@example.com", which
    // matches the id_token returned by signed_test_id_token.
    let base_state = test_auth_state_with_mock_google().await;

    // Confirm no extra rows exist.
    assert!(
        base_state
            .store
            .list_allowed_users()
            .await
            .unwrap()
            .is_empty()
    );

    // Seed browser-login state.
    base_state
        .store
        .insert_browser_login_state(BrowserLoginStateRow {
            state: "browser-state-2".to_string(),
            return_to: "/".to_string(),
            provider_code_verifier: "provider-verifier".to_string(),
            created_at: now_unix(),
            expires_at: now_unix() + 300,
        })
        .await
        .unwrap();

    let app = router(base_state);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/auth/google/callback?state=browser-state-2&code=upstream-code")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert!(response.headers().contains_key(header::SET_COOKIE));
}

/// The oauth-client callback must also succeed for a non-admin email that
/// exists in `allowed_users`.
#[tokio::test]
async fn oauth_client_callback_succeeds_for_allowlisted_non_admin_email() {
    let mut config = test_auth_config();
    config.admin_email = "admin@example.com".to_string();
    let base_state = test_auth_state_with_config(config).await;

    // Register a client.
    base_state
        .store
        .register_client(RegisteredClient {
            client_id: "client".to_string(),
            redirect_uris: vec!["http://127.0.0.1:7777/callback".to_string()],
            created_at: now_unix(),
        })
        .await
        .unwrap();

    // Add id_token email to allowed_users.
    base_state
        .store
        .add_allowed_user("user@example.com", "admin", now_unix())
        .await
        .unwrap();

    let state = state_with_mock_google_from(&base_state).await;

    // Seed an authorization request row.
    state
        .store
        .insert_authorization_request(AuthorizationRequestRow {
            state: "oauth-state".to_string(),
            client_id: "client".to_string(),
            redirect_uri: "http://127.0.0.1:7777/callback".to_string(),
            client_state: "client-xyz".to_string(),
            scope: "lab".to_string(),
            provider_code_verifier: "provider-verifier".to_string(),
            code_challenge: "challenge".to_string(),
            code_challenge_method: "S256".to_string(),
            created_at: now_unix(),
            expires_at: now_unix() + 300,
        })
        .await
        .unwrap();

    let app = router(state);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/auth/google/callback?state=oauth-state&code=upstream-code")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Success: redirect to client callback with `code` param (no `error`).
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let location = response
        .headers()
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    let redirect = Url::parse(location).unwrap();
    let params: std::collections::HashMap<_, _> = redirect.query_pairs().collect();
    assert!(
        params.contains_key("code"),
        "expected code in redirect: {location}"
    );
    assert!(
        !params.contains_key("error"),
        "unexpected error in redirect: {location}"
    );
}

#[tokio::test]
async fn oauth_client_admin_email_gets_full_scope_even_when_client_requests_read_only() {
    let mut config = test_auth_config();
    config.admin_email = "user@example.com".to_string();
    config.scopes_supported = vec!["axon:read".to_string(), "axon:write".to_string()];
    config.default_scope = "axon:read axon:write".to_string();
    let base_state = test_auth_state_with_config(config).await;

    base_state
        .store
        .register_client(RegisteredClient {
            client_id: "client".to_string(),
            redirect_uris: vec!["http://127.0.0.1:7777/callback".to_string()],
            created_at: now_unix(),
        })
        .await
        .unwrap();

    let state = state_with_mock_google_from(&base_state).await;
    state
        .store
        .insert_authorization_request(AuthorizationRequestRow {
            state: "admin-read-only-state".to_string(),
            client_id: "client".to_string(),
            redirect_uri: "http://127.0.0.1:7777/callback".to_string(),
            client_state: "client-admin".to_string(),
            scope: "axon:read".to_string(),
            provider_code_verifier: "provider-verifier".to_string(),
            code_challenge: pkce_challenge("verifier"),
            code_challenge_method: "S256".to_string(),
            created_at: now_unix(),
            expires_at: now_unix() + 300,
        })
        .await
        .unwrap();

    let app = router(state);
    let callback = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/auth/google/callback?state=admin-read-only-state&code=upstream-code")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(callback.status(), StatusCode::SEE_OTHER);
    let location = callback
        .headers()
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    let redirect = Url::parse(location).unwrap();
    let code = redirect
        .query_pairs()
        .find_map(|(key, value)| (key == "code").then(|| value.into_owned()))
        .expect("authorization code in redirect");

    let token = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/token")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(format!(
                    "grant_type=authorization_code&code={code}&client_id=client&redirect_uri=http://127.0.0.1:7777/callback&code_verifier=verifier"
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(token.status(), StatusCode::OK);
    let body = axum::body::to_bytes(token.into_body(), usize::MAX)
        .await
        .unwrap();
    let response: TokenResponse = serde_json::from_slice(&body).unwrap();

    assert_eq!(response.scope, "axon:read axon:write");
}

#[tokio::test]
async fn oauth_client_allowlisted_non_admin_keeps_requested_read_only_scope() {
    let mut config = test_auth_config();
    config.admin_email = "admin@example.com".to_string();
    config.scopes_supported = vec!["axon:read".to_string(), "axon:write".to_string()];
    config.default_scope = "axon:read axon:write".to_string();
    let base_state = test_auth_state_with_config(config).await;

    base_state
        .store
        .register_client(RegisteredClient {
            client_id: "client".to_string(),
            redirect_uris: vec!["http://127.0.0.1:7777/callback".to_string()],
            created_at: now_unix(),
        })
        .await
        .unwrap();
    base_state
        .store
        .add_allowed_user("user@example.com", "admin", now_unix())
        .await
        .unwrap();

    let state = state_with_mock_google_from(&base_state).await;
    state
        .store
        .insert_authorization_request(AuthorizationRequestRow {
            state: "non-admin-read-only-state".to_string(),
            client_id: "client".to_string(),
            redirect_uri: "http://127.0.0.1:7777/callback".to_string(),
            client_state: "client-non-admin".to_string(),
            scope: "axon:read".to_string(),
            provider_code_verifier: "provider-verifier".to_string(),
            code_challenge: pkce_challenge("verifier"),
            code_challenge_method: "S256".to_string(),
            created_at: now_unix(),
            expires_at: now_unix() + 300,
        })
        .await
        .unwrap();

    let app = router(state);
    let callback = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/auth/google/callback?state=non-admin-read-only-state&code=upstream-code")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(callback.status(), StatusCode::SEE_OTHER);
    let location = callback
        .headers()
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    let redirect = Url::parse(location).unwrap();
    let code = redirect
        .query_pairs()
        .find_map(|(key, value)| (key == "code").then(|| value.into_owned()))
        .expect("authorization code in redirect");

    let token = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/token")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(format!(
                    "grant_type=authorization_code&code={code}&client_id=client&redirect_uri=http://127.0.0.1:7777/callback&code_verifier=verifier"
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(token.status(), StatusCode::OK);
    let body = axum::body::to_bytes(token.into_body(), usize::MAX)
        .await
        .unwrap();
    let response: TokenResponse = serde_json::from_slice(&body).unwrap();

    assert_eq!(response.scope, "axon:read");
}

/// Email not in admin or allowed_users must be rejected in the browser-login
/// branch (401 Unauthorized).
#[tokio::test]
async fn browser_login_rejects_email_absent_from_both_admin_and_db() {
    let mut config = test_auth_config();
    // Neither admin nor allowed_users contains "user@example.com" (the id_token email).
    config.admin_email = "admin@example.com".to_string();
    let base_state = test_auth_state_with_config(config).await;

    let state = state_with_mock_google_from(&base_state).await;

    state
        .store
        .insert_browser_login_state(BrowserLoginStateRow {
            state: "browser-state-3".to_string(),
            return_to: "/".to_string(),
            provider_code_verifier: "provider-verifier".to_string(),
            created_at: now_unix(),
            expires_at: now_unix() + 300,
        })
        .await
        .unwrap();

    let app = router(state);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/auth/google/callback?state=browser-state-3&code=upstream-code")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/// Admin also in the DB table must not appear twice (dedup check via
/// resolve_allowed_emails, verified indirectly: the callback still succeeds
/// and there is no panic from duplicate iteration).
#[tokio::test]
async fn admin_in_db_table_is_deduped_and_still_authorised() {
    // Default config: admin_email = "user@example.com".
    let base_state = test_auth_state_with_mock_google().await;

    // Also add the admin email to allowed_users — this is the duplicate.
    base_state
        .store
        .add_allowed_user("user@example.com", "self", now_unix())
        .await
        .unwrap();

    // Seed browser-login state.
    base_state
        .store
        .insert_browser_login_state(BrowserLoginStateRow {
            state: "browser-state-4".to_string(),
            return_to: "/".to_string(),
            provider_code_verifier: "provider-verifier".to_string(),
            created_at: now_unix(),
            expires_at: now_unix() + 300,
        })
        .await
        .unwrap();

    let app = router(base_state);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/auth/google/callback?state=browser-state-4&code=upstream-code")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Must still succeed — dedup should not break the check.
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert!(response.headers().contains_key(header::SET_COOKIE));
}
